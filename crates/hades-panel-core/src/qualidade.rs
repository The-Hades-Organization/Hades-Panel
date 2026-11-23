//! Rota `quality` (`panel/routes-core.ts:117-123`): o catálogo de qualidade resolvido contra o perfil do projeto, com
//! os pacotes e o caminho das escolhas. A regra de cada função está em `lib/quality.ts`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hades_core::Env;
use hades_core::fsx::{list_dir, read_text};
use hades_core::paths::{hades_home, skill_root};
use hades_core::profile::{load_profile, path_value, read_json};
use hades_core::project::resolve_project;
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::leitura::worktree;

/// `GROUPS` (`quality.ts:12`).
pub const GROUPS: &[&str] = &[
    "linguagem",
    "framework",
    "praticas",
    "estilo",
    "testes",
    "banco",
];
/// `LEVELS` (`quality.ts:13`).
pub const LEVELS: &[&str] = &["bloqueante", "alta", "media", "baixa"];
const SCHEMA: &str = "hades.quality/1";

type Obj = Map<String, Value>;

/// `userDir` (`quality.ts:27`).
pub fn user_dir(env: &Env) -> PathBuf {
    hades_home(env).join("quality")
}

fn str_of(v: Option<&Value>, fallback: &str) -> String {
    v.and_then(Value::as_str).unwrap_or(fallback).to_owned()
}

/// `items` (`quality.ts:35-40`).
fn items(file: &Path, key: &str) -> Vec<Obj> {
    let Some(Value::Object(doc)) = read_json(file) else {
        return Vec::new();
    };
    if doc.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Vec::new();
    }
    match doc.get(key) {
        Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_object().cloned()).collect(),
        _ => Vec::new(),
    }
}

/// `collect` (`quality.ts:52-58`): base e depois usuário; id repetido fica no lugar do primeiro com o valor do último.
fn collect(env: &Env, key: &str) -> Vec<Obj> {
    let mut out: Vec<Obj> = Vec::new();
    let mut pos: HashMap<String, usize> = HashMap::new();
    for (root, source) in [
        (skill_root(env).join("quality"), "base"),
        (user_dir(env), "usuario"),
    ] {
        for name in list_dir(&root).into_iter().filter(|n| n.ends_with(".json")) {
            for mut item in items(&root.join(name), key) {
                item.insert("source".into(), json!(source));
                let id = str_of(item.get("id"), "");
                match pos.get(&id) {
                    Some(&i) => out[i] = item,
                    None => {
                        pos.insert(id, out.len());
                        out.push(item);
                    }
                }
            }
        }
    }
    out
}

/// `readChoices` (`quality.ts:76-85`).
pub fn read_choices(env: &Env) -> HashMap<String, String> {
    let mut choices = HashMap::new();
    let Some(text) = read_text(&user_dir(env).join("choices.tsv")) else {
        return choices;
    };
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let cols: Vec<&str> = line.split('\t').collect();
        if !line.is_empty()
            && !line.starts_with('#')
            && cols.len() == 2
            && (cols[1] == "on" || cols[1] == "off")
        {
            choices.insert(cols[0].to_owned(), cols[1].to_owned());
        }
    }
    choices
}

/// `effective` (`quality.ts:107-113`).
fn effective(env: &Env) -> Vec<Obj> {
    let choices = read_choices(env);
    collect(env, "rules")
        .into_iter()
        .map(|mut r| {
            let id = str_of(r.get("id"), "");
            let fallback = str_of(r.get("default"), "on");
            let state = choices
                .get(&id)
                .cloned()
                .unwrap_or_else(|| fallback.clone());
            r.insert("state".into(), json!(state));
            r.insert("default".into(), json!(fallback));
            r.insert("chosen".into(), json!(choices.contains_key(&id)));
            r
        })
        .collect()
}

