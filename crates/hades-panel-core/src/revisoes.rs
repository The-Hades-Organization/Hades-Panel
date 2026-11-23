//! Rotas `reviews`, `review/report` e `reviews/loop` (`panel/routes-misc.ts:227-260`, `panel/reviews-loop.ts`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use hades_core::config::{config_value, unit_prefix};
use hades_core::fsx::read_text;
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::leitura::{
    PanelError, config_values, cwd, js_num, js_string, json_files, read_json, worktree,
};
use crate::vivo::Vivo;

/// `REVIEW_LIST_MAX`, `REVIEW_JOBS_MAX` (`routes-misc.ts:37-38`).
const REVIEW_LIST_MAX: usize = 20;
const REVIEW_JOBS_MAX: usize = 60;
const EFFORTS: [&str; 3] = ["low", "medium", "high"];
/// `MAX_PARALLEL_DEFAULT` e `MAX_PARALLEL_CEILING` (`lib/review-queue.ts`).
const MAX_PARALLEL_DEFAULT: i64 = 4;
const MAX_PARALLEL_CEILING: f64 = 32.0;
/// `ROUNDS_MAX` (`reviews-loop.ts`).
const ROUNDS_MAX: usize = 20;
const RODANDO: &str = "rodando";

static REVIEW_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{8,40}$").expect("regex"));
static GITHUB_REMOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"github\.com[:/]([^/]+)/([^/.]+)").expect("regex"));

fn reviews_dir(env: &PanelEnv) -> PathBuf {
    env.env()
        .home()
        .join(".claude")
        .join("hades")
        .join("reviews")
}

/// `unitJob`: `HADES_JOB_UNIT`, senão `<prefixo>-pjob`.
fn job_unit(env: &PanelEnv) -> String {
    env.env()
        .get("HADES_JOB_UNIT")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}-pjob", unit_prefix(env.env(), env.data())))
}

/// Os jobs do painel em execução entre os `REVIEW_JOBS_MAX` mais novos: sem `<id>.rc` e com a unit ativa.
fn running_jobs(env: &PanelEnv, vivo: &dyn Vivo) -> Vec<String> {
    let dir = env.data().join("logs").join("panel-jobs");
    let mut files = json_files(&dir);
    files.reverse();
    let unit = job_unit(env);
    files
        .into_iter()
        .take(REVIEW_JOBS_MAX)
        .filter_map(|f| f.strip_suffix(".json").map(str::to_owned))
        .filter(|id| !dir.join(format!("{id}.rc")).exists())
        .filter(|id| vivo.unit_ativa(&format!("{unit}-{id}")))
        .collect()
}

/// `maxParallel` (`lib/review-queue.ts:19`).
fn max_parallel(env: &PanelEnv) -> i64 {
    let raw = config_value(
        env.env(),
        &cwd(),
        "HADES_REVIEW_MAX_PARALLEL",
        &MAX_PARALLEL_DEFAULT.to_string(),
    );
    match moirai_core::jsnum::js_number(raw.trim()) {
        Some(n) if n.fract() == 0.0 && (1.0..=MAX_PARALLEL_CEILING).contains(&n) => n as i64,
        _ => MAX_PARALLEL_DEFAULT,
    }
}

fn defaults(cfg: &HashMap<String, String>) -> Value {
    let model = cfg.get("HADES_MODEL_REVIEW").map_or("", String::as_str);
    let effort = cfg.get("HADES_EFFORT_REVIEW").map_or("", String::as_str);
    json!({
        "model": if model.contains("sonnet") { "sonnet" } else { "opus" },
        "effort": if EFFORTS.contains(&effort) { effort } else { "medium" },
    })
}

