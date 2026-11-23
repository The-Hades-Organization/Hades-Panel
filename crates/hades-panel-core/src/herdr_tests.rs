use serde_json::json;

use super::*;

#[test]
fn escolhe_pedida_default_ou_primeira() {
    let n = vec!["a".to_owned(), "default".to_owned()];
    assert_eq!(escolher(&n, Some("a")).as_deref(), Some("a"));
    assert_eq!(escolher(&n, Some("x")).as_deref(), Some("default"));
    assert_eq!(escolher(&n[..1], None).as_deref(), Some("a"));
    assert_eq!(escolher(&[], None), None);
}

#[test]
fn le_sessoes_e_snapshot() {
    let s = sessoes_de(r#"{"sessions":[{"name":"default","running":true}]}"#);
    assert_eq!(s.len(), 1);
    assert!(sessoes_de("lixo").is_empty());
    let snap = snapshot_de(r#"{"result":{"snapshot":{"agents":[]}}}"#).unwrap();
    assert_eq!(snap, json!({"agents": []}));
    assert!(snapshot_de("{}").is_none());
}

#[test]
fn monta_arvore_e_contagem() {
    let snap = json!({
        "workspaces": [{"workspace_id": "w1", "label": "repo", "worktree": {"repo_name": "r", "checkout_path": "/p"}}],
        "tabs": [{"workspace_id": "w1", "tab_id": "t1", "label": "aba"}],
        "panes": [
            {"tab_id": "t1", "pane_id": "p1", "agent_status": "working", "cwd": "/a", "terminal_title_stripped": "x"},
            {"tab_id": "t1", "pane_id": "p2", "cwd": "/b", "foreground_cwd": "/c"}
        ],
        "agents": [{"pane_id": "p1", "name": "rs-1", "agent": "claude", "status": "working"}]
    });
    let t = arvore(&snap);
    let panes = &t[0]["tabs"][0]["panes"];
    assert_eq!(panes[0]["name"], "rs-1");
    assert_eq!(panes[0]["agent"], "claude");
    assert_eq!(panes[1]["status"], "unknown");
    assert_eq!(panes[1]["cwd"], "/c");
    assert_eq!(t[0]["worktree"]["repo"], "r");
    let v = sessao_view(
        json!({"name": "default", "running": true})
            .as_object()
            .unwrap(),
        Some(&snap),
    );
    assert_eq!(v["agents"], 1);
    assert_eq!(v["working"], 1);
}
