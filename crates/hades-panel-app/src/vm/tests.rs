use serde_json::json;

use super::*;

#[test]
fn trilho_tem_os_cinco_grupos_e_as_paginas_prontas() {
    let grupos: Vec<&str> = GRUPOS.iter().map(|g| g.0).collect();
    assert_eq!(
        grupos,
        ["Ambiente", "Loop", "Operação", "Ferramentas", "Sistema"]
    );
    let prontas: Vec<&str> = paginas().filter(|p| p.pronta).map(|p| p.id).collect();
    assert_eq!(prontas, [VISAO_GERAL, "loop", "queue", "usage"]);
}

#[test]
fn ids_e_rotulos_sao_unicos() {
    let todas: Vec<&Pagina> = paginas().collect();
    for (i, a) in todas.iter().enumerate() {
        for b in &todas[i + 1..] {
            assert_ne!(a.id, b.id);
            assert_ne!(a.rotulo, b.rotulo);
        }
    }
}

#[test]
fn ferramentas_sao_as_seis() {
    let f: Vec<&str> = GRUPOS[3].1.iter().map(|p| p.rotulo).collect();
    assert_eq!(
        f,
        [
            "Argos", "Moirai", "Cerberus", "Minos", "Thanatos", "Caronte"
        ]
    );
}

#[test]
fn rotulo_do_trilho_volta_a_pagina() {
    assert_eq!(por_rotulo("Fila de gates").map(|p| p.id), Some("queue"));
    assert_eq!(por_rotulo("Inexistente"), None);
}

#[test]
fn paleta_navega_para_cada_pagina() {
    let itens = itens_paleta();
    assert_eq!(itens.len(), paginas().count());
    for item in &itens {
        let destino = destino_paleta(&item.id).expect("item de navegação");
        assert_eq!(item.label.as_ref(), destino.rotulo);
        assert_eq!(item.group.as_ref(), "Páginas");
    }
    assert_eq!(destino_paleta("overview"), None);
    assert_eq!(destino_paleta("ir:nada"), None);
}

#[test]
fn ctrl_k_abre_a_paleta() {
    assert!(abre_paleta("k", true, false));
    assert!(abre_paleta("k", false, true));
    assert!(abre_paleta("K", true, false));
    assert!(!abre_paleta("k", false, false));
    assert!(!abre_paleta("j", true, false));
}

fn p(t_ms: i64) -> Ponto {
    Ponto {
        t_ms,
        memoria: t_ms as f64,
        fila: 0.0,
        agentes: 0.0,
    }
}

#[test]
fn historico_guarda_so_a_ultima_hora() {
    let mut h = Historico::default();
    assert!(h.is_empty());
    h.registrar(p(0));
    h.registrar(p(30 * 60 * 1000));
    h.registrar(p(JANELA_MS));
    assert_eq!(h.len(), 3);
    h.registrar(p(JANELA_MS + 1));
    assert_eq!(
        h.serie(|x| x.t_ms as f64),
        [1_800_000.0, 3_600_000.0, 3_600_001.0]
    );
}

#[test]
fn historico_ignora_ponto_que_volta_no_tempo() {
    let mut h = Historico::default();
    h.registrar(p(10));
    h.registrar(p(5));
    assert_eq!(h.len(), 1);
}

#[test]
fn estado_geral_segue_o_overall_do_web() {
    assert_eq!(estado_geral("up"), ("No ar", Tom::Ok));
    assert_eq!(estado_geral("partial"), ("Parcial", Tom::Aviso));
    assert_eq!(estado_geral("paused"), ("Pausado", Tom::Parado));
    assert_eq!(estado_geral("busy"), ("Em operação", Tom::Ocupado));
    assert_eq!(estado_geral("stopped"), ("Parado", Tom::Parado));
    assert_eq!(estado_geral("qualquer"), ("Parado", Tom::Parado));
}

