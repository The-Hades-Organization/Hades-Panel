//! `sample` (`panel/sampler.ts:293-390`): a amostra única do painel (rota `snapshot`), sem `history`.
//!
//! Os serviços vivos (systemd, docker, HTTP local, git, herdr, jobs) ficam atrás de [`Fontes`]; com
//! `HADES_PANEL_NO_INFRA=1` o painel não consulta nenhum deles e a amostra sai só do disco e do `/proc`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use hades_core::config::read_config_env;
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::fila::estado::mtime_of;
use crate::fila::procs::{fields, load_average, meminfo, read_lines, read_text};
use crate::fila::tempo::{js_num, js_opt};
use crate::fila::{gate_queue, locks_dir};
use crate::leitura::{config_values, worktree};

/// `DEFAULT_BASE` (`panel/sampler.ts`).
pub const DEFAULT_BASE: &str = "origin/develop";

/// Propriedades de uma unit do systemd (`ActiveState`, `SubState`, `Result`, `NRestarts`, `MemoryCurrent`,
/// `CPUUsageNSec`, `ActiveEnterTimestampMonotonic`).
pub type Unit = HashMap<String, String>;

/// O que a amostra pergunta a serviços vivos.
pub trait Fontes {
    /// `units(names)`: as propriedades de cada unit, pelo nome completo (`hds-<ms>`).
    fn units(&self, names: &[String]) -> HashMap<String, Unit>;
    /// `enabledServices()`: os serviços habilitados.
    fn enabled_services(&self) -> HashSet<String>;
    /// Status HTTP de uma URL local, `None` sem resposta.
    fn http_status(&self, url: &str) -> Option<i64>;
    /// `containers()`: as linhas do `docker ps` do projeto.
    fn containers(&self) -> Vec<Value>;
    /// `sliceUsage()`: `{used, max, high}` da slice.
    fn slice_usage(&self) -> Value;
    /// `jobs.list()`.
    fn jobs(&self) -> Vec<Value>;
    /// `skillProcs()`.
    fn skill_procs(&self) -> Vec<Value>;
    /// `git -C <W> <args>`, `""` em falha.
    fn git_out(&self, args: &[&str]) -> String;
    /// `herdr.badge()`.
    fn herdr_badge(&self) -> Value;
}

/// Sem infraestrutura (`HADES_PANEL_NO_INFRA=1`): nada responde.
pub struct SemInfra;

impl Fontes for SemInfra {
    fn units(&self, _: &[String]) -> HashMap<String, Unit> {
        HashMap::new()
    }
    fn enabled_services(&self) -> HashSet<String> {
        HashSet::new()
    }
    fn http_status(&self, _: &str) -> Option<i64> {
        None
    }
    fn containers(&self) -> Vec<Value> {
        Vec::new()
    }
    fn slice_usage(&self) -> Value {
        json!({ "used": 0, "max": null, "high": null })
    }
    fn jobs(&self) -> Vec<Value> {
        Vec::new()
    }
    fn skill_procs(&self) -> Vec<Value> {
        Vec::new()
    }
    fn git_out(&self, _: &[&str]) -> String {
        String::new()
    }
    fn herdr_badge(&self) -> Value {
        Value::Null
    }
}

/// `infraOff()`: `HADES_PANEL_NO_INFRA=1`.
pub fn infra_off(env: &PanelEnv) -> bool {
    env.env().get("HADES_PANEL_NO_INFRA") == Some("1")
}

/// Identidade da instância (`panel/env.ts:44-66`).
#[derive(Debug, PartialEq)]
pub struct Instancia {
    pub id: String,
    pub hu: String,
    pub slice: String,
    pub project: String,
    pub tunnel_id: String,
}

