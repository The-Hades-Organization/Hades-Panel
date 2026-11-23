//! Configurações do painel (`panel/config.ts` sobre `lib/config-catalog.ts`): o catálogo com o valor efetivo, as
//! configurações herdadas do Claude Code, o export, o plano de importação, o `doctor` e a gravação validada do
//! `$D/config.env` sob o mesmo `flock` do TS.

mod engine;
mod validate;

use std::path::PathBuf;

use hades_core::Env;
use hades_core::config::{
    ConfigError, ConfigStore, Effective, OrderedMap, Sensitivity, catalog, effective, is_key, save,
    sensitivity, size_bytes, split_lines,
};
use hades_core::fsx::read_text;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::{Map, Value, json};

pub use engine::{Engine, Reason, engine};
pub use validate::validate;

use crate::PanelEnv;

const ENGINE_KEY: &str = "HADES_DRAWING_ENGINE";
const SLICE_HIGH: &str = "HADES_SLICE_HIGH";
const SLICE_MAX: &str = "HADES_SLICE_MAX";
const SLICE_ERROR: &str = "Pressão de memória (MemoryHigh) não pode passar do teto (MemoryMax)";
const HEIRS: [&str; 3] = [
    "sessões interativas",
    "agentes sem --model/--effort",
    "subagentes (modelo da sessão pai)",
];

/// O `store` do painel: `config.env` e o catálogo do kit lidos de `$D`.
pub fn store(env: &PanelEnv) -> ConfigStore {
    ConfigStore {
        dir: env.data().to_owned(),
        kit: env.data().to_owned(),
    }
}

/// Um item do catálogo como o JSON do painel: `plugin` só aparece no item que veio do plugin.
pub fn item_json(e: &Effective) -> Value {
    let mut m = Map::new();
    for (k, v) in [
        ("key", &e.item.key),
        ("default", &e.item.default),
        ("type", &e.item.kind),
        ("group", &e.item.group),
        ("label", &e.item.label),
        ("apply", &e.item.apply),
        ("desc", &e.item.desc),
    ] {
        m.insert(k.to_owned(), Value::String(v.clone()));
    }
    if e.item.plugin {
        m.insert("plugin".to_owned(), Value::Bool(true));
    }
    m.insert("value".to_owned(), Value::String(e.value.clone()));
    m.insert("changed".to_owned(), Value::Bool(e.changed));
    Value::Object(m)
}

/// `visibleItems`: a escolha do motor some enquanto o Cardinal não serve, salvo se o dono já a mudou.
pub fn visible_items<'a>(items: &'a [Effective], engine: &Engine) -> Vec<&'a Effective> {
    items
        .iter()
        .filter(|e| e.item.key != ENGINE_KEY || engine.usable || e.changed)
        .collect()
}

/// Rota `config`: `{items, path, engine}`.
pub fn state_json(env: &PanelEnv) -> Result<Value, ConfigError> {
    let store = store(env);
    let items = effective(&store)?;
    let eng = engine(env.env(), &items);
    let visible: Vec<Value> = visible_items(&items, &eng)
        .into_iter()
        .map(item_json)
        .collect();
    Ok(json!({
        "items": visible,
        "path": store.dir.join("config.env").to_string_lossy(),
        "engine": eng.to_json(),
    }))
}

fn trimmed_str(v: &Value) -> String {
    v.as_str()
        .map(|s| validate::js_trim(s).to_owned())
        .unwrap_or_default()
}

/// Rota `config/inherited`: modelo e esforço do `settings.json` do Claude Code, herdados por quem não os fixa.
pub fn inherited_json(env: &Env) -> Value {
    let path = env
        .get("HADES_CLAUDE_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".claude").join("settings.json"));
    let fail = |reason: String| json!({"ok": false, "reason": reason});
    let Some(text) = read_text(&path) else {
        return fail("arquivo ausente ou ilegível".to_owned());
    };
    let json: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return fail(format!("JSON inválido ({e})")),
    };
    if !json.is_object() {
        return fail("o conteúdo não é um objeto JSON".to_owned());
    }
    json!({
        "ok": true,
        "model": trimmed_str(&json["model"]),
        "effort": trimmed_str(&json["effortLevel"]),
        "heirs": HEIRS,
    })
}

