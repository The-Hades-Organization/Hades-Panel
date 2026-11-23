use serde_json::json;

use super::*;

fn usage() -> Value {
    json!({
        "age": 120,
        "refreshing": true,
        "data": {
            "days": 7,
            "since": "2026-11-16",
            "total": { "cost": 700, "turns": 12_345, "pollCost": 70, "rewriteCost": 25 },
            "baseline": { "perDay": 80, "savedAt": "2026-11-01" },
            "byRole": [
                { "role": "impl", "sessions": 3, "cost": 300, "share": 42.9, "pollShare": 1.5, "turns": 100, "ctxAvg": 100_000 },
                { "role": "orchestrator", "sessions": 1, "cost": 400, "share": 57.1, "pollShare": 9, "turns": 50, "ctxAvg": 50_000 }
            ],
            "byDay": [{ "day": "2026-11-22", "cost": 90, "pollCost": 5 }],
            "overCap": [{ "cwd": "/home/acme/demo", "role": "impl · sub", "turns": 40, "ctxMax": 260_000, "ctxCap": 200_000 }],
            "byAgent": [{ "name": "demo-impl", "pr": "1234", "role": "impl", "models": { "sonnet": 1 }, "cost": 300, "turns": 1200, "ctxAvg": 100_000 }]
        }
    })
}

#[test]
fn rotulos_de_papel_e_datas() {
    assert_eq!(rotulo_papel("impl"), "Implementação");
    assert_eq!(rotulo_papel("review · x"), "Revisão · subagentes");
    assert_eq!(rotulo_papel("outro"), "outro");
    assert_eq!(ddmm("2026-11-23"), "23/11");
    assert_eq!(ddmm("ontem"), "ontem");
}

#[test]
fn lendo_e_calculando() {
    assert_eq!(Estado::de(None, &Value::Null), Estado::Lendo);
    assert_eq!(
        Estado::de(Some(&json!({ "data": null })), &Value::Null),
        Estado::Calculando
    );
}

fn pronto(limites: &Value) -> ConsumoVm {
    match Estado::de(Some(&usage()), limites) {
        Estado::Pronto(vm) => *vm,
        e => panic!("esperava Pronto, veio {e:?}"),
    }
}

#[test]
fn pronto_com_resumo_e_leituras() {
    let vm = pronto(&Value::Null);
    assert_eq!(
        vm.principal,
        "US$ 100 por dia (equivalente em API, estimativa)"
    );
    assert_eq!(vm.resto[0], "+25% sobre a linha de base");
    assert_eq!(vm.resto[1], "7 dias desde 16/11");
    assert_eq!(vm.resto[2], "12.345 turnos");
    assert_eq!(vm.resto[3], "atualizado há 2 min, atualizando…");
    assert_eq!(vm.leituras[1].valor, "10%");
    assert!(vm.leituras[1].aviso);
    assert_eq!(vm.leituras[2].valor, "100k");
    assert!(!vm.leituras[2].aviso);
    assert!(vm.leituras[3].aviso);
    assert_eq!(vm.papeis_resumo, "US$ 700 em 7 dias");
    assert_eq!(vm.papeis[0][3], "42,9%");
    assert_eq!(vm.dias[0].rotulo, "22/11");
    assert_eq!(vm.acima_teto[0][0], "demo");
    assert_eq!(vm.acima_teto[0][1], "Implementação · subagentes");
    assert_eq!(vm.agentes[0].nome, "demo-impl #1234");
    assert_eq!(vm.agentes[0].turnos, "1.200");
    assert_eq!(vm.agentes[0].detalhe.meta, "Implementação · #1234");
    assert_eq!(vm.agentes_resumo, "por agente do herdr");
}

#[test]
fn janelas_sem_resposta_sem_coleta_e_com_coleta() {
    assert_eq!(pronto(&Value::Null).janelas, None);
    assert_eq!(
        pronto(&json!({ "coleta": false })).janelas,
        Some(Vec::new())
    );
    let j = pronto(&json!({
        "coleta": true,
        "janelas": [{ "rotulo": "5 h", "pct": 85, "renova": "14:00", "idadeS": 60 }]
    }))
    .janelas
    .expect("janelas");
    assert_eq!(j[0].texto, "renova 14:00 · medido há 1 min");
    assert!(j[0].aviso);
}

#[test]
fn sem_linha_de_base() {
    let mut u = usage();
    u["data"]["baseline"] = Value::Null;
    let Estado::Pronto(vm) = Estado::de(Some(&u), &Value::Null) else {
        panic!("esperava Pronto");
    };
    assert_eq!(vm.resto[0], "sem linha de base");
    assert_eq!(vm.leituras[0].detalhe, "sem linha de base");
}
