//! Rotas `orchestration`, `orchestration/file`, `moirai/plan` e `moirai/unit/<nome>` (`panel/orchestration.ts`,
//! `panel/moirai.ts`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use hades_core::config::effective_config;
use hades_core::fsx::read_text;
use hades_core::journal::{self, Query};
use moirai_core::budget::{MoiraiDeps, MoiraiError, MoiraiRoutes, Registry};
use moirai_core::real::default_projects_dir;
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::leitura::{PanelError, config_values, js_string, json_files, locks, read_json, truthy};
use crate::vivo::Vivo;

/// `FILES` (`orchestration.ts:17`), na ordem do objeto.
const FILES: [(&str, &str); 4] = [
    ("brief", "brief.md"),
    ("questions", "questions.md"),
    ("report", "report.md"),
    ("handoff", "handoff.md"),
];
const DEFAULT_REPORTS_DIR: &str = ".design-sync-reports";
const ORCH_JOURNAL: &str = "_sessions/_orch";
const DECISIONS_SHOWN: usize = 20;
const LEVEL_LABELS: [&str; 4] = [
    "Pergunta tudo",
    "Operacional",
    "Execução",
    "Totalmente autônomo",
];
const SKILL_SLUG: &str = "_skill";

static AGENT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,30}$").expect("regex"));
static HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^## (Q\d+.*)$").expect("regex"));
static ANSWERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^## Resposta do dono \((Q\d+)\)").expect("regex"));

/// Raiz do registro do orquestrador: `dirname(LOCKS)/orchestration`.
fn orch_root(env: &PanelEnv) -> Option<PathBuf> {
    locks(env).map(|l| parent(&l).join("orchestration"))
}