/// `packs` (`quality.ts:116-132`).
fn packs(env: &Env, rules: &[Obj]) -> Vec<Value> {
    let state: HashMap<String, String> = rules
        .iter()
        .map(|r| (str_of(r.get("id"), ""), str_of(r.get("state"), "")))
        .collect();
    collect(env, "packs")
        .into_iter()
        .map(|mut k| {
            let listed: Vec<String> = k
                .get("rules")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let mut members: Vec<String> = Vec::new();
            for i in &listed {
                if state.contains_key(i) && !members.contains(i) {
                    members.push(i.clone());
                }
            }
            let on = members.iter().filter(|i| state[*i] == "on").count();
            let missing: Vec<&String> = listed.iter().filter(|i| !state.contains_key(*i)).collect();
            let total = members.len();
            let st = if on == 0 {
                "off"
            } else if on == total {
                "on"
            } else {
                "partial"
            };
            k.insert("rules".into(), json!(members));
            k.insert("missing".into(), json!(missing));
            k.insert("on".into(), json!(on));
            k.insert("total".into(), json!(total));
            k.insert("state".into(), json!(st));
            Value::Object(k)
        })
        .collect()
}

/// `truthy` (`quality.ts:68-72`).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(_) => true,
    }
}

/// `overlayOf` (`quality.ts:165-173`).
fn overlay_of(env: &Env, dir: &Path) -> (Vec<(String, Obj)>, Option<String>) {
    let r = resolve_project(env, dir);
    if r.code != 0 {
        return (Vec::new(), None);
    }
    let profile = load_profile(&r);
    let mut overlay: Vec<(String, Obj)> = Vec::new();
    for e in path_value(&profile, "rules.quality")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(o) = e.as_object() else { continue };
        let id = str_of(o.get("id"), "");
        if id.is_empty() {
            continue;
        }
        match overlay.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = o.clone(),
            None => overlay.push((id, o.clone())),
        }
    }
    (overlay, r.project.clone())
}

fn conflict(id: &str, kind: &str, project: &Option<String>, o: &Obj, detail: String) -> Value {
    json!({ "id": id, "kind": kind, "project": project, "note": str_of(o.get("note"), ""), "detail": detail })
}

fn level_at(s: &str) -> i64 {
    LEVELS.iter().position(|l| *l == s).map_or(-1, |i| i as i64)
}

/// `resolved` (`quality.ts:184-218`): severidade efetiva (o perfil só sobe) e os conflitos com o projeto.
pub fn resolved(env: &Env, dir: &Path) -> (Vec<Obj>, Vec<Value>, Option<String>) {
    let (overlay, project) = overlay_of(env, dir);
    let mut rules = Vec::new();
    let mut conflicts = Vec::new();
    for base in effective(env) {
        let severity = str_of(base.get("severity"), "");
        let mut r = base;
        r.insert("effSeverity".into(), json!(severity));
        let id = str_of(r.get("id"), "");
        let on = r.get("state").and_then(Value::as_str) == Some("on");
        if let Some((_, o)) = overlay.iter().find(|(k, _)| *k == id) {
            let want = str_of(o.get("severity"), "");
            let (want_at, have_at) = (level_at(&want), level_at(&severity));
            if want_at >= 0 && want_at < have_at {
                r.insert("effSeverity".into(), json!(want));
                r.insert("raisedBy".into(), json!(project));
            } else if want_at >= 0 && want_at > have_at && on {
                let detail = format!(
                    "o projeto pede {want}, o catálogo ligado exige {severity}: vale {severity}"
                );
                conflicts.push(conflict(&id, "severidade-menor", &project, o, detail));
            }
            if truthy(o.get("contradicts")) && on {
                let detail = "a regra do projeto contraria este item ligado".to_owned();
                conflicts.push(conflict(&id, "contradiz", &project, o, detail));
            }
        }
        rules.push(r);
    }
    for (id, o) in &overlay {
        if !rules.iter().any(|r| str_of(r.get("id"), "") == *id) {
            let detail = "o perfil cita um id que não está no catálogo".to_owned();
            conflicts.push(conflict(id, "id-desconhecido", &project, o, detail));
        }
    }
    (rules, conflicts, project)
}

/// `qualityState` (`routes-core.ts:117-123`).
pub fn quality_state(env: &PanelEnv) -> Value {
    let e = env.env();
    let (rules, conflicts, project) = resolved(e, &worktree(env));
    let packs = packs(e, &rules);
    json!({
        "rules": rules,
        "conflicts": conflicts,
        "project": project,
        "groups": GROUPS,
        "levels": LEVELS,
        "packs": packs,
        "choices": user_dir(e).join("choices.tsv").to_string_lossy(),
    })
}

#[cfg(test)]
#[path = "qualidade_tests.rs"]
mod tests;
