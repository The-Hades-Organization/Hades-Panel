//! Rotas `log-files` e `log` (`panel/routes-misc.ts:89-152`): os `.log` de `$D` e a leitura de uma fonte de log
//! (`job:`, `file:`, `svc:`, `unit:`, `docker:` ou `doc:`).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

use crate::PanelEnv;
use crate::amostra::{Instancia, service_catalog};
use crate::leitura::PanelError;

/// `LOG_JOB_TAIL` (`routes-misc.ts:26`).
const LOG_JOB_TAIL: usize = 200_000;
/// `LOG_FILE_TAIL` (`routes-misc.ts:27`).
const LOG_FILE_TAIL: usize = 400_000;
/// `LOG_LINES_DEFAULT` (`routes-misc.ts:28`).
pub const LOG_LINES_DEFAULT: &str = "400";
/// `DOCS_ALLOWED` (`routes-misc.ts:42`).
pub const DOCS_ALLOWED: &[&str] = &[
    "STATUS.md",
    "notes.md",
    "conflicts.log",
    "rebuild.report",
    "prs.list",
];

/// `logSources` (`routes-misc.ts:89-99`): os `.log` de `$D/logs` e de `$D`, sem repetir, em ordem.
pub fn log_sources(env: &PanelEnv) -> Vec<String> {
    let mut names = BTreeSet::new();
    for dir in [env.data().join("logs"), env.data().to_path_buf()] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.filter_map(Result::ok) {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if name.ends_with(".log") && dir.join(&name).is_file() {
                names.insert(name);
            }
        }
    }
    names.into_iter().collect()
}

pub fn log_files_json(env: &PanelEnv) -> Value {
    json!(log_sources(env))
}

/// `pyInt` (`routes-misc.ts:46-49`): o `int()` do Python, com a mensagem do `ValueError`.
fn py_int(text: &str) -> Result<i64, PanelError> {
    let t = text.trim();
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PanelError::bad(format!(
            "invalid literal for int() with base 10: '{text}'"
        )));
    }
    Ok(t.parse::<i64>().unwrap_or(if t.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    }))
}