/// Versão da skill no cabeçalho do export: `VERSION` ao lado do kit, depois a da skill instalada.
fn skill_version(env: &Env, store: &ConfigStore) -> String {
    [
        store.kit.join("..").join("VERSION"),
        env.home()
            .join(".claude")
            .join("skills")
            .join("hades")
            .join("VERSION"),
    ]
    .iter()
    .find_map(|p| read_text(p))
    .map(|t| validate::js_trim(&t).to_owned())
    .unwrap_or_else(|| "desconhecida".to_owned())
}

/// `YYYY-MM-DD HH:MM` no fuso do ambiente (`TZ`), senão no do sistema.
fn local_minute(env: &Env, now_ms: i64) -> String {
    let tz = env
        .get("TZ")
        .and_then(|name| TimeZone::get(name.trim_start_matches(':')).ok())
        .unwrap_or_else(TimeZone::system);
    Timestamp::from_millisecond(now_ms)
        .map(|t| t.to_zoned(tz).strftime("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Opções do export: `all` inclui o que está no padrão; `identity` inclui contas e caminhos desta máquina.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportOptions {
    pub all: bool,
    pub identity: bool,
}

/// `exportText` (`config-catalog.ts:270`): `KEY=valor` do dono, sem segredo e (por padrão) sem identidade.
pub fn export_text(
    env: &Env,
    store: &ConfigStore,
    opts: ExportOptions,
    now_ms: i64,
) -> Result<String, ConfigError> {
    let mut body = String::new();
    let mut skipped = 0;
    for e in effective(store)? {
        if !e.changed && !opts.all {
            continue;
        }
        match sensitivity(&e.item) {
            Sensitivity::Secret => skipped += 1,
            Sensitivity::Identity if !opts.identity => skipped += 1,
            _ => body.push_str(&format!("{}={}\n", e.item.key, e.value)),
        }
    }
    let scope = if opts.all {
        "tudo"
    } else {
        "só o que difere do padrão"
    };
    let identity = if opts.identity {
        "; com contas e caminhos desta máquina"
    } else {
        ""
    };
    Ok(format!(
        "# Configurações do Hades — skill {} — exportado em {}\n# Valores do dono ({scope}{identity}). Importar: \
         config.sh import <arquivo> [--dry-run]\n# {skipped} chave(s) de segredo ou desta máquina ficaram de fora \
         (--include-identity inclui contas e caminhos).\n{body}",
        skill_version(env, store),
        local_minute(env, now_ms),
    ))
}

/// Rota `doctor`: o `$D/doctor.json` com `mtime` em segundos; sem o arquivo, `{}`.
pub fn doctor_json(env: &PanelEnv) -> Result<Value, ConfigError> {
    let path = env.data().join("doctor.json");
    let Some(text) = read_text(&path) else {
        return Ok(json!({}));
    };
    let parsed: Value =
        serde_json::from_str(&text).map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
    let mut out = match parsed {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or_default();
    out.insert("mtime".to_owned(), json!(mtime));
    Ok(Value::Object(out))
}

/// Uma mudança do plano de importação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub key: String,
    pub label: String,
    pub group: String,
    pub apply: String,
    pub old: String,
    pub new: String,
}

/// `ImportPlan` (`config-catalog.ts:292`): o que um `KEY=VALUE` colado faria, sem gravar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub changes: Vec<Change>,
    pub same: Vec<String>,
    pub invalid: Vec<(String, String)>,
    pub unknown: Vec<String>,
    pub secret: Vec<String>,
    pub blocked: Vec<String>,
    pub apply: Vec<String>,
}

impl ImportPlan {
    pub fn to_json(&self) -> Value {
        json!({
            "changes": self.changes.iter().map(|c| json!({
                "key": c.key, "label": c.label, "group": c.group, "apply": c.apply, "old": c.old, "new": c.new,
            })).collect::<Vec<_>>(),
            "same": self.same,
            "invalid": self.invalid.iter().map(|(k, e)| json!({"key": k, "error": e})).collect::<Vec<_>>(),
            "unknown": self.unknown,
            "secret": self.secret,
            "blocked": self.blocked,
            "apply": self.apply,
        })
    }
}

/// Pares `KEY=VALUE` do texto: a chave repetida fica na posição da primeira com o último valor.
fn parse_pairs(text: &str, invalid: &mut Vec<(String, String)>) -> OrderedMap {
    let mut pairs = OrderedMap::default();
    for (index, raw) in split_lines(text).iter().enumerate() {
        let line = validate::js_trim(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.find('=').filter(|&eq| is_key(&line[..eq])) {
            Some(eq) => pairs.set(&line[..eq], &line[eq + 1..]),
            None => invalid.push((
                format!("linha {}", index + 1),
                "esperado KEY=VALUE".to_owned(),
            )),
        }
    }
    pairs
}

/// `plan` (`config-catalog.ts:292`): o preview da importação. `local_only` são as chaves que o painel não deixa
/// importar (vazio no painel gpui).
pub fn plan(
    store: &ConfigStore,
    text: &str,
    local_only: &[&str],
) -> Result<ImportPlan, ConfigError> {
    let items = effective(store)?;
    let mut p = ImportPlan::default();
    let pairs = parse_pairs(text, &mut p.invalid);
    for (key, raw) in pairs.iter() {
        let Some(e) = items.iter().find(|e| e.item.key == key) else {
            p.unknown.push(key.to_owned());
            continue;
        };
        if sensitivity(&e.item) == Sensitivity::Secret {
            p.secret.push(key.to_owned());
            continue;
        }
        if local_only.contains(&key) {
            p.blocked.push(key.to_owned());
            continue;
        }
        match validate(&e.item, raw) {
            Err(err) => p.invalid.push((key.to_owned(), err.0)),
            Ok(new) if new == e.value => p.same.push(key.to_owned()),
            Ok(new) => p.changes.push(Change {
                key: key.to_owned(),
                label: e.item.label.clone(),
                group: e.item.group.clone(),
                apply: e.item.apply.clone(),
                old: e.value.clone(),
                new,
            }),
        }
    }
    let after = |key: &str| {
        p.changes
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.new.clone())
            .or_else(|| {
                items
                    .iter()
                    .find(|e| e.item.key == key)
                    .map(|e| e.value.clone())
            })
            .and_then(|v| size_bytes(&v))
    };
    if let (Some(high), Some(max)) = (after(SLICE_HIGH), after(SLICE_MAX))
        && high > max
    {
        p.invalid
            .push((SLICE_HIGH.to_owned(), SLICE_ERROR.to_owned()));
        p.changes
            .retain(|c| c.key != SLICE_HIGH && c.key != SLICE_MAX);
    }
    p.apply = p.changes.iter().map(|c| c.apply.clone()).collect();
    p.apply.sort();
    p.apply.dedup();
    Ok(p)
}

/// Gravação do painel: valida cada valor pelo tipo do catálogo e grava pelo `save` do `hades-core`, que roda o ciclo
/// ler-alterar-gravar sob `flock` em `config.env.lock` (o mesmo lock do `save` do TS). Devolve os modos de aplicar.
pub fn save_validated(
    store: &ConfigStore,
    updates: &[(&str, &str)],
    unset: &[&str],
) -> Result<Vec<String>, ConfigError> {
    let items = catalog(store)?;
    let mut valid = Vec::with_capacity(updates.len());
    for (key, value) in updates {
        let item = items
            .iter()
            .find(|i| i.key == *key)
            .ok_or_else(|| ConfigError(format!("chave desconhecida: {key}")))?;
        valid.push((*key, validate(item, value)?));
    }
    let pairs: Vec<(&str, &str)> = valid.iter().map(|(k, v)| (*k, v.as_str())).collect();
    save(store, &pairs, unset)
}

#[cfg(test)]
mod tests;