fn parent(p: &Path) -> PathBuf {
    p.parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

/// `path.join` do Node para os dois pedaços que o painel junta: o segundo nunca recomeça a raiz.
fn join_js(base: &str, rest: &str) -> PathBuf {
    let joined = format!("{base}/{rest}");
    let mut out = String::with_capacity(joined.len());
    for c in joined.chars() {
        if c == '/' && out.ends_with('/') {
            continue;
        }
        out.push(c);
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    PathBuf::from(out)
}

fn reports_dir(cfg: &std::collections::HashMap<String, String>) -> String {
    cfg.get("HADES_ORCH_REPORTS_DIR")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_REPORTS_DIR.to_owned())
}

/// `resolveHerdrBin` (`lib/config.ts:129`): vazio é `herdr`, `~/` expande o HOME e relativo só vale nome simples.
pub fn herdr_bin(env: &PanelEnv) -> String {
    let raw = effective_config(env.data(), env.env())
        .get("HADES_HERDR_BIN")
        .unwrap_or_default()
        .trim()
        .to_owned();
    if raw.is_empty() {
        return "herdr".into();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return env.env().home().join(rest).to_string_lossy().into_owned();
    }
    if raw.starts_with('/') || !raw.contains('/') {
        return raw;
    }
    "herdr".into()
}

fn worktree_of(a: &Map<String, Value>) -> String {
    match a.get("worktree") {
        Some(v) if truthy(v) => js_string(Some(v)),
        _ => "/nonexistent".into(),
    }
}

/// `autonomy()` (`orchestration.ts:84`).
fn autonomy(cfg: &std::collections::HashMap<String, String>) -> Value {
    let raw = cfg.get("HADES_ORCH_AUTONOMY").map_or("0", String::as_str);
    let level = moirai_core::jsnum::js_number(raw)
        .filter(|n| n.fract() == 0.0 && (0.0..=3.0).contains(n))
        .map_or(0, |n| n as usize);
    json!({ "level": level, "label": LEVEL_LABELS[level] })
}

/// `decisions()` (`orchestration.ts:90`): as últimas decisões do diário do orquestrador, mais recentes primeiro.
fn decisions(env: &PanelEnv) -> Vec<Value> {
    let query = Query {
        agent: Some(ORCH_JOURNAL.into()),
        since_ms: None,
        kind: Some("decision".into()),
        limit: DECISIONS_SHOWN,
    };
    let root = hades_core::paths::work_root(env.env());
    let mut out: Vec<Value> = journal::read_events(&query, &root)
        .into_iter()
        .filter_map(|e| {
            let v = e.to_value();
            let Some(Value::Object(d)) = v.get("decision") else {
                return None;
            };
            let mut m = Map::new();
            m.insert("ts".into(), Value::String(e.ts.clone()));
            for (k, val) in d {
                m.insert(k.clone(), val.clone());
            }
            Some(Value::Object(m))
        })
        .collect();
    out.reverse();
    out
}

/// Perguntas do `questions.md`: (abertas, total).
fn questions(text: &str) -> (Vec<String>, usize) {
    let heads: Vec<String> = HEAD.captures_iter(text).map(|c| c[1].to_owned()).collect();
    let answered: HashSet<String> = ANSWERED
        .captures_iter(text)
        .map(|c| c[1].to_owned())
        .collect();
    let open = heads
        .iter()
        .filter(|h| {
            let first = h.split_whitespace().next().unwrap_or_default();
            !h.contains("(respondida)") && !answered.contains(first)
        })
        .cloned()
        .collect();
    (open, heads.len())
}

fn agent_view(a: Map<String, Value>, rd: &str, live: Option<&Value>) -> Value {
    let w = worktree_of(&a);
    let base = join_js(&w, rd);
    let q = read_text(&base.join("questions.md")).unwrap_or_default();
    let (open, total) = questions(&q);
    let report = read_text(&base.join("report.md")).unwrap_or_default();
    let field = |k: &str| {
        live.and_then(|h| h.get(k))
            .filter(|v| !v.is_null())
            .cloned()
    };
    let live_status = match field("agent_status") {
        Some(v) if truthy(&v) => v,
        _ if a.get("state") != Some(&Value::String("done".into())) => Value::String("fora".into()),
        _ => Value::Null,
    };
    let files: Vec<Value> = FILES
        .iter()
        .filter(|(_, f)| base.join(f).exists())
        .map(|(k, _)| Value::String((*k).into()))
        .collect();
    let mut m = a;
    m.insert("live".into(), live_status);
    m.insert(
        "title".into(),
        field("terminal_title_stripped").unwrap_or(Value::Null),
    );
    m.insert("cwd".into(), field("foreground_cwd").unwrap_or(Value::Null));
    m.insert("pane".into(), field("pane_id").unwrap_or(Value::Null));
    m.insert("openQuestions".into(), json!(open));
    m.insert("questions".into(), json!(total));
    m.insert(
        "reportDone".into(),
        Value::Bool(report.trim_start().starts_with("STATUS: DONE")),
    );
    m.insert("files".into(), Value::Array(files));
    Value::Object(m)
}

/// `snapshot()` (`orchestration.ts:95`).
pub fn snapshot(env: &PanelEnv, vivo: &dyn Vivo) -> Value {
    let cfg = config_values(env);
    let Some(root) = orch_root(env) else {
        return json!({
            "available": false,
            "agents": [],
            "consolidations": [],
            "autonomy": autonomy(&cfg),
            "decisions": [],
        });
    };
    let live = vivo.herdr_agents(&herdr_bin(env));
    let live_of = |name: &Value| {
        live.iter()
            .filter(|h| h.get("name").is_some_and(truthy))
            .rfind(|h| h.get("name") == Some(name))
    };
    let rd = reports_dir(&cfg);
    let agents_dir = root.join("agents");
    let agents: Vec<Value> = json_files(&agents_dir)
        .iter()
        .filter_map(|f| read_json(&agents_dir.join(f)))
        .map(|a| {
            let h = a.get("name").and_then(live_of);
            agent_view(a.clone(), &rd, h)
        })
        .collect();
    let cons_dir = root.join("consolidations");
    let consolidations: Vec<Value> = json_files(&cons_dir)
        .iter()
        .filter_map(|f| read_json(&cons_dir.join(f)).map(Value::Object))
        .collect();
    json!({
        "available": true,
        "reportsDir": rd,
        "agents": agents,
        "consolidations": consolidations,
        "autonomy": autonomy(&cfg),
        "decisions": decisions(env),
    })
}

/// `file(name, which)` (`orchestration.ts:119`).
pub fn file(env: &PanelEnv, name: &str, which: &str) -> Result<Value, PanelError> {
    let root = orch_root(env)
        .filter(|_| AGENT_NAME.is_match(name))
        .ok_or_else(|| PanelError::bad("agente inválido"))?;
    let a = read_json(&root.join("agents").join(format!("{name}.json")))
        .ok_or_else(|| PanelError::bad(format!("agente {name} não registrado")))?;
    let (_, file_name) = FILES
        .iter()
        .find(|(k, _)| *k == which)
        .ok_or_else(|| PanelError::bad("arquivo inválido"))?;
    let path = join_js(&worktree_of(&a), &reports_dir(&config_values(env))).join(file_name);
    let text = read_text(&path).map_or(Value::Null, Value::String);
    Ok(json!({ "name": name, "file": path.to_string_lossy(), "text": text }))
}

/// `registries` (`argos-graph.ts:563`): o projeto do `LOCKS` (se não for `_skill`) e sempre o `_skill`.
pub fn registries(env: &PanelEnv) -> Vec<Registry> {
    let locks = locks(env);
    let root = locks
        .as_deref()
        .map_or_else(|| env.work_root().to_path_buf(), |l| parent(&parent(l)));
    let mut out = Vec::new();
    if let Some(l) = locks.as_deref() {
        let proj = parent(l);
        let slug = proj
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if slug != SKILL_SLUG {
            out.push(Registry {
                slug,
                orch: proj.join("orchestration"),
            });
        }
    }
    out.push(Registry {
        slug: SKILL_SLUG.into(),
        orch: root.join(SKILL_SLUG).join("orchestration"),
    });
    out
}

/// As rotas do Moirai sobre o `moirai-core`, com o relógio real (o TS usa `new Date`, não `HADES_NOW`).
pub fn moirai_routes(env: &PanelEnv) -> MoiraiRoutes {
    let regs = registries(env);
    let cfg = config_values(env);
    MoiraiRoutes::new(MoiraiDeps {
        registries: Box::new(move || regs.clone()),
        config: Box::new(move || cfg.clone()),
        projects_dir: default_projects_dir(env.env()),
        now_ms: Box::new(|| hades_core::journal::now_ms() as f64),
    })
}

pub fn moirai_error(e: MoiraiError) -> PanelError {
    PanelError {
        message: e.message,
        code: Some(e.code),
    }
}

#[cfg(test)]
#[path = "orquestracao_tests.rs"]
mod tests;
