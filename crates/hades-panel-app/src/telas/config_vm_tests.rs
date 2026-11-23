use hades_panel_core::config::{Change, ImportPlan};
use serde_json::json;

use super::*;

fn estado() -> Value {
    json!({
        "path": "/d/config.env",
        "items": [
            {"key": "A_BOOL", "type": "bool", "group": "Painel", "label": "Liga", "default": "0", "value": "1", "changed": true},
            {"key": "A_ENUM", "type": "enum:x,y", "group": "Painel", "label": "Modo", "default": "x", "value": "x", "changed": false},
            {"key": "B_INT", "type": "int:1:8", "group": "Builds", "label": "Jobs", "default": "2", "value": "2", "changed": false},
            {"key": "B_TOKEN", "type": "secret", "group": "Builds", "label": "", "default": "", "value": "", "changed": false},
        ],
    })
}

fn sem_env(_: &str) -> Option<String> {
    None
}

#[test]
fn controle_pelo_tipo() {
    assert_eq!(Controle::de("bool"), Controle::Switch);
    assert_eq!(
        Controle::de("enum:a,b"),
        Controle::Select(vec!["a".into(), "b".into()])
    );
    assert_eq!(Controle::de("int:1:64"), Controle::Numero(1, 64));
    assert_eq!(Controle::de("int:x"), Controle::Texto);
    assert_eq!(Controle::de("secret"), Controle::Segredo);
    assert_eq!(Controle::de("size"), Controle::Texto);
    assert!(ligado("1") && !ligado("0"));
}

#[test]
fn agrupa_na_ordem_e_diz_a_origem() {
    let env = |k: &str| (k == "B_INT").then(|| "4".to_owned());
    let c = Config::de(&estado(), &env);
    assert_eq!(c.caminho, "/d/config.env");
    let nomes: Vec<&str> = c.grupos.iter().map(|g| g.nome.as_str()).collect();
    assert_eq!(nomes, ["Painel", "Builds"]);
    let i: Vec<&Item> = c.itens().collect();
    assert_eq!(i[0].origem, SettingOrigin::File);
    assert_eq!(i[1].origem, SettingOrigin::Default);
    assert_eq!(
        (i[2].origem, i[2].valor.as_str()),
        (SettingOrigin::Env, "4")
    );
    assert!(!i[2].editavel());
    assert_eq!(i[3].rotulo, "B_TOKEN");
    assert!(!i[3].editavel());
}

#[test]
fn pendentes_so_do_que_mudou_e_e_editavel() {
    let c = Config::de(&estado(), &sem_env);
    let editado = |i: &Item| match i.chave.as_str() {
        "A_BOOL" => Some("1".to_owned()),
        "A_ENUM" => Some("y".to_owned()),
        "B_TOKEN" => Some("segredo".to_owned()),
        _ => None,
    };
    let p = c.pendentes(&editado);
    assert_eq!(p, [("A_ENUM".to_owned(), "y".to_owned())]);
    assert_eq!(texto_plano(&p), "A_ENUM=y\n");
}

#[test]
fn previa_do_plano() {
    let plano = ImportPlan {
        changes: vec![Change {
            key: "A_ENUM".into(),
            label: "Modo".into(),
            group: "Painel".into(),
            apply: "agora".into(),
            old: "x".into(),
            new: "y".into(),
        }],
        apply: vec!["agora".into()],
        ..ImportPlan::default()
    };
    let p = Previa::de(&plano);
    assert_eq!(p.mudancas, ["Modo (A_ENUM): x → y"]);
    assert!(p.pode_gravar());
    let ruim = Previa::de(&ImportPlan {
        invalid: vec![("B_INT".into(), "fora da faixa".into())],
        ..plano
    });
    assert_eq!(ruim.recusas, ["B_INT: fora da faixa"]);
    assert!(!ruim.pode_gravar());
}
