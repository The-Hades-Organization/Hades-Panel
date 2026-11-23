//! Estado do loop por repositório: escalações, entregas, PRs recentes, sumários de gate e journal da fila
//! (`panel/gatewatch.ts:167-269`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use super::procs::{read_text, readdir_sorted};
use super::tempo::{falsy, iso_utc_epoch, js_num, js_string};

const SUMMARY_WINDOW_S: f64 = 6.0 * 3600.0;
const RECENT_GATES: usize = 15;
const SHARD_MAX_AGE_S: f64 = 86_400.0 * 2.0;
const DAY_S: f64 = 86_400.0;
const JOURNAL_TAIL: usize = 40;
const ESCALATION_WHY_MAX: usize = 220;
const ESCALATIONS_SHOWN: usize = 12;
const RECENT_PRS_SHOWN: usize = 12;
const SUMMARY_LAST_MAX: usize = 160;
const FAILED_SHOWN: usize = 6;
const NORMALIZED_MAX: usize = 120;

static CMD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\tcmd=(.+)$").unwrap());
static DUR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\tdur=(\d+)s").unwrap());
static CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(?:exit|rc)=(\d+)|^[\w:.-]+=(\d+)$").unwrap());
static PR_SHARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^pr-.*\.json$").unwrap());

/// `.slice(0, n)` do JS por caractere.
pub fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `mtimeOf` (`gatewatch.ts:56-63`): segundos desde a época, com fração.
pub fn mtime_of(path: &Path) -> Option<f64> {
    let m = fs::metadata(path).ok()?.modified().ok()?;
    let d = m.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((d.as_micros() as f64 / 1000.0).floor() / 1000.0)
}