/// `stripAnsi` (`routes-misc.ts:22,52`): tira `ESC[…letra` e todo `\r`.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c == '\r' {
            continue;
        }
        if c == '\x1b' && text[i + 1..].starts_with('[') {
            let resto = &text[i + 2..];
            let corpo = resto
                .find(|ch: char| !(ch.is_ascii_digit() || ch == ';' || ch == '?'))
                .unwrap_or(resto.len());
            if resto[corpo..].starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                let fim = i + 2 + corpo + 1;
                while it.peek().is_some_and(|(j, _)| *j < fim) {
                    it.next();
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// `TextDecoder("utf-8").decode`: inválido vira U+FFFD e o BOM do começo cai.
fn decode(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.strip_prefix('\u{feff}').unwrap_or(&s).to_owned()
}

fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `slice(start)` do `Buffer`: negativo conta do fim, fora do tamanho fica no limite.
fn slice_from(data: &[u8], start: i64) -> &[u8] {
    let len = data.len() as i64;
    let s = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    &data[s as usize..]
}

/// `all.slice(-lines || undefined)`.
fn last_lines(all: &[&str], lines: i64) -> Vec<String> {
    let n = all.len() as i64;
    let start = match lines {
        0 => 0,
        l if l > 0 => (n - l).max(0),
        l => l.unsigned_abs().min(n as u64) as i64,
    };
    all[start as usize..]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// `split(/\r\n|\n|\r/)`.
fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' => {
                out.push(&text[start..i]);
                i += if b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                out.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push(&text[start..]);
    out
}

fn jobs_dir(env: &PanelEnv) -> PathBuf {
    env.data().join("logs").join("panel-jobs")
}

fn read_file(env: &PanelEnv, name: &str, lines: i64) -> Result<Value, PanelError> {
    let valid = name
        .strip_suffix(".log")
        .is_some_and(|b| !b.is_empty() && b.chars().all(|c| word(c) || ".@-".contains(c)));
    if !valid {
        return Err(PanelError::bad("arquivo inválido"));
    }
    let in_logs = env.data().join("logs").join(name);
    let path = if in_logs.exists() {
        in_logs
    } else {
        env.data().join(name)
    };
    let data = std::fs::read(&path).map_err(|_| {
        PanelError::bad(format!(
            "[Errno 2] No such file or directory: '{}'",
            path.display()
        ))
    })?;
    let text = decode(&data[data.len().saturating_sub(LOG_FILE_TAIL)..]);
    let mut all = split_lines(&text);
    if all.last() == Some(&"") {
        all.pop();
    }
    Ok(json!({ "text": strip_ansi(&last_lines(&all, lines).join("\n")) }))
}

/// `unitNamePattern` (`panel/plugin.ts:39`): `<hu>-[\w.-]+`.
fn unit_valida(hu: &str, name: &str) -> bool {
    name.strip_prefix(hu)
        .and_then(|r| r.strip_prefix('-'))
        .is_some_and(|r| !r.is_empty() && r.chars().all(|c| word(c) || c == '.' || c == '-'))
}

fn journal(name: &str, lines: i64) -> String {
    Command::new("journalctl")
        .args(["--user", "-u", name, "-n", &lines.to_string()])
        .args(["--no-pager", "-o", "short-iso"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn docker_logs(name: &str, lines: i64) -> String {
    Command::new("timeout")
        .args(["8", "docker", "logs", "--tail", &lines.to_string()])
        .args(["--timestamps", name])
        .output()
        .map(|o| {
            if o.status.code() == Some(124) {
                "(docker logs demorou demais)".to_owned()
            } else {
                let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
                t.push_str(&String::from_utf8_lossy(&o.stderr));
                strip_ansi(&t)
            }
        })
        .unwrap_or_default()
}

/// `readLog` (`routes-misc.ts:101-152`). `containers` são os nomes da amostra (`containerNames`).
pub fn read_log(
    env: &PanelEnv,
    src: &str,
    n: &str,
    offset: Option<&str>,
    containers: &[String],
) -> Result<Value, PanelError> {
    let (kind, name) = src.split_once(':').unwrap_or((src, ""));
    let lines = py_int(n)?;
    match kind {
        "job" => {
            if name.is_empty() || !name.chars().all(|c| word(c) || c == '-') {
                return Err(PanelError::bad("job inválido"));
            }
            let data = std::fs::read(jobs_dir(env).join(format!("{name}.log"))).unwrap_or_default();
            let start = match offset {
                Some(o) => py_int(o)?,
                None => data.len().saturating_sub(LOG_JOB_TAIL) as i64,
            };
            let text = strip_ansi(&decode(slice_from(&data, start)));
            Ok(json!({ "text": text, "offset": data.len() }))
        }
        "file" => read_file(env, name, lines),
        "svc" => {
            if !service_catalog(env.data()).iter().any(|s| s.name == name) {
                return Err(PanelError::bad("serviço inválido"));
            }
            read_file(env, &format!("{name}.log"), lines)
        }
        "unit" => {
            if !unit_valida(&Instancia::de(env).hu, name) {
                return Err(PanelError::bad("unit inválida"));
            }
            Ok(json!({ "text": journal(name, lines) }))
        }
        "docker" => {
            if !containers.iter().any(|c| c == name) {
                return Err(PanelError::bad("container inválido"));
            }
            Ok(json!({ "text": docker_logs(name, lines) }))
        }
        "doc" => {
            if !DOCS_ALLOWED.contains(&name) {
                return Err(PanelError::bad("documento inválido"));
            }
            let text = decode(&std::fs::read(env.data().join(name)).unwrap_or_default());
            let text = if text.is_empty() {
                "(vazio)".into()
            } else {
                text
            };
            Ok(json!({ "text": text }))
        }
        _ => Err(PanelError::bad("fonte inválida")),
    }
}

/// Caminho do `.log` de um job do painel (`jobs.dir`).
pub fn job_log(env: &PanelEnv, id: &str) -> PathBuf {
    jobs_dir(env).join(format!("{id}.log"))
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
