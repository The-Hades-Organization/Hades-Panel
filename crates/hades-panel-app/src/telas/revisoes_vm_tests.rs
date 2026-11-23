use serde_json::json;

use super::*;

#[test]
fn numeros_no_formato_do_web() {
    assert_eq!(usd(1.5), "US$ 1,50");
    assert_eq!(tok(950.0), "950");
    assert_eq!(tok(12_400.0), "12k");
    assert_eq!(tok(1_250_000.0), "1,2M");
    assert_eq!(tok(15_000_000.0), "15M");
    assert_eq!(dur(5.0), "5 s");
    assert_eq!(dur(125.0), "2 min 5 s");
    assert_eq!(dur(3720.0), "1 h 2 min");
}

#[test]
fn resultado_por_estado_e_veredito() {
    assert_eq!(
        Resultado::de(&json!({"state": "falhou", "error": "sem token"})),
        Resultado::Falhou(Some("sem token".into()))
    );
    assert_eq!(
        Resultado::de(&json!({"state": "interrompido"})),
        Resultado::Interrompida
    );
    assert_eq!(
        Resultado::de(
            &json!({"state": "ok", "verdict": "red", "blocking": 2, "important": 1, "inline": 3})
        ),
        Resultado::Veredito {
            rotulo: "Bloqueia",
            tom: Tom::Ruim,
            contagens: Some("2 bloqueantes · 1 importante · 3 in-line".into()),
        }
    );
    assert_eq!(
        Resultado::de(&json!({"state": "ok"})),
        Resultado::Veredito {
            rotulo: "Concluída, sem veredito",
            tom: Tom::Parado,
            contagens: None
        }
    );
}

#[test]
fn publicacao_confirmada_pedida_ou_local() {
    assert_eq!(
        publicacao(&json!({"posted": true, "post": true})).0,
        "comentada no GitHub"
    );
    assert_eq!(
        publicacao(&json!({"post": true, "state": "ok"})),
        ("publicação não confirmada", Some(Tom::Aviso))
    );
    assert_eq!(
        publicacao(&json!({"post": true, "state": "rodando"})).0,
        "publicação pedida"
    );
    assert_eq!(publicacao(&json!({})).0, "só local");
}

#[test]
fn separa_em_curso_do_historico() {
    let reviews = json!({
        "maxParallel": 3,
        "items": [
            {"id": "a", "pr": 10, "state": "rodando", "fase": "fila", "cmd": "review-pr", "model": "opus", "effort": "medium",
             "started": "2026-11-23T10:05:00Z"},
            {"id": "b", "pr": 11, "state": "ok", "verdict": "green", "costUsd": 0.42, "durationMs": 61000,
             "tokens": {"input": 1000, "output": 500}, "report": true, "cmd": "review-agents", "model": "sonnet", "effort": "high"},
        ],
    });
    let r = Revisoes::de(
        &reviews,
        &json!({"active": false, "status": null, "rounds": []}),
    );
    assert_eq!(r.linha, "1 em curso de até 3 em paralelo");
    assert_eq!(r.em_curso[0].fase, "na fila");
    assert_eq!(r.em_curso[0].fio, FioState::Parado);
    assert_eq!(r.em_curso[0].comecou, "23/11 10:05Z");
    assert_eq!(r.em_curso[0].como, "/review-pr · Opus · esforço médio");
    let h = &r.historico[0];
    assert_eq!(h.pr, "#11");
    assert_eq!(h.meta, ["US$ 0,42", "1 min 1 s", "2k tokens"]);
    assert!(h.relatorio);
    assert!(r.loop_.is_none());
}

#[test]
fn loop_ativo_com_rodadas() {
    let l = Loop::de(&json!({
        "active": true,
        "status": {"state": "rodando", "round": 4, "nextAt": "2026-11-23T11:00:00Z", "reason": null},
        "rounds": [{"round": 3, "startedAt": "2026-11-23T10:00:00Z", "outcome": "ok", "reviewed": 2, "skipped": 1, "costUsd": 1.2}],
    }))
    .expect("loop");
    assert_eq!(l.estado, "rodando");
    assert_eq!(l.tom, Tom::Ocupado);
    assert_eq!(l.detalhe, "rodada 4 · próxima 23/11 11:00Z");
    assert_eq!(l.rodadas[0].titulo, "Rodada 3 · 23/11 10:00Z");
    assert_eq!(
        l.rodadas[0].resumo,
        "ok · 2 revisadas · 1 pulada · US$ 1,20"
    );
}
