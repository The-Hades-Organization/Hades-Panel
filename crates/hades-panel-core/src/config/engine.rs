//! Motor de desenho do canvas (`panel/drawing-engine.ts`): a extensão Cardinal pelo manifesto, o binário e o build web.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use hades_core::Env;
use hades_core::config::Effective;
use hades_core::fsx::{is_file, read_text};
use hades_core::paths::skill_root;
use hades_core::proc::{RunOptions, run, which};
use regex::Regex;
use serde_json::{Value, json};

use super::validate::js_trim;

const ENGINE_KEY: &str = "HADES_DRAWING_ENGINE";
const WEB_KEY: &str = "HADES_CARDINAL_WEB";
const TIMEOUT: Duration = Duration::from_secs(5);
const VERSION_SCAN: usize = 4096;

static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\d+\.\d+").expect("regex da versão"));
static RELATIVE_BIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").expect("regex do binário"));

/// `{code, message}` do motivo de o Cardinal não servir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub code: &'static str,
    pub message: String,
}

impl Reason {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// `DrawingEngine` (`drawing-engine.ts`): motor escolhido, motor em uso e se o Cardinal serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub configured: &'static str,
    pub effective: &'static str,
    pub usable: bool,
    pub reason: Option<Reason>,
    pub warn: bool,
}

impl Engine {
    pub fn to_json(&self) -> Value {
        json!({
            "configured": self.configured,
            "effective": self.effective,
            "usable": self.usable,
            "reason": self.reason.as_ref().map(|r| json!({"code": r.code, "message": r.message})),
            "warn": self.warn,
        })
    }
}

struct Manifest {
    names: Vec<String>,
    config_key: Option<String>,
    version: Vec<String>,
    min: String,
    health: Vec<String>,
    enable_key: Option<String>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn manifest(env: &Env) -> Option<Manifest> {
    let path = skill_root(env)
        .join("extensions")
        .join("cardinal")
        .join("extension.json");
    let json: Value = serde_json::from_str(&read_text(&path)?).ok()?;
    let text = |v: &Value| v.as_str().map(str::to_owned);
    Some(Manifest {
        names: strings(&json["binary"]["names"]),
        config_key: text(&json["binary"]["configKey"]),
        version: strings(&json["version"]["command"]),
        min: text(&json["version"]["min"]).unwrap_or_default(),
        health: strings(&json["health"]["command"]),
        enable_key: text(&json["enableKey"]),
    })
}

/// `~` e `~/x` pelo HOME do ambiente.
fn expand_home(env: &Env, value: &str) -> PathBuf {
    match value.strip_prefix('~') {
        Some("") => env.home(),
        Some(rest) if rest.starts_with('/') => env.home().join(&rest[1..]),
        _ => PathBuf::from(value),
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `resolveBinary`: a chave de config com valor manda (relativo pelo PATH, absoluto se for executável); senão os
/// nomes do manifesto no PATH. O TS lança no absoluto não executável e no relativo fora do padrão; aqui é ausência.
fn resolve_binary(env: &Env, config: &Config, m: &Manifest) -> Option<PathBuf> {
    let path_var = env.get_raw("PATH").unwrap_or_default();
    let configured = m
        .config_key
        .as_deref()
        .and_then(|k| config.get(k))
        .map(js_trim)
        .unwrap_or_default();
    if configured.is_empty() {
        return m.names.iter().find_map(|n| which(n, path_var));
    }
    let expanded = expand_home(env, configured);
    if expanded.is_absolute() {
        return is_executable(&expanded).then_some(expanded);
    }
    let name = expanded.to_string_lossy();
    if RELATIVE_BIN.is_match(&name) {
        which(&name, path_var)
    } else {
        None
    }
}

fn semver(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn run_with(bin: &Path, command: &[String]) -> hades_core::proc::RunResult {
    let args: Vec<&str> = command.iter().skip(1).map(String::as_str).collect();
    let options = RunOptions {
        timeout: Some(TIMEOUT),
        ..RunOptions::default()
    };
    run(&bin.to_string_lossy(), &args, &options)
}

/// `statusOf`: os problemas do binário, na ordem em que o painel os mostra.
fn problems(env: &Env, config: &Config, m: &Manifest) -> Vec<Reason> {
    let Some(bin) = resolve_binary(env, config, m) else {
        return vec![missing(m)];
    };
    let mut found = Vec::new();
    let r = run_with(&bin, &m.version);
    let both = format!("{}{}", r.stdout, r.stderr);
    let head: String = both.chars().take(VERSION_SCAN).collect();
    match VERSION.find(&head).map(|v| v.as_str().to_owned()) {
        None => found.push(Reason::new(
            "ext.version-unreadable",
            "não consegui ler a versão do binário",
        )),
        Some(v) if semver(&v) < semver(&m.min) => found.push(Reason::new(
            "ext.version-below-min",
            format!("versão {v} abaixo da mínima {}", m.min),
        )),
        Some(_) => {}
    }
    if !m.health.is_empty() && run_with(&bin, &m.health).status != 0 {
        found.push(Reason::new(
            "ext.health-failed",
            "o comando de saúde falhou",
        ));
    }
    found
}

fn missing(m: &Manifest) -> Reason {
    Reason::new(
        "ext.binary-missing",
        format!("binário não encontrado ({})", m.names.join(", ")),
    )
}

fn web_present(env: &Env, config: &Config) -> bool {
    let dir = config.get(WEB_KEY).map(js_trim).unwrap_or_default();
    !dir.is_empty() && is_file(&expand_home(env, dir).join("index.html"))
}

/// O `config` do motor: para cada item do catálogo, a variável do ambiente (vazia inclusive) ou o valor efetivo.
struct Config(Vec<(String, String)>);

impl Config {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// `drawingEngine`: o motor configurado e o que vale de fato, com o motivo quando o Cardinal não serve.
pub fn engine(env: &Env, items: &[Effective]) -> Engine {
    let config = Config(
        items
            .iter()
            .map(|e| {
                (
                    e.item.key.clone(),
                    env.get_raw(&e.item.key).unwrap_or(&e.value).to_owned(),
                )
            })
            .collect(),
    );
    let configured = if config.get(ENGINE_KEY) == Some("hades") {
        "hades"
    } else {
        "cardinal"
    };
    let m = manifest(env);
    let enabled = m
        .as_ref()
        .and_then(|m| m.enable_key.as_deref())
        .and_then(|k| config.get(k))
        != Some("0");
    let reason = match &m {
        None => Some(Reason::new(
            "engine.extension-missing",
            "a extensão cardinal não está nesta instalação",
        )),
        Some(_) if !enabled => Some(Reason::new(
            "engine.extension-off",
            "a extensão cardinal está desligada",
        )),
        Some(m) if !web_present(env, &config) => problems(env, &config, m).into_iter().next(),
        Some(_) => None,
    };
    let usable = reason.is_none();
    let effective = if configured == "cardinal" && usable {
        "cardinal"
    } else {
        "hades"
    };
    Engine {
        configured,
        effective,
        usable,
        warn: configured == "cardinal" && effective == "hades" && m.is_some() && enabled,
        reason,
    }
}
