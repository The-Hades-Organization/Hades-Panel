use serde_json::json;

use super::*;
use crate::teste_vivo::{Falso, panel, sandbox, write};

#[test]
fn revisao_rodando_sem_job_vira_interrompida() {
    let h = sandbox("rev-lista");
    let dir = h.join(".claude/hades/reviews");
    write(
        &dir.join("r-aaaaaaaa.json"),
        r#"{"id":"r-aaaaaaaa","state":"rodando","job":"j1"}"#,
    );
    write(
        &dir.join("r-bbbbbbbb.json"),
        r#"{"id":"r-bbbbbbbb","state":"rodando","job":"j2","report":1}"#,
    );
    write(&dir.join("r-bbbbbbbb.md"), "# r");
    write(&dir.join("r-cccccccc.json"), "{");
    write(&h.join("data/logs/panel-jobs/j1.json"), "{}");
    write(&h.join("data/logs/panel-jobs/j2.json"), "{}");
    write(&h.join("data/logs/panel-jobs/j2.rc"), "0");
    let env = panel(&h);
    let unit = format!("{}-j1", job_unit(&env));
    let vivo = Falso {
        ativas: vec![unit, format!("{}-j2", job_unit(&env))],
        ..Falso::default()
    };
    let v = reviews(&env, &vivo);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["state"], json!("interrompido"));
    assert_eq!(items[0]["report"], json!(true));
    let keys: Vec<&String> = items[0].as_object().unwrap().keys().collect();
    assert_eq!(keys[3], "report");
    assert_eq!(items[1]["state"], json!("rodando"));
    assert_eq!(items[1]["report"], json!(false));
    assert_eq!(v["maxParallel"], json!(4));
    assert_eq!(v["defaults"], json!({"model": "opus", "effort": "medium"}));
}

#[test]
fn relatorio_valida_o_id() {
    let h = sandbox("rev-rel");
    let env = panel(&h);
    assert_eq!(
        review_report(&env, "curto").unwrap_err(),
        PanelError::bad("revisão inválida")
    );
    assert_eq!(
        review_report(&env, "a/../bbbbbbbb").unwrap_err(),
        PanelError::bad("revisão inválida")
    );
    assert_eq!(
        review_report(&env, "r-aaaaaaaa").unwrap()["text"],
        Value::Null
    );
}

#[test]
fn rodada_fora_do_formato_some() {
    assert_eq!(round_view("{"), None);
    assert_eq!(
        round_view(r#"{"id":"x","round":"1","startedAt":"t"}"#),
        None
    );
    let v = round_view(
        r#"{"id":"x","round":2,"startedAt":"t","reviewed":[1,2],"costUsd":0.5,"usage":[]}"#,
    )
    .unwrap();
    assert_eq!(
        v,
        json!({"id":"x","round":2,"startedAt":"t","outcome":"?","reviewed":2,"skipped":0,"costUsd":0.5,"tokens":0,"usage":{}})
    );
}

#[test]
fn loop_le_status_e_rodadas() {
    let h = sandbox("rev-loop");
    let dir = h.join("work/acme__demo/review-loop");
    write(
        &dir.join("status.json"),
        r#"{"state":"x","pid":12,"round":3,"since":"s"}"#,
    );
    let rounds: String = (1..=45)
        .map(|i| format!("{{\"id\":\"r{i}\",\"round\":{i},\"startedAt\":\"t\"}}\n"))
        .collect();
    write(&dir.join("rounds.jsonl"), &format!("{rounds}{{corte"));
    let vivo = Falso {
        remote: Some("git@github.com:acme/demo.git".into()),
        ..Falso::default()
    };
    let v = reviews_loop(&panel(&h), &vivo);
    assert_eq!(v["active"], json!(false));
    assert_eq!(
        v["status"],
        json!({"state":"parada","pid":12,"since":"s","round":3,"nextAt":null,"reason":null})
    );
    let r = v["rounds"].as_array().unwrap();
    assert_eq!(r.len(), 20);
    assert_eq!(r[0]["id"], json!("r45"));
    let fora = Falso {
        remote: Some("https://gitlab.com/a/b".into()),
        ..Falso::default()
    };
    assert_eq!(reviews_loop(&panel(&h), &fora)["status"], Value::Null);
}
