//! `validate` (`config-catalog.ts:138`): o valor normalizado de uma chave do catálogo, ou a mensagem para o dono.
//! O `hades_core::config::save` grava só com `trim`; o painel valida antes de gravar, como o `save` do TS.

use std::sync::LazyLock;

use hades_core::config::{ConfigError, Item, size_bytes};
use regex::Regex;

/// `W` (`config-catalog.ts:12`): letra, número e `_` em qualquer escrita.
const W: &str = r"\p{L}\p{N}_";
const MIN_SIZE: f64 = 64.0 * 1024.0 * 1024.0;
const MAX_PERCENT: u32 = 400;

/// `HOST_PATTERN` (`gate-remote/protocol.ts:38`).
static HOST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[A-Za-z0-9_][A-Za-z0-9._-]*@)?[A-Za-z0-9][A-Za-z0-9._-]*$").expect("regex")
});
/// `LABEL_PATTERN` (`gate-remote/protocol.ts:39`).
static LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]*$").expect("regex"));

/// `fullMatch` (`config-catalog.ts:55`): o padrão casa o valor inteiro.
fn full_match(pattern: &str, value: &str) -> bool {
    Regex::new(&format!("^(?:{pattern})$")).is_ok_and(|re| re.is_match(value))
}

/// `String.prototype.trim` do JS: espaço Unicode, BOM e quebras de linha; o NEL (U+0085) fica.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// `commaList` (`config-catalog.ts:131`).
fn comma_list(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(js_trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// `isHost` (`gate-remote/protocol.ts:57`).
fn is_host(value: &str) -> bool {
    HOST.is_match(value)
}

/// `parseWorkers` (`gate-remote/protocol.ts:69`), só a validade.
fn workers_ok(raw: &str) -> bool {
    raw.split(',')
        .map(js_trim)
        .filter(|p| !p.is_empty())
        .all(|part| {
            let (host, labels) = part.split_once(':').unwrap_or((part, ""));
            is_host(host)
                && labels
                    .split('+')
                    .filter(|l| !l.is_empty())
                    .all(|l| LABEL.is_match(l))
        })
}

/// `Number.parseInt(v, 10)` de um valor que já casou `[1-9][0-9]*…`: os dígitos do começo.
fn leading_int(v: &str) -> u64 {
    let end = v.find(|c: char| !c.is_ascii_digit()).unwrap_or(v.len());
    v[..end].parse().unwrap_or(u64::MAX)
}

/// `validate` (`config-catalog.ts:138`).
pub fn validate(item: &Item, value: &str) -> Result<String, ConfigError> {
    let fail = |message: String| Err(ConfigError(format!("{}: {message}", item.label)));
    let mut v = js_trim(value).to_owned();
    let t = item.kind.as_str();
    match t {
        "size" => {
            v = v.to_uppercase().replace("MB", "M").replace("GB", "G");
            if !full_match("[1-9][0-9]{0,6}[MG]", &v)
                || size_bytes(&v).is_some_and(|b| b < MIN_SIZE)
            {
                return fail("use um tamanho como 512M ou 4G (mínimo 64M)".into());
            }
        }
        _ if t.starts_with("int:") => {
            let mut bounds = t.split(':').skip(1).map(|b| b.parse::<i128>());
            let (Some(Ok(lo)), Some(Ok(hi))) = (bounds.next(), bounds.next()) else {
                return Err(ConfigError(format!(
                    "{}: tipo desconhecido no catálogo ({t})",
                    item.key
                )));
            };
            let n = full_match("-?[0-9]+", &v)
                .then(|| v.parse::<i128>().ok())
                .flatten()
                .filter(|n| (lo..=hi).contains(n));
            let Some(n) = n else {
                return fail(format!("número inteiro entre {lo} e {hi}"));
            };
            v = n.to_string();
        }
        "bool" => {
            v = match v.to_lowercase().as_str() {
                "true" | "on" | "sim" => "1".into(),
                "false" | "off" | "não" => "0".into(),
                _ => v,
            };
            if v != "0" && v != "1" {
                return fail("0 ou 1".into());
            }
        }
        "percent" => {
            if !full_match("[1-9][0-9]{0,2}%", &v) || leading_int(&v) > u64::from(MAX_PERCENT) {
                return fail("porcentagem como 5% (até 400%)".into());
            }
        }
        _ if t.starts_with("enum:") => {
            let opts: Vec<&str> = t[5..].split(',').collect();
            if !opts.contains(&v.as_str()) {
                return fail(format!("um de {}", opts.join(", ")));
            }
        }
        _ if t.starts_with("multi:") => {
            let opts: Vec<&str> = t[6..].split(',').collect();
            let sel = comma_list(&v);
            if sel.is_empty() || sel.iter().any(|x| !opts.contains(x)) {
                return fail(format!("um ou mais de {}", opts.join(", ")));
            }
            v = opts
                .iter()
                .filter(|o| sel.contains(o))
                .copied()
                .collect::<Vec<_>>()
                .join(",");
        }
        "globs" => {
            let sel = comma_list(&v);
            if sel.is_empty() || sel.iter().any(|x| !full_match(&format!("[{W}*?.-]+"), x)) {
                return fail(
                    "padrões de nome separados por vírgula (letras, números, * e ?)".into(),
                );
            }
            v = sel.join(",");
        }
        "ttl" => {
            if !full_match("[1-9][0-9]{0,3}[hd]", &v) {
                return fail("duração como 8h ou 1d".into());
            }
        }
        "login" => {
            if !full_match("[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})|", &v) {
                return fail("login do GitHub (letras, números e hífen) ou vazio".into());
            }
        }
        "logins" | "emails" | "tags" => {
            let (pattern, what) = match t {
                "logins" => (
                    "[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})".to_owned(),
                    "contas do GitHub",
                ),
                "emails" => (format!(r"[{W}.+-]+@[{W}-]+(?:\.[{W}-]+)+"), "e-mails"),
                _ => (format!("[{W}-]+(?: [{W}-]+)*"), "palavras"),
            };
            let sel = comma_list(&v);
            if sel.iter().any(|x| !full_match(&pattern, x)) {
                return fail(format!("lista separada por vírgula ({what})"));
            }
            v = sel.join(",");
        }
        "modelmap" => {
            let pairs = comma_list(&v);
            if pairs.is_empty()
                || pairs
                    .iter()
                    .any(|x| !full_match("(default|[a-z0-9-]+)=(inherit|haiku|sonnet|opus)", x))
            {
                return fail(
                    "lista nome=modelo (modelo: inherit, haiku, sonnet, opus), com default=…"
                        .into(),
                );
            }
            v = pairs.join(",");
        }
        "where" => {
            if v != "local" && v != "auto" && !is_host(&v) {
                return fail("local, auto ou o host do worker (user@host)".into());
            }
        }
        "workers" => {
            if !workers_ok(&v) {
                return fail(
                    "host[:rotulo+rotulo], separados por vírgula (user@host:docker+mem16)".into(),
                );
            }
        }
        "id" => {
            if !full_match("[A-Za-z0-9_:-]{0,64}", &v) {
                return fail("letras, números, _ : -".into());
            }
        }
        "digits" => {
            if !full_match("[0-9]{0,20}", &v) {
                return fail("só dígitos".into());
            }
        }
        "path" => {
            if !v.is_empty() && !full_match(r#"(~|/)[^\s"'$`\\]*"#, &v) {
                return fail("caminho absoluto ou começando por ~/, sem espaço nem aspas".into());
            }
        }
        "secret" => {
            if !full_match(&format!("[{W}.:-]{{0,128}}"), &v) {
                return fail("letras, números e . _ : -".into());
            }
        }
        "relpath" => {
            if !full_match(&format!("[{W}.-]+(?:/[{W}.-]+)*"), &v)
                || v.split('/').any(|p| p == "..")
            {
                return fail("caminho relativo, sem .. nem espaço".into());
            }
        }
        "cidr" => {
            if !v.is_empty() && !full_match(r"(?:\d{1,3}\.){3}\d{1,3}/\d{1,2}", &v) {
                return fail("sub-rede IPv4 como 172.18.0.0/16, ou vazio".into());
            }
        }
        "port" => {
            let porta = full_match("[0-9]{4,5}", &v) && (1024..=65535).contains(&leading_int(&v));
            if !v.is_empty() && !porta {
                return fail("porta entre 1024 e 65535, ou vazio".into());
            }
        }
        "repo" => {
            if !full_match("[A-Za-z0-9._-]+/[A-Za-z0-9._-]+", &v) {
                return fail("dono/nome do repositório no GitHub".into());
            }
        }
        "url" => {
            if !full_match(
                &format!(r"https?://[{W}.-]+(?::[0-9]{{1,5}})?(?:/[{W}./~-]*)?"),
                &v,
            ) {
                return fail("endereço http:// ou https:// sem espaço nem parâmetros".into());
            }
        }
        "hours" => {
            let ok = full_match(
                "([01][0-9]|2[0-3]):([0-5][0-9])-([01][0-9]|2[0-3]):([0-5][0-9])|",
                &v,
            );
            if !ok || (!v.is_empty() && v.get(..5) == v.get(6..)) {
                return fail("HH:MM-HH:MM com horas diferentes (ex.: 22:00-07:00) ou vazio".into());
            }
        }
        _ => {
            return Err(ConfigError(format!(
                "{}: tipo desconhecido no catálogo ({t})",
                item.key
            )));
        }
    }
    Ok(v)
}
