use hades_ui::{Comando, FerramentaInfo, IconName, TelaInfo};

use super::*;

const TELAS_A: &[TelaInfo] = &[
    TelaInfo {
        id: "fila",
        rotulo: "Fila",
        icone: IconName::ListOrdered,
    },
    TelaInfo {
        id: "historico",
        rotulo: "Histórico",
        icone: IconName::History,
    },
];

const TELAS_B: &[TelaInfo] = &[TelaInfo {
    id: "hist",
    rotulo: "Histórico",
    icone: IconName::History,
}];

const COMANDOS: &[Comando] = &[
    Comando {
        id: "recarregar",
        grupo: "Fila",
        rotulo: "Recarregar a fila",
        descricao: Some("Relê a fila do disco"),
        icone: IconName::RefreshCw,
        atalho: Some("Ctrl R"),
    },
    Comando {
        id: "limpar",
        grupo: "Fila",
        rotulo: "Limpar",
        descricao: None,
        icone: IconName::Package,
        atalho: None,
    },
];

const A: Ligada = Ligada {
    info: FerramentaInfo {
        id: "alfa",
        titulo: "Alfa",
        icone: IconName::Package,
        telas: TELAS_A,
        comandos: COMANDOS,
    },
    telas: TELAS_A,
};

const B: Ligada = Ligada {
    info: FerramentaInfo {
        id: "beta",
        titulo: "Beta",
        icone: IconName::Scale,
        telas: TELAS_B,
        comandos: &[],
    },
    telas: TELAS_B,
};

#[test]
fn sem_ferramenta_ligada_nao_ha_grupo_nem_item_na_paleta() {
    assert!(grupos(&[], &["Loop"]).is_empty());
    assert!(itens_paleta(&[]).is_empty());
}

#[test]
fn um_grupo_por_ferramenta_com_as_telas_dela() {
    let g = grupos(&[A, B], &[]);
    assert_eq!(
        g.iter().map(|g| g.titulo).collect::<Vec<_>>(),
        ["Alfa", "Beta"]
    );
    assert_eq!(
        g[0].itens.iter().map(|i| i.tela).collect::<Vec<_>>(),
        ["fila", "historico"]
    );
}

#[test]
fn rotulo_repetido_ganha_o_nome_da_ferramenta() {
    let g = grupos(&[A, B], &["Fila"]);
    assert_eq!(g[0].itens[0].rotulo, "Fila · Alfa");
    assert_eq!(g[0].itens[1].rotulo, "Histórico");
    assert_eq!(g[1].itens[0].rotulo, "Histórico · Beta");
    let i = por_rotulo(&g, "Histórico · Beta").expect("item da Beta");
    assert_eq!((i.ferramenta, i.tela), ("beta", "hist"));
    assert!(por_rotulo(&g, "Fila").is_none());
    assert_eq!(
        item(&g, "alfa", "fila").map(|i| i.rotulo.as_str()),
        Some("Fila · Alfa")
    );
}

#[test]
fn paleta_tem_telas_e_comandos_das_ferramentas() {
    let itens = itens_paleta(&[A, B]);
    let ids: Vec<&str> = itens.iter().map(|i| i.id.as_ref()).collect();
    assert_eq!(
        ids,
        [
            "tela:alfa/fila",
            "tela:alfa/historico",
            "cmd:alfa/recarregar",
            "cmd:alfa/limpar",
            "tela:beta/hist"
        ]
    );
    let r = &itens[2];
    assert_eq!(r.group.as_ref(), "Alfa · Fila");
    assert_eq!(r.label.as_ref(), "Recarregar a fila");
    assert_eq!(
        r.desc.as_ref().map(|d| d.as_ref()),
        Some("Relê a fila do disco")
    );
    assert_eq!(r.shortcut.as_ref().map(|d| d.as_ref()), Some("Ctrl R"));
    assert!(itens[3].desc.is_none() && itens[3].shortcut.is_none());
    assert_eq!(itens[0].label.as_ref(), "Alfa: Fila");
}

#[test]
fn destino_volta_do_id_da_paleta() {
    assert_eq!(
        destino("tela:alfa/fila"),
        Some(Destino::Tela {
            ferramenta: "alfa",
            tela: "fila"
        })
    );
    assert_eq!(
        destino("cmd:alfa/recarregar"),
        Some(Destino::Comando {
            ferramenta: "alfa",
            comando: "recarregar"
        })
    );
    assert_eq!(destino("ir:loop"), None);
    assert_eq!(destino("tela:sem-barra"), None);
}
