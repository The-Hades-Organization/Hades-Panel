//! Herdr só leitura (`panel/herdr-page.ts:363-420`, `overview` enxuto): sessões, árvore da sessão escolhida e a
//! leitura de um pane. Sem `since`, branch nem registro do Hades.

use std::process::{Command, Stdio};

use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::orquestracao::herdr_bin;

/// `CALL_TIMEOUT_MS` (`herdr-page.ts`).
const TIMEOUT_S: u64 = 8;
/// Linhas lidas do pane no detalhe.
pub const LINHAS: u32 = 200;

fn herdr(e: &PanelEnv, args: &[&str]) -> Option<String> {
    let o = Command::new("timeout")
        .arg(TIMEOUT_S.to_string())
        .arg(herdr_bin(e))
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

fn rows(v: Option<&Value>) -> Vec<&Map<String, Value>> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_object).collect())
        .unwrap_or_default()
}

fn texto(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn opt(m: &Map<String, Value>, k: &str) -> Value {
    m.get(k).cloned().unwrap_or(Value::Null)
}

/// `.sessions` de `herdr session list --json`.
pub fn sessoes_de(stdout: &str) -> Vec<Map<String, Value>> {
    serde_json::from_str::<Value>(stdout)
        .ok()
        .and_then(|v| v.get("sessions").cloned())
        .and_then(|v| v.as_array().cloned())
        .map(|a| {
            a.into_iter()
                .filter_map(|s| s.as_object().cloned())
                .collect()
        })
        .unwrap_or_default()
}

/// `.result.snapshot` de `herdr api snapshot`.
pub fn snapshot_de(stdout: &str) -> Option<Value> {
    serde_json::from_str::<Value>(stdout)
        .ok()?
        .pointer("/result/snapshot")
        .cloned()
}

fn snap(e: &PanelEnv, sessao: &str) -> Option<Value> {
    snapshot_de(&herdr(e, &["--session", sessao, "api", "snapshot"])?)
}

/// A sessão pedida, senão `default`, senão a primeira.
pub fn escolher(nomes: &[String], pedida: Option<&str>) -> Option<String> {
    pedida
        .filter(|p| nomes.iter().any(|n| n == p))
        .map(str::to_owned)
        .or_else(|| nomes.iter().find(|n| *n == "default").cloned())
        .or_else(|| nomes.first().cloned())
}

fn conta(agents: &[&Map<String, Value>], status: &str) -> usize {
    agents
        .iter()
        .filter(|a| texto(a.get("status").or(a.get("agent_status"))) == status)
        .count()
}

/// A linha de uma sessão na lista (`views`).
pub fn sessao_view(s: &Map<String, Value>, snap: Option<&Value>) -> Value {
    let agents = rows(snap.and_then(|v| v.get("agents")));
    json!({
        "name": opt(s, "name"),
        "running": s.get("running").and_then(Value::as_bool).unwrap_or(false),
        "default": s.get("default").and_then(Value::as_bool).unwrap_or(false),
        "agents": agents.len(),
        "working": conta(&agents, "working"),
        "blocked": conta(&agents, "blocked"),
    })
}

/// A árvore workspace → tab → pane de um snapshot.
pub fn arvore(snap: &Value) -> Vec<Value> {
    let agents: Vec<_> = rows(snap.get("agents"));
    let agente = |pane: &str| {
        agents
            .iter()
            .find(|a| texto(a.get("pane_id")) == pane)
            .copied()
    };
    let panes = rows(snap.get("panes"));
    let tabs = rows(snap.get("tabs"));
    rows(snap.get("workspaces"))
        .into_iter()
        .map(|w| {
            let wid = texto(w.get("workspace_id"));
            let tabs: Vec<Value> = tabs
                .iter()
                .filter(|t| texto(t.get("workspace_id")) == wid)
                .map(|t| {
                    let tid = texto(t.get("tab_id"));
                    let panes: Vec<Value> = panes
                        .iter()
                        .filter(|p| texto(p.get("tab_id")) == tid)
                        .map(|p| {
                            let id = texto(p.get("pane_id"));
                            let a = agente(&id);
                            let status = texto(p.get("agent_status"));
                            let titulo = texto(p.get("terminal_title_stripped"));
                            json!({
                                "id": id,
                                "focused": p.get("focused").and_then(Value::as_bool).unwrap_or(false),
                                "cwd": p.get("foreground_cwd").or(p.get("cwd")).cloned().unwrap_or(Value::Null),
                                "title": if titulo.is_empty() { Value::Null } else { titulo.into() },
                                "agent": p.get("agent").filter(|v| !v.is_null()).cloned()
                                    .or_else(|| a.and_then(|a| a.get("agent").cloned()))
                                    .unwrap_or(Value::Null),
                                "name": a.map(|a| opt(a, "name")).unwrap_or(Value::Null),
                                "status": if status.is_empty() { "unknown".into() } else { status },
                            })
                        })
                        .collect();
                    json!({
                        "id": tid, "label": opt(t, "label"), "number": opt(t, "number"),
                        "status": opt(t, "agent_status"), "panes": panes,
                    })
                })
                .collect();
            let wt = w.get("worktree").and_then(Value::as_object);
            json!({
                "id": wid, "label": opt(w, "label"), "number": opt(w, "number"),
                "status": opt(w, "agent_status"),
                "worktree": wt.map(|wt| json!({"repo": opt(wt, "repo_name"), "path": opt(wt, "checkout_path")})),
                "tabs": tabs,
            })
        })
        .collect()
}

/// `overview` (`herdr-page.ts:363`), enxuto.
pub fn overview(e: &PanelEnv, pedida: Option<&str>) -> Value {
    let Some(lista) = herdr(e, &["session", "list", "--json"]) else {
        return json!({"available": false, "reason": "herdr indisponível", "sessions": [], "tree": []});
    };
    let todas = sessoes_de(&lista);
    let nomes: Vec<String> = todas.iter().map(|s| texto(s.get("name"))).collect();
    let sessao = escolher(&nomes, pedida);
    let mut alvo = None;
    let views: Vec<Value> = todas
        .iter()
        .map(|s| {
            let nome = texto(s.get("name"));
            let rodando = s.get("running").and_then(Value::as_bool).unwrap_or(false);
            let sn = rodando.then(|| snap(e, &nome)).flatten();
            if sessao.as_deref() == Some(nome.as_str()) {
                alvo = sn.clone();
            }
            sessao_view(s, sn.as_ref())
        })
        .collect();
    let reason = match (&sessao, &alvo) {
        (None, _) => Some("nenhuma sessão do herdr"),
        (Some(_), None) => Some("servidor herdr desta sessão fora do ar"),
        _ => None,
    };
    json!({
        "available": true,
        "sessions": views,
        "session": sessao,
        "tree": alvo.as_ref().map(arvore).unwrap_or_default(),
        "reason": reason,
    })
}

/// O texto recente do pane, sem ANSI; painel recém-aberto cai na tela visível.
pub fn pane_read(e: &PanelEnv, sessao: &str, pane: &str) -> Result<String, String> {
    let linhas = LINHAS.to_string();
    for fonte in ["recent-unwrapped", "visible"] {
        let args = [
            "--session",
            sessao,
            "pane",
            "read",
            pane,
            "--source",
            fonte,
            "--lines",
            &linhas,
        ];
        if let Some(t) = herdr(e, &args) {
            let t = crate::logs::strip_ansi(&t);
            if !t.trim().is_empty() {
                return Ok(t);
            }
        }
    }
    Err(format!("não consegui ler o pane {pane}"))
}

#[cfg(test)]
#[path = "herdr_tests.rs"]
mod tests;
