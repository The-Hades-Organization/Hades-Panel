use serde_json::json;

use super::*;

#[test]
fn notify_vazio_vira_objeto_e_lixo_e_erro() {
    assert_eq!(notify_de("").unwrap(), json!({}));
    assert_eq!(
        notify_de(r#"{"enabled":true}"#).unwrap(),
        json!({"enabled": true})
    );
    assert_eq!(notify_de("x").unwrap_err(), "notify.sh não devolveu JSON");
}

#[test]
fn onboard_falha_cita_a_ultima_linha_do_stderr() {
    assert_eq!(
        onboard_de(Some(1), "", "a\nb\n").unwrap_err(),
        "onboard.sh falhou: b"
    );
    assert_eq!(
        onboard_de(Some(0), " ", "").unwrap_err(),
        "onboard.sh falhou: sem saída"
    );
    assert_eq!(
        onboard_de(Some(0), r#"{"complete":true}"#, "").unwrap(),
        json!({"complete": true})
    );
}

#[test]
fn plugins_le_a_ultima_linha_e_marca_local() {
    let v = plugins_de(Some(0), "log\n{\"ok\":true,\"packs\":[]}\n").unwrap();
    assert_eq!(v, json!({"ok": true, "packs": [], "remote": false}));
    assert_eq!(
        plugins_de(Some(1), r#"{"ok":false,"error":"sem pasta"}"#).unwrap_err(),
        "sem pasta"
    );
    assert_eq!(
        plugins_de(Some(0), r#"{"ok":false}"#).unwrap_err(),
        "falha ao listar"
    );
    assert_eq!(
        plugins_de(Some(0), "[1]").unwrap_err(),
        "o plugins.sh falhou (detalhe no log do painel)"
    );
}
