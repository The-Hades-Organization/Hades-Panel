use serde_json::json;

use super::*;
use crate::teste_vivo::{Falso, panel, sandbox, write};

fn com_locks(nome: &str) -> PathBuf {
    let h = sandbox(nome);
    let locks = h.join("work/acme__demo/locks");
    write(
        &h.join("data/env.sh"),
        &format!("LOCKS={}\n", locks.display()),
    );
    h
}

#[test]
fn sem_locks_nao_ha_orquestracao() {
    let h = sandbox("orq-sem");
    let v = snapshot(&panel(&h), &Falso::default());
    assert_eq!(v["available"], json!(false));
    assert_eq!(v["autonomy"], json!({"level": 0, "label": "Pergunta tudo"}));
}

#[test]
fn agente_com_perguntas_relatorio_e_vivo() {
    let h = com_locks("orq-agente");
    let wt = h.join("wt/a1");
    let orch = h.join("work/acme__demo/orchestration");
    write(
        &orch.join("agents/a1.json"),
        &format!(
            r#"{{"name":"a1","live":"x","state":"running","worktree":"{}"}}"#,
            wt.display()
        ),
    );
    write(
        &orch.join("agents/b2.json"),
        r#"{"name":"b2","state":"done"}"#,
    );
    write(&orch.join("agents/ruim.json"), "[1]");
    write(&orch.join("consolidations/c1.json"), r#"{"id":"c1"}"#);
    let rd = wt.join(DEFAULT_REPORTS_DIR);
    write(
        &rd.join("questions.md"),
        "## Q1 primeira\n## Q2 (respondida)\n## Q3 terceira\n## Resposta do dono (Q3)\nok\n",
    );
    write(&rd.join("report.md"), "\n  STATUS: DONE\n");
    let vivo = Falso {
        agents: vec![
            json!({"name":"a1","agent_status":"idle","pane_id":"p1","terminal_title_stripped":null}),
        ],
        ..Falso::default()
    };
    let v = snapshot(&panel(&h), &vivo);
    assert_eq!(v["available"], json!(true));
    let a1 = &v["agents"][0];
    let keys: Vec<&String> = a1.as_object().unwrap().keys().collect();
    assert_eq!(keys[1], "live");
    assert_eq!(a1["live"], json!("idle"));
    assert_eq!(a1["pane"], json!("p1"));
    assert_eq!(a1["title"], Value::Null);
    assert_eq!(a1["openQuestions"], json!(["Q1 primeira"]));
    assert_eq!(a1["questions"], json!(3));
    assert_eq!(a1["reportDone"], json!(true));
    assert_eq!(a1["files"], json!(["questions", "report"]));
    assert_eq!(v["agents"][1]["live"], Value::Null);
    assert_eq!(v["agents"].as_array().unwrap().len(), 2);
    assert_eq!(v["consolidations"], json!([{"id":"c1"}]));
}

#[test]
fn arquivo_do_agente_valida_nome_e_arquivo() {
    let h = com_locks("orq-file");
    let env = panel(&h);
    let orch = h.join("work/acme__demo/orchestration");
    write(
        &orch.join("agents/a1.json"),
        r#"{"name":"a1","worktree":"/nada"}"#,
    );
    assert_eq!(
        file(&env, "A1", "brief").unwrap_err(),
        PanelError::bad("agente inválido")
    );
    assert_eq!(
        file(&env, "zz", "brief").unwrap_err(),
        PanelError::bad("agente zz não registrado")
    );
    assert_eq!(
        file(&env, "a1", "x").unwrap_err(),
        PanelError::bad("arquivo inválido")
    );
    let v = file(&env, "a1", "brief").unwrap();
    assert_eq!(v["file"], json!("/nada/.design-sync-reports/brief.md"));
    assert_eq!(v["text"], Value::Null);
}

#[test]
fn registros_do_projeto_e_da_skill() {
    let h = com_locks("orq-reg");
    let r = registries(&panel(&h));
    assert_eq!(r[0].slug, "acme__demo");
    assert_eq!(r[0].orch, h.join("work/acme__demo/orchestration"));
    assert_eq!(r[1].slug, "_skill");
    assert_eq!(r[1].orch, h.join("work/_skill/orchestration"));
    let s = sandbox("orq-reg-sem");
    let r = registries(&panel(&s));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].orch, s.join("work/_skill/orchestration"));
}

#[test]
fn autonomia_fora_da_faixa_e_zero() {
    let cfg = |v: &str| {
        std::collections::HashMap::from([("HADES_ORCH_AUTONOMY".to_owned(), v.to_owned())])
    };
    assert_eq!(autonomy(&cfg("2"))["label"], json!("Execução"));
    assert_eq!(autonomy(&cfg("4"))["level"], json!(0));
    assert_eq!(autonomy(&cfg("1.5"))["level"], json!(0));
}

#[test]
fn join_do_node() {
    assert_eq!(join_js("/a/", ".x"), PathBuf::from("/a/.x"));
    assert_eq!(join_js("/a", "/b/"), PathBuf::from("/a/b"));
}
