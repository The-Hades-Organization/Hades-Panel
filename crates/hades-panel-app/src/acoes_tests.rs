use super::*;

const LISTA: &str = r#"[
  { "id": "update", "label": "Atualizar (update.sh)", "ops": [], "confirm": true },
  { "id": "pause", "label": "Pausar ou retomar o Hades", "ops": ["pause", "resume"], "confirm": ["pause"] },
  { "id": "status", "label": "Gerar STATUS.md", "ops": [], "confirm": false },
  { "label": "sem id" }
]"#;

#[test]
fn lista_le_o_catalogo_e_ignora_item_sem_id() {
    let l = lista_de(LISTA).expect("lista");
    assert_eq!(
        l.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["update", "pause", "status"]
    );
    assert_eq!(l[1].ops, ["pause", "resume"]);
    assert_eq!(l[1].confirma, Confirma::Ops(vec!["pause".into()]));
    assert_eq!(l[0].confirma, Confirma::Sempre);
    assert_eq!(l[2].confirma, Confirma::Nunca);
}

#[test]
fn skill_sem_panel_action_deixa_a_lista_indisponivel() {
    let uso = "Uso: hades panel serve [--port <porta>]\n";
    assert_eq!(lista_de(uso), Err(INDISPONIVEL.to_owned()));
    assert_eq!(lista_de(r#"{"error":"x"}"#), Err(INDISPONIVEL.to_owned()));
}

#[test]
fn confirmacao_por_acao_e_por_op() {
    let l = lista_de(LISTA).expect("lista");
    assert!(l[0].pede_confirmacao(None));
    assert!(l[1].pede_confirmacao(Some("pause")));
    assert!(!l[1].pede_confirmacao(Some("resume")));
    assert!(!l[1].pede_confirmacao(None));
    assert!(!l[2].pede_confirmacao(None));
}

#[test]
fn paleta_tem_um_item_por_op_e_volta_ao_pedido() {
    let l = lista_de(LISTA).expect("lista");
    let itens = itens_paleta(&l);
    let ids: Vec<&str> = itens.iter().map(|i| i.id.as_ref()).collect();
    assert_eq!(
        ids,
        [
            "acao:update",
            "acao:pause?pause",
            "acao:pause?resume",
            "acao:status"
        ]
    );
    assert_eq!(itens[1].label.as_ref(), "Pausar ou retomar o Hades: pause");
    assert!(itens.iter().all(|i| i.group.as_ref() == "Ações"));
    let p = pedido("acao:pause?resume").expect("pedido");
    assert_eq!(p.op.as_deref(), Some("resume"));
    assert_eq!(p.args(), ["op=resume"]);
    let p = pedido("acao:update").expect("pedido");
    assert_eq!((p.id.as_str(), p.args().len()), ("update", 0));
    assert_eq!(pedido("ir:loop"), None);
}

#[test]
fn resultado_pelo_codigo_de_saida() {
    assert_eq!(
        resultado(
            Some(0),
            r#"{"job":"j-12","title":"Atualizar","exclusive":true}"#
        ),
        Resultado::Lancada {
            job: "j-12".into(),
            titulo: "Atualizar".into(),
            exclusiva: true
        }
    );
    assert_eq!(
        resultado(Some(1), r#"{"error":"ambiente ocupado","code":"busy"}"#),
        Resultado::Recusada {
            erro: "ambiente ocupado".into(),
            codigo: "busy".into()
        }
    );
    let desconhecida =
        r#"{"error":"ação desconhecida: x","code":"unknown_action","actions":["status"]}"#;
    assert_eq!(resultado(Some(2), desconhecida), Resultado::Indisponivel);
    assert_eq!(
        resultado(Some(0), "Uso: hades panel serve"),
        Resultado::Indisponivel
    );
    assert!(matches!(resultado(Some(3), ""), Resultado::Falhou(_)));
    assert!(matches!(resultado(None, ""), Resultado::Falhou(_)));
}