/// `reviews()` (`routes-misc.ts:227`): as últimas revisões, com o estado corrigido quando o job não roda mais.
pub fn reviews(env: &PanelEnv, vivo: &dyn Vivo) -> Value {
    let dir = reviews_dir(env);
    let mut names = json_files(&dir);
    names.reverse();
    let mut running: Option<Vec<String>> = None;
    let mut items = Vec::new();
    for name in names.iter().take(REVIEW_LIST_MAX) {
        let Some(text) = read_text(&dir.join(name)) else {
            continue;
        };
        let Ok(Value::Object(mut r)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if r.get("state").and_then(Value::as_str) == Some(RODANDO) {
            let job = js_string(r.get("job"));
            let jobs = running.get_or_insert_with(|| running_jobs(env, vivo));
            if !jobs.contains(&job) {
                r.insert("state".into(), Value::String("interrompido".into()));
            }
        }
        let id = js_string(r.get("id"));
        let report = dir.join(format!("{id}.md")).exists();
        r.insert("report".into(), Value::Bool(report));
        items.push(Value::Object(r));
    }
    json!({
        "items": items,
        "maxParallel": max_parallel(env),
        "defaults": defaults(&config_values(env)),
    })
}

/// `reviewReport(id)` (`routes-misc.ts:256`).
pub fn review_report(env: &PanelEnv, id: &str) -> Result<Value, PanelError> {
    if !REVIEW_ID.is_match(id) {
        return Err(PanelError::bad("revisão inválida"));
    }
    let file = reviews_dir(env).join(format!("{id}.md"));
    let text = read_text(&file).map_or(Value::Null, Value::String);
    Ok(json!({ "id": id, "file": file.to_string_lossy(), "text": text }))
}

fn is_finite_num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// `roundView` (`reviews-loop.ts`): a rodada pronta para a tela; linha fora do formato é ignorada.
pub fn round_view(line: &str) -> Option<Value> {
    let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    let id = doc.get("id")?.as_str()?;
    let round = doc.get("round").filter(|v| v.is_number())?;
    let started = doc.get("startedAt")?.as_str()?;
    let count = |k: &str| doc.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    let num = |k: &str| is_finite_num(doc.get(k)).map_or(json!(0), js_num);
    let mut m = Map::new();
    m.insert("id".into(), json!(id));
    m.insert("round".into(), round.as_f64().map_or(Value::Null, js_num));
    m.insert("startedAt".into(), json!(started));
    m.insert(
        "outcome".into(),
        json!(doc.get("outcome").and_then(Value::as_str).unwrap_or("?")),
    );
    m.insert("reviewed".into(), json!(count("reviewed")));
    m.insert("skipped".into(), json!(count("skipped")));
    m.insert("costUsd".into(), num("costUsd"));
    m.insert("tokens".into(), num("tokens"));
    m.insert(
        "usage".into(),
        doc.get("usage")
            .filter(|u| u.is_object())
            .cloned()
            .unwrap_or_else(|| json!({})),
    );
    Some(Value::Object(m))
}

/// `readStatus` (`lib/review-loop-store.ts:84`) já na forma da tela.
fn status_view(dir: &Path) -> Value {
    let Some(d) = read_json(&dir.join("status.json")) else {
        return Value::Null;
    };
    let (Some(pid), Some(round)) = (
        d.get("pid").filter(|v| v.is_number()),
        d.get("round").filter(|v| v.is_number()),
    ) else {
        return Value::Null;
    };
    let state = d
        .get("state")
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "rodando" | "pausada" | "parada"))
        .unwrap_or("parada");
    let text = |k: &str| d.get(k).and_then(Value::as_str);
    json!({
        "state": state,
        "pid": pid.as_f64().map_or(Value::Null, js_num),
        "since": text("since").unwrap_or(""),
        "round": round.as_f64().map_or(Value::Null, js_num),
        "nextAt": text("nextAt"),
        "reason": text("reason"),
    })
}

/// `loopRunning` (`lib/review-loop-store.ts:65`): outro processo segura o `loop.flock`.
fn loop_running(dir: &Path) -> bool {
    matches!(
        hades_core::lock::try_lock(&dir.join("loop.flock")),
        Ok(None)
    )
}

/// `reviewsLoopOverview` (`reviews-loop.ts`): o status do loop do repositório do worktree e as últimas rodadas.
pub fn reviews_loop(env: &PanelEnv, vivo: &dyn Vivo) -> Value {
    let none = json!({ "active": false, "status": null, "rounds": [] });
    let url = vivo.remote_url(&worktree(env)).unwrap_or_default();
    let Some(m) = GITHUB_REMOTE.captures(&url) else {
        return none;
    };
    let dir = hades_core::paths::work_root(env.env())
        .join(format!("{}__{}", &m[1], &m[2]))
        .join("review-loop");
    if !dir.exists() {
        return none;
    }
    let status = status_view(&dir);
    let text = read_text(&dir.join("rounds.jsonl")).unwrap_or_default();
    let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(ROUNDS_MAX * 2)..];
    let views: Vec<Value> = tail.iter().filter_map(|l| round_view(l)).collect();
    let mut rounds: Vec<Value> = views[views.len().saturating_sub(ROUNDS_MAX)..].to_vec();
    rounds.reverse();
    let active = dir.join("loop.flock").exists() && loop_running(&dir);
    json!({ "active": active, "status": status, "rounds": rounds })
}

#[cfg(test)]
#[path = "revisoes_tests.rs"]
mod tests;