fn load_json(path: &Path) -> Value {
    read_text(path)
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

fn as_dict(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn or_null(m: &Map<String, Value>, key: &str) -> Value {
    m.get(key).cloned().unwrap_or(Value::Null)
}

/// `countBy` (`gatewatch.ts:65-69`), na ordem em que cada chave aparece.
fn count_by(items: &[String]) -> Map<String, Value> {
    let mut out = Map::new();
    for i in items {
        let n = out.get(i).and_then(Value::as_i64).unwrap_or(0);
        out.insert(i.clone(), Value::from(n + 1));
    }
    out
}

/// `skillState` (`gatewatch.ts:167-210`).
pub fn skill_state(repo_dir: &Path, at: f64) -> Value {
    let state_dir = repo_dir.join("state");
    let g = as_dict(&load_json(&state_dir.join("global.json")));
    let mut escalations: Vec<Map<String, Value>> = g
        .get("escalations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(as_dict)
        .filter(|e| falsy(e.get("answeredAt")))
        .collect();
    let asked = |e: &Map<String, Value>| {
        e.get("askedAt")
            .filter(|v| !v.is_null())
            .map(js_string)
            .unwrap_or_default()
    };
    escalations.sort_by_key(|e| std::cmp::Reverse(asked(e)));
    let statuses: Vec<String> = g
        .get("deliveries")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
        .map(|v| {
            as_dict(v)
                .get("status")
                .filter(|s| !s.is_null())
                .map_or_else(|| "?".to_owned(), js_string)
        })
        .collect();
    let mut hours: HashMap<i64, Map<String, Value>> = HashMap::new();
    let mut verdicts = Map::new();
    let mut recent: Vec<(f64, Value)> = Vec::new();
    for name in readdir_sorted(&state_dir)
        .into_iter()
        .filter(|n| PR_SHARD.is_match(n))
    {
        let path = state_dir.join(&name);
        let Some(mtime) = mtime_of(&path) else {
            continue;
        };
        if at - mtime > SHARD_MAX_AGE_S {
            continue;
        }
        let d = as_dict(&load_json(&path));
        let mut last = 0.0_f64;
        for bucket in ["threads", "comments"] {
            for v in d
                .get(bucket)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|m| m.values())
            {
                let item = as_dict(v);
                let Some(t) =
                    iso_utc_epoch(item.get("at").and_then(Value::as_str)).filter(|t| *t != 0.0)
                else {
                    continue;
                };
                last = last.max(t);
                if at - t <= DAY_S {
                    let h = ((at - t) / 3600.0).floor() as i64;
                    let verdict = item
                        .get("verdict")
                        .filter(|v| !v.is_null())
                        .map_or_else(|| "?".to_owned(), js_string);
                    let row = hours.entry(h).or_default();
                    let n = row.get(&verdict).and_then(Value::as_i64).unwrap_or(0);
                    row.insert(verdict.clone(), Value::from(n + 1));
                    let n = verdicts.get(&verdict).and_then(Value::as_i64).unwrap_or(0);
                    verdicts.insert(verdict, Value::from(n + 1));
                }
            }
        }
        let rv = as_dict(d.get("review").unwrap_or(&Value::Null));
        recent.push((
            last,
            json!({
                "pr": or_null(&d, "pr"), "last": js_num(last), "rounds": or_null(&d, "reviewRounds"),
                "review": or_null(&rv, "verdict"), "blocking": or_null(&rv, "blocking"),
            }),
        ));
    }
    recent.sort_by(|a, b| b.0.total_cmp(&a.0));
    let shown: Vec<Value> = escalations
        .iter()
        .take(ESCALATIONS_SHOWN)
        .map(|e| {
            let why = e.get("why").filter(|v| !v.is_null()).map(js_string).unwrap_or_default();
            json!({ "id": or_null(e, "id"), "pr": or_null(e, "pr"), "why": cut(&why, ESCALATION_WHY_MAX), "askedAt": or_null(e, "askedAt") })
        })
        .collect();
    let by_hour: Vec<Value> = (0..24)
        .map(|h| {
            let mut row = Map::new();
            row.insert("hoursAgo".into(), Value::from(h));
            for (k, v) in hours.get(&h).cloned().unwrap_or_default() {
                row.insert(k, v);
            }
            Value::Object(row)
        })
        .collect();
    json!({
        "escalationsOpen": escalations.len(),
        "escalations": shown,
        "deliveries": count_by(&statuses),
        "actionsByHour": by_hour,
        "verdicts24h": verdicts,
        "recentPrs": recent.into_iter().take(RECENT_PRS_SHOWN).map(|r| r.1).collect::<Vec<_>>(),
    })
}

fn walk_files(root: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    for name in readdir_sorted(root) {
        let path = root.join(&name);
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            walk_files(&path, visit);
        } else if meta.is_file() {
            visit(&path, &name);
        }
    }
}

/// `recentGates` (`gatewatch.ts:212-239`): sumários de gate das últimas 6 h, do mais novo.
pub fn recent_gates(repo_dir: &Path, at: f64) -> Vec<(f64, Value)> {
    let mut runs: Vec<(f64, Value)> = Vec::new();
    walk_files(repo_dir, &mut |path, name| {
        if !(name.starts_with("summary") || name.ends_with(".summary")) {
            return;
        }
        let Some(mtime) = mtime_of(path) else { return };
        if at - mtime > SUMMARY_WINDOW_S {
            return;
        }
        let text = read_text(path).unwrap_or_default();
        let lines: Vec<&str> = text.split('\n').filter(|l| !l.trim().is_empty()).collect();
        let codes: Vec<(String, i64)> = lines
            .iter()
            .filter_map(|l| {
                let t = l.trim();
                let c = CODE_RE.captures(t)?;
                let n = c.get(1).or_else(|| c.get(2))?.as_str().parse().ok()?;
                Some((t.to_owned(), n))
            })
            .collect();
        let rel = path
            .strip_prefix(repo_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let failed: Vec<String> = codes
            .iter()
            .filter(|c| c.1 != 0)
            .map(|c| c.0.clone())
            .take(FAILED_SHOWN)
            .collect();
        runs.push((
            mtime,
            json!({
                "path": rel, "mtime": js_num(mtime), "done": lines.iter().any(|l| l.trim() == "DONE"),
                "steps": codes.len(), "failed": failed, "last": cut(lines.last().copied().unwrap_or(""), SUMMARY_LAST_MAX),
            }),
        ));
    });
    runs.sort_by(|a, b| b.0.total_cmp(&a.0));
    runs.truncate(RECENT_GATES);
    runs
}

/// `journalLines` (`gatewatch.ts:241`): `locks/gate-journal.tsv` sem a linha vazia final.
pub fn journal_lines(repo_dir: &Path) -> Vec<String> {
    let text = read_text(&repo_dir.join("locks").join("gate-journal.tsv")).unwrap_or_default();
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// `journalTail` (`gatewatch.ts:242`): as últimas 40 linhas, em campos.
pub fn journal_tail(lines: &[String]) -> Vec<Vec<String>> {
    let from = lines.len().saturating_sub(JOURNAL_TAIL);
    lines[from..]
        .iter()
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}

/// `stepDurations` (`gatewatch.ts:245-256`): mediana da duração por comando das linhas `done`.
pub fn step_durations(lines: &[String]) -> HashMap<String, i64> {
    let mut acc: HashMap<String, Vec<i64>> = HashMap::new();
    for line in lines.iter().filter(|l| l.contains("\tdone\t")) {
        let (Some(c), Some(d)) = (CMD_RE.captures(line), DUR_RE.captures(line)) else {
            continue;
        };
        let Ok(dur) = d[1].parse() else { continue };
        acc.entry(c[1].trim().to_owned()).or_default().push(dur);
    }
    acc.into_iter()
        .map(|(k, mut v)| {
            v.sort_unstable();
            let mid = v[v.len() / 2];
            (k, mid)
        })
        .collect()
}

/// `normalizeCmd` (`gatewatch.ts:259-272`): a mesma chave do `normalize_cmd` do gate-lock.sh.
pub fn normalize_cmd(cmd: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut skip = false;
    for tok in cmd.split_whitespace() {
        if !skip && tok == "env" {
            skip = true;
            continue;
        }
        if skip && tok.contains('=') {
            continue;
        }
        skip = false;
        out.push(tok.rsplit('/').next().unwrap_or(tok));
    }
    cut(&out.join(" "), NORMALIZED_MAX)
}