fn snapshot() -> serde_json::Value {
    json!({
        "overall": "partial", "paused": false,
        "services": [
            { "name": "api", "enabled": true, "ready": true },
            { "name": "web", "enabled": true, "ready": false },
            { "name": "bot", "enabled": false, "ready": false },
        ],
        "slice": { "used": 7.0 * 1024.0 * 1024.0 * 1024.0, "max": 10.0 * 1024.0 * 1024.0 * 1024.0 },
        "config": { "HADES_SLICE_MAX": "10G" },
        "herdr": { "working": 3, "blocked": 2 },
        "extras": { "panel": { "state": "active" } },
        "instance": { "project": "demo" },
        "versionCheck": { "local": "7.0.0" },
        "gates": [],
    })
}

fn fila() -> serde_json::Value {
    json!({
        "available": true,
        "gates": [ { "state": "running" }, { "state": "waiting" }, { "state": "waiting" } ],
        "agents": [ { "pid": 1 }, { "pid": 2 } ],
    })
}

#[test]
fn visao_geral_monta_estado_fatos_e_leituras() {
    let v = VisaoGeral::de(&snapshot(), &fila());
    assert_eq!(v.estado, "Parcial");
    assert_eq!(v.tom, Tom::Aviso);
    assert_eq!(v.fatos, "1 de 2 serviços prontos · 1 rodando · 2 na fila");
    assert_eq!(v.pedem, 2);
    let l: Vec<(&str, &str, &str)> = v
        .leituras
        .iter()
        .map(|l| (l.rotulo, l.valor.as_str(), l.detalhe.as_str()))
        .collect();
    assert_eq!(
        l,
        [
            ("Agentes ativos", "2", "3 trabalhando no herdr"),
            ("Serviços prontos", "1 / 2", "1 desligado"),
            ("Memória do Hades", "7,0 GB", "70% do teto de 10,0 GB"),
            ("Fila de gates", "3", "1 rodando · 2 esperando"),
        ]
    );
    assert_eq!(v.leituras[2].tom, Some(Tom::Aviso));
    assert_eq!(v.leituras[3].tom, Some(Tom::Aviso));
}

#[test]
fn visao_geral_sem_infra() {
    let snap = json!({
        "overall": "stopped", "paused": true, "services": [],
        "slice": { "used": 0, "max": null }, "config": {}, "herdr": null, "gates": [],
    });
    let v = VisaoGeral::de(&snap, &json!({ "available": false, "gates": [] }));
    assert_eq!(v.estado, "Parado");
    assert_eq!(v.fatos, "pausado · nenhum serviço ligado · nada rodando");
    assert_eq!(v.pedem, 0);
    assert_eq!(v.leituras[0].detalhe, "herdr fora do ar");
    assert_eq!(v.leituras[2].valor, "0,0 GB");
    assert_eq!(v.leituras[2].detalhe, "nenhum serviço rodando · teto —");
}

#[test]
fn fila_indisponivel_cai_nos_gates_do_snapshot() {
    let snap = json!({ "gates": [ { "state": "running" } ] });
    assert_eq!(contagem_fila(&snap, &json!({ "available": false })), (1, 0));
}

#[test]
fn ponto_da_amostra() {
    let pt = ponto(42, &snapshot(), &fila());
    assert_eq!(pt.t_ms, 42);
    assert_eq!(pt.fila, 3.0);
    assert_eq!(pt.agentes, 2.0);
}

#[test]
fn barra_de_estado() {
    let (e, d) = barra_estado(&snapshot());
    assert_eq!(e, "demo · herdr: 3 agentes · painel web no ar");
    assert_eq!(d, format!("hades 7.0.0 · hades-core {HADES_CORE}"));
    let (e, d) = barra_estado(&json!({}));
    assert_eq!(e, "herdr fora do ar · painel web parado");
    assert_eq!(d, format!("hades — · hades-core {HADES_CORE}"));
}

#[test]
fn tarefa_descarta_geracao_velha() {
    let mut t = Tarefa::default();
    let g = t.iniciar().expect("livre");
    assert!(t.carregando());
    assert_eq!(t.iniciar(), None);
    t.invalidar();
    assert!(!t.concluir(g));
    let g2 = t.iniciar().expect("livre depois de invalidar");
    assert!(t.concluir(g2));
    assert!(!t.carregando());
}

#[test]
fn decimal_usa_virgula() {
    assert_eq!(decimal(1.26), "1,3");
    assert_eq!(decimal(3.0), "3,0");
}
