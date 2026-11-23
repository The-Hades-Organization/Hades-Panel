use serde_json::json;

use super::*;
use crate::teste_vivo::{panel, sandbox, write};

#[test]
fn fontes_juntam_logs_e_dados_sem_repetir() {
    let h = sandbox("logs-fontes");
    write(&h.join("data/logs/b.log"), "");
    write(&h.join("data/a.log"), "");
    write(&h.join("data/b.log"), "");
    write(&h.join("data/c.txt"), "");
    std::fs::create_dir_all(h.join("data/d.log")).unwrap();
    assert_eq!(log_sources(&panel(&h)), ["a.log", "b.log"]);
}

#[test]
fn arquivo_le_as_ultimas_linhas_sem_ansi() {
    let h = sandbox("logs-arquivo");
    write(&h.join("data/x.log"), "um\r\n\x1b[31mdois\x1b[0m\ntres\n");
    let env = panel(&h);
    let v = read_log(&env, "file:x.log", "2", None, &[]).unwrap();
    assert_eq!(v, json!({"text": "dois\ntres"}));
    let v = read_log(&env, "file:x.log", "0", None, &[]).unwrap();
    assert_eq!(v["text"], json!("um\ndois\ntres"));
    let v = read_log(&env, "file:x.log", "-1", None, &[]).unwrap();
    assert_eq!(v["text"], json!("dois\ntres"));
    let e = read_log(&env, "file:y.log", "2", None, &[]).unwrap_err();
    assert!(
        e.message
            .starts_with("[Errno 2] No such file or directory: '")
    );
    let e = read_log(&env, "file:../x.log", "2", None, &[]).unwrap_err();
    assert_eq!(e.message, "arquivo inválido");
}

#[test]
fn job_devolve_offset_e_continua_dele() {
    let h = sandbox("logs-job");
    write(&h.join("data/logs/panel-jobs/j1.log"), "abc\x1b[1mdef");
    let env = panel(&h);
    let v = read_log(&env, "job:j1", "400", None, &[]).unwrap();
    assert_eq!(v, json!({"text": "abcdef", "offset": 10}));
    let v = read_log(&env, "job:j1", "400", Some("3"), &[]).unwrap();
    assert_eq!(v["text"], json!("def"));
    let v = read_log(&env, "job:sem", "400", None, &[]).unwrap();
    assert_eq!(v, json!({"text": "", "offset": 0}));
    assert_eq!(
        read_log(&env, "job:a/b", "1", None, &[])
            .unwrap_err()
            .message,
        "job inválido"
    );
}

#[test]
fn validacoes_e_documentos() {
    let h = sandbox("logs-valida");
    write(&h.join("data/notes.md"), "nota");
    let env = panel(&h);
    let e = read_log(&env, "file:x.log", "dez", None, &[]).unwrap_err();
    assert_eq!(e.message, "invalid literal for int() with base 10: 'dez'");
    assert_eq!(
        read_log(&env, "doc:notes.md", "1", None, &[]).unwrap()["text"],
        json!("nota")
    );
    assert_eq!(
        read_log(&env, "doc:STATUS.md", "1", None, &[]).unwrap()["text"],
        json!("(vazio)")
    );
    assert_eq!(
        read_log(&env, "doc:x.md", "1", None, &[])
            .unwrap_err()
            .message,
        "documento inválido"
    );
    assert_eq!(
        read_log(&env, "svc:x", "1", None, &[]).unwrap_err().message,
        "serviço inválido"
    );
    assert_eq!(
        read_log(&env, "unit:outro-x", "1", None, &[])
            .unwrap_err()
            .message,
        "unit inválida"
    );
    assert_eq!(
        read_log(&env, "docker:c", "1", None, &[])
            .unwrap_err()
            .message,
        "container inválido"
    );
    assert_eq!(
        read_log(&env, "nada", "1", None, &[]).unwrap_err().message,
        "fonte inválida"
    );
    assert!(unit_valida("hds", "hds-ms.a-b"));
    assert!(!unit_valida("hds", "hds-"));
}
