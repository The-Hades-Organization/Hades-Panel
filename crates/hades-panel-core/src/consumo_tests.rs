use moirai_core::limits::Atual;
use serde_json::json;

use super::*;
use crate::teste_vivo::{Falso, panel, sandbox, write};

#[test]
fn sem_arquivo_dispara_o_coletor() {
    let h = sandbox("uso-sem");
    let vivo = Falso::default();
    let v = usage(&panel(&h), &vivo);
    assert_eq!(v, json!({"data": null, "age": null, "refreshing": true}));
    assert_eq!(*vivo.coletas.borrow(), vec!["7".to_owned()]);
}

#[test]
fn arquivo_novo_nao_dispara() {
    let h = sandbox("uso-novo");
    write(&h.join(".claude/hades/usage/latest.json"), r#"{"total":3}"#);
    let vivo = Falso::default();
    let v = usage_at(&panel(&h), &vivo, now_s() + 10.0);
    assert_eq!(v["data"], json!({"total": 3}));
    assert_eq!(v["refreshing"], json!(false));
    assert!(vivo.coletas.borrow().is_empty());
    let v = usage_at(&panel(&h), &vivo, now_s() + 1000.0);
    assert_eq!(v["refreshing"], json!(true));
}

#[test]
fn painel_arredonda_e_formata() {
    let atual = Atuais {
        five: Some(Atual {
            pct: 42.46,
            resets_at: 1_795_000_000.0,
            at: "2026-11-23T15:00:00.000Z".into(),
        }),
        seven: None,
    };
    let now = hades_core::journal::parse_iso("2026-11-23T15:00:30.400Z").unwrap() as f64;
    let v = painel_de_limites(&atual, now);
    assert_eq!(v["coleta"], json!(true));
    let j = &v["janelas"][0];
    assert_eq!(j["janela"], json!("five"));
    assert_eq!(j["pct"], json!(42.5));
    assert_eq!(j["idadeS"], json!(30));
    assert_eq!(
        painel_de_limites(&Atuais::default(), now),
        json!({"coleta": false, "janelas": []})
    );
}