impl Instancia {
    /// `HADES_ID` do ambiente, senão o do `config.env` de `$D`; fora de `[a-z0-9_]{0,24}` é saneado.
    pub fn de(env: &PanelEnv) -> Self {
        let raw = env
            .env()
            .get("HADES_ID")
            .map(str::to_owned)
            .or_else(|| {
                read_config_env(env.data())
                    .get("HADES_ID")
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        Self::com_id(&raw)
    }

    pub fn com_id(raw: &str) -> Self {
        let valid = raw.len() <= 24
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        let id: String = if valid {
            raw.to_owned()
        } else {
            raw.to_lowercase()
                .chars()
                .map(|c| {
                    if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(24)
                .collect()
        };
        let suf = |p: &str| {
            if id.is_empty() {
                p.to_owned()
            } else {
                format!("{p}_{id}")
            }
        };
        Self {
            hu: suf("hds"),
            slice: suf("hades"),
            project: suf("hades"),
            tunnel_id: if id.is_empty() {
                String::new()
            } else {
                format!("-{}", id.replace('_', "-"))
            },
            id,
        }
    }
}

/// `serviceCatalog` (`panel/system.ts:165-173`): `services.tsv` de `$D`.
pub struct Servico {
    pub name: String,
    pub port: Option<i64>,
    pub redis: bool,
    pub db: bool,
}

pub fn service_catalog(dir: &Path) -> Vec<Servico> {
    let mut out: Vec<Servico> = Vec::new();
    for line in read_lines(&dir.join("services.tsv")) {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let row = Servico {
            name: f[0].to_owned(),
            port: crate::fila::procs::parse_int(f[1]),
            redis: f[3] != "-",
            db: f[5] != "-",
        };
        match out.iter_mut().find(|s| s.name == row.name) {
            Some(s) => *s = row,
            None => out.push(row),
        }
    }
    out
}

fn is_uint(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `unitView` (`sampler.ts:303-321`) de uma amostra só: sem CPU anterior, `cpu` é sempre `null`.
pub fn unit_view(u: Option<&Unit>, up: f64) -> Value {
    let empty = Unit::new();
    let u = u.unwrap_or(&empty);
    let get = |k: &str| u.get(k).map(String::as_str);
    let active = get("ActiveState") == Some("active");
    let mem = get("MemoryCurrent").unwrap_or("");
    let mono = get("ActiveEnterTimestampMonotonic").unwrap_or("0");
    let restarts = get("NRestarts").filter(|s| is_uint(s));
    let mono_n: f64 = if is_uint(mono) {
        mono.parse().unwrap_or(0.0)
    } else {
        0.0
    };
    json!({
        "state": get("ActiveState").unwrap_or("inactive"),
        "sub": get("SubState").unwrap_or("dead"),
        "result": get("Result"),
        "restarts": restarts.map_or(Value::from(0), |s| js_num(s.parse().unwrap_or(0.0))),
        "mem": if is_uint(mem) && active { js_num(mem.parse().unwrap_or(0.0)) } else { Value::Null },
        "cpu": null,
        "age": if active && mono_n != 0.0 { js_num(up - mono_n / 1e6) } else { Value::Null },
    })
}

/// `overall` (`sampler.ts:360-364`).
pub fn overall(
    busy: bool,
    paused: bool,
    n_active: usize,
    n_ready: usize,
    n_enabled: usize,
    c_up: usize,
    n_long: usize,
) -> &'static str {
    if busy {
        "busy"
    } else if n_active == 0 && c_up <= 1 {
        if paused { "paused" } else { "stopped" }
    } else if n_ready == n_enabled && c_up + 1 >= n_long {
        "up"
    } else {
        "partial"
    }
}

fn uptime(env: &PanelEnv) -> f64 {
    read_text(&env.proc_path(["uptime"]))
        .and_then(|t| {
            t.split_whitespace()
                .next()
                .and_then(crate::fila::procs::parse_float)
        })
        .unwrap_or(0.0)
}

fn trimmed_lines(path: &Path) -> Vec<String> {
    read_lines(path)
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

fn opt_str(s: Option<&str>) -> Value {
    s.map_or(Value::Null, Value::from)
}

/// `branch()` (`sampler.ts:185-225`). A base é `DEFAULT_BASE`: o `project get branch.base` do TS não é portado.
pub fn branch(env: &PanelEnv, fontes: &dyn Fontes) -> Value {
    let dir = env.data();
    let w = worktree(env);
    let head_raw = fontes.git_out(&["log", "-1", "--format=%H%x09%cI%x09%s"]);
    let head: Vec<&str> = head_raw.trim().split('\t').collect();
    let name = fontes.git_out(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let sha = head.first().copied().filter(|s| !s.is_empty());
    let prs: Vec<Value> = read_lines(&dir.join("prs.tsv"))
        .iter()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            (f.len() >= 5).then(|| {
                json!({ "number": f[0], "sha": f[1].chars().take(10).collect::<String>(), "base": f[2],
                        "branch": f[3], "title": f[4] })
            })
        })
        .collect();
    let conflicts: Vec<String> = read_lines(&dir.join("conflicts.log"))
        .into_iter()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let base = DEFAULT_BASE;
    let dev_raw = fontes.git_out(&["log", "-1", "--format=%H%x09%cI%x09%s", base]);
    let dev: Vec<&str> = dev_raw.trim().split('\t').collect();
    let ahead_raw = fontes.git_out(&["rev-list", "--count", &format!("HEAD..{base}")]);
    let ahead = ahead_raw.trim();
    let fetch = fontes.git_out(&["rev-parse", "--git-path", "FETCH_HEAD"]);
    let fetch = fetch.trim();
    let fetch_head = if fetch.is_empty() {
        Path::new("/nonexistent").to_path_buf()
    } else if Path::new(fetch).is_absolute() {
        Path::new(fetch).to_path_buf()
    } else {
        w.join(fetch)
    };
    let web_paths = trimmed_lines(&dir.join("web.paths"));
    let mut svc_paths = trimmed_lines(&dir.join("services.paths"));
    if svc_paths.is_empty() {
        svc_paths = ["apps", "libs"]
            .iter()
            .filter(|p| w.join(p).is_dir())
            .map(|p| (*p).to_owned())
            .collect();
    }
    let prs_list: Vec<String> = read_lines(&dir.join("prs.list"))
        .iter()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    let develop_only = prs_list.len() == 1 && prs_list[0] == "-0";
    let remote = fontes.git_out(&["config", "--get", "remote.origin.url"]);
    let repo = repo_of(remote.trim());
    let behind = |file: &str, paths: &[String]| -> Value {
        if paths.is_empty() {
            return json!({ "built": null, "stale": null, "paths": [] });
        }
        let built = read_text(&dir.join(file))
            .unwrap_or_default()
            .trim()
            .to_owned();
        let stale = match sha {
            Some(s) if !built.is_empty() && built == s => Value::Bool(false),
            _ => Value::Null,
        };
        json!({ "built": if built.is_empty() { Value::Null } else { Value::from(built) }, "stale": stale,
                "paths": paths })
    };
    let report: Map<String, Value> = fields(&dir.join("rebuild.report"))
        .into_iter()
        .map(|(k, v)| (k, Value::from(v)))
        .collect();
    let heads = read_text(&dir.join("logs").join("watch.last-diff"))
        .unwrap_or_default()
        .trim()
        .to_owned();
    let tail: Vec<&String> = conflicts
        .iter()
        .skip(conflicts.len().saturating_sub(4))
        .collect();
    json!({
        "name": name.trim(), "sha": opt_str(sha), "date": opt_str(head.get(1).copied()),
        "subject": opt_str(head.get(2).copied()),
        "services": behind("services-built.sha", &svc_paths),
        "web": behind("web-built.sha", &web_paths),
        "repo": opt_str(repo.as_deref()),
        "developOnly": develop_only,
        "prsList": prs_list,
        "base": base, "baseName": base.strip_prefix("origin/").unwrap_or(base),
        "develop": {
            "sha": opt_str(dev.first().copied().filter(|s| !s.is_empty())),
            "date": opt_str(dev.get(1).copied()), "subject": opt_str(dev.get(2).copied()),
            "newCommits": if is_uint(ahead) { js_num(ahead.parse().unwrap_or(0.0)) } else { Value::Null },
            "fetchedAt": js_opt(mtime_of(&fetch_head)),
        },
        "prs": prs, "report": report,
        "headsChanged": if heads.is_empty() { Value::Null } else { Value::from(heads) },
        "conflicts": conflicts.len(), "conflictsTail": tail,
        "statusAt": js_opt(mtime_of(&dir.join("STATUS.md"))),
    })
}

/// `/([^:/]+\/[^/]+?)(?:\.git)?\/?$/` sobre a URL do `origin`.
pub fn repo_of(url: &str) -> Option<String> {
    let u = url.strip_suffix('/').unwrap_or(url);
    let u = u.strip_suffix(".git").unwrap_or(u);
    let (head, name) = u.rsplit_once('/')?;
    let owner = head.rsplit(['/', ':']).next()?;
    (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// `versionState` sem a consulta à rede: o cache `version-check.json` de `$D`, senão o estado inicial.
fn version_state(env: &PanelEnv) -> Value {
    let mut v = json!({ "enabled": true, "access": false, "local": "", "remote": "", "checkedAt": null,
                        "update": false });
    if let Some(cache) = crate::leitura::read_json(&env.data().join("version-check.json")) {
        for k in ["access", "local", "remote", "checkedAt", "update"] {
            if let Some(x) = cache.get(k) {
                v[k] = x.clone();
            }
        }
    }
    v
}

/// A amostra pelos serviços de verdade (ou nenhum, com a infraestrutura desligada).
pub fn snapshot(env: &PanelEnv) -> Value {
    snapshot_com(env, &SemInfra, available_parallelism())
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// `sample()` com as fontes dadas; `ncpu` é o `availableParallelism()` do host.
pub fn snapshot_com(env: &PanelEnv, fontes: &dyn Fontes, ncpu: usize) -> Value {
    let dir = env.data();
    let now = env.now_ms().map_or(0.0, |ms| ms as f64 / 1000.0);
    let up = uptime(env);
    let inst = Instancia::de(env);
    let hu = &inst.hu;
    let catalog = service_catalog(dir);
    let unit_names: Vec<String> = catalog
        .iter()
        .map(|s| format!("{hu}-{}", s.name))
        .chain(
            ["devtunnel", "panel", "panel-tunnel", "widget-tunnel"]
                .iter()
                .map(|u| format!("{hu}-{u}")),
        )
        .collect();
    let units = fontes.units(&unit_names);
    let enabled = fontes.enabled_services();
    let required = trimmed_lines(&dir.join("services.required"));
    let w = worktree(env);
    let view = |name: &str| unit_view(units.get(name), up);

    let mut services: Vec<Value> = Vec::new();
    for s in &catalog {
        let mut v = view(&format!("{hu}-{}", s.name));
        let active = v["state"] == "active";
        let code = match (active, s.port) {
            (true, Some(p)) => fontes.http_status(&format!("http://127.0.0.1:{p}/health/ready")),
            _ => None,
        };
        let obj = v.as_object_mut().expect("unit_view é objeto");
        obj.insert("name".into(), Value::from(s.name.clone()));
        obj.insert("port".into(), js_opt(s.port.map(|p| p as f64)));
        obj.insert("enabled".into(), Value::Bool(enabled.contains(&s.name)));
        obj.insert("required".into(), Value::Bool(required.contains(&s.name)));
        let built = w.join("dist").join("apps").join(&s.name).join("main.js");
        obj.insert("built".into(), Value::Bool(built.exists()));
        obj.insert("redis".into(), Value::Bool(s.redis));
        obj.insert("db".into(), Value::Bool(s.db));
        obj.insert("logs".into(), json!({ "errors": 0 }));
        obj.insert("ready".into(), Value::Bool(code == Some(200)));
        obj.insert("readyCode".into(), js_opt(code.map(|c| c as f64)));
        services.push(v);
    }

    let mut extras = Map::new();
    for (key, unit) in [
        ("tunnel", "devtunnel"),
        ("panel", "panel"),
        ("panelTunnel", "panel-tunnel"),
        ("widgetTunnel", "widget-tunnel"),
    ] {
        extras.insert(key.into(), view(&format!("{hu}-{unit}")));
    }

    let boxes = fontes.containers();
    let slice = fontes.slice_usage();
    let jobs = fontes.jobs();
    let mem = meminfo(env.proc_root());
    let m = |k: &str| mem.get(k).copied().unwrap_or(0);
    let cede: Vec<String> = trimmed_lines(&dir.join("logs").join("cede.log"));
    let last_cede: Vec<&str> = cede
        .last()
        .map(|l| l.split_whitespace().collect())
        .unwrap_or_default();
    let busy = jobs
        .iter()
        .any(|j| j["state"] == "rodando" && crate::leitura::truthy(&j["exclusive"]));
    let procs = fontes.skill_procs();
    let on: Vec<&Value> = services.iter().filter(|s| s["enabled"] == true).collect();
    let long: Vec<&Value> = boxes
        .iter()
        .filter(|c| !crate::leitura::truthy(&c["oneshot"]))
        .collect();
    let paused = last_cede.last() == Some(&"paused");
    let state = overall(
        busy || !procs.is_empty(),
        paused,
        on.iter().filter(|s| s["state"] == "active").count(),
        on.iter().filter(|s| s["ready"] == true).count(),
        on.len(),
        long.iter().filter(|c| c["state"] == "running").count(),
        long.len(),
    );
    let cfg = config_values(env);
    let cfg_json: Map<String, Value> = cfg
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.clone())))
        .collect();
    let tunnel_url = read_text(&dir.join("tunnel.url"))
        .unwrap_or_default()
        .trim()
        .to_owned();
    let tunnel = &inst.tunnel_id;
    json!({
        "t": js_num(now), "overall": state, "paused": paused, "pausedAt": opt_str(last_cede.first().copied()),
        "cedeDisabled": dir.join("cede.disabled").exists() || cfg.get("HADES_CEDE").map(String::as_str) == Some("0"),
        "config": cfg_json,
        "host": {
            "memTotal": m("MemTotal"), "memAvailable": m("MemAvailable"),
            "swapUsed": m("SwapTotal") - m("SwapFree"), "swapTotal": m("SwapTotal"),
            "load": load_average(env.proc_root()).into_iter().map(js_opt).collect::<Vec<_>>(), "ncpu": ncpu,
        },
        "slice": slice, "services": services, "containers": boxes, "plugin": {}, "extras": extras,
        "endpoints": {},
        "tunnelUrl": if tunnel_url.is_empty() { Value::Null } else { Value::from(tunnel_url) },
        "panelTunnel": { "unit": view(&format!("{hu}-panel-tunnel"))["state"], "url": null },
        "unitPrefix": hu,
        "instance": { "id": inst.id, "unitPrefix": hu, "slice": inst.slice, "project": inst.project,
                      "tunnel": format!("hades{tunnel}"), "panelTunnel": format!("hades-panel{tunnel}") },
        "branch": branch(env, fontes), "gates": gate_queue(locks_dir(env).as_deref()), "jobs": jobs,
        "skillProcs": procs,
        "paths": { "D": dir.to_string_lossy(), "W": w.to_string_lossy() },
        "herdr": if infra_off(env) { Value::Null } else { fontes.herdr_badge() },
        "versionCheck": version_state(env),
    })
}

#[cfg(test)]
#[path = "amostra_tests.rs"]
mod tests;
