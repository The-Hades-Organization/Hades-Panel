use serde_json::json;

use super::*;

const AGORA: f64 = 1_700_000_000.0;

fn fila() -> Value {
    json!({
        "available": true,
        "loop": {
            "escalations": [{ "id": "e1", "pr": "1234", "why": "spec ambígua", "askedAt": "2023-11-14T21:13:20Z" }],
            "actionsByHour": [{ "hoursAgo": 0, "fixed": 2, "applied": 1 }, { "hoursAgo": 1, "read": 3 }],
            "verdicts24h": { "fixed": 2 },
            "deliveries": { "open": 1 },
            "recentPrs": [
                { "pr": "1234", "rounds": 2, "blocking": 1, "last": AGORA - 120.0 },
                { "pr": "1235", "rounds": 1, "blocking": 0, "review": "green", "last": 0 }
            ]
        },
        "agents": [{ "pid": 4242, "cwd": "/home/acme/demo", "age": 90, "rss": 104_857_600, "gates": ["test"], "locks": [] }],
        "locks": [{ "name": "demo", "mode": "exclusive", "agentPresent": true, "age": 30 }]
    })
}

#[test]
fn indisponivel_vira_padrao() {
    assert_eq!(
        LoopVm::de(&json!({ "available": false }), AGORA),
        LoopVm::default()
    );
}

#[test]
fn le_escalacoes_prs_sessoes_e_locks() {
    let vm = LoopVm::de(&fila(), AGORA);
    assert!(vm.disponivel);
    assert_eq!(vm.principal, "1 escalação esperando você");
    assert_eq!(vm.escalacoes[0].pr.as_deref(), Some("1234"));
    assert_eq!(vm.escalacoes[0].ha, "1 h");
    assert_eq!(vm.por_hora, [3.0, 3.0]);
    assert_eq!(vm.acoes, 6);
    assert_eq!(vm.legenda, ["Corrigido 2"]);
    assert_eq!(vm.resto[0], "6 ações em 24 h");
    assert_eq!(vm.resto[1], "1 com PR aberta");
    assert_eq!(vm.resto[2], "1 sessão viva, 1 lock");
    assert_eq!(vm.prs[0].situacao, "1 bloqueante");
    assert_eq!(vm.prs[0].meta, "2 rodadas · há 2 min");
    assert_eq!(vm.prs[1].situacao, "Aprova");
    assert_eq!(vm.prs[1].meta, "1 rodada · há —");
    assert_eq!(vm.sessoes[0].memoria, "100 MB");
    assert_eq!(vm.sessoes[0].em_posse, "test");
    assert!(vm.locks[0].com_agente);
}

#[test]
fn sem_escalacao_nem_entrega() {
    let vm = LoopVm::de(&json!({ "available": true }), AGORA);
    assert_eq!(vm.principal, "nenhuma escalação aberta");
    assert_eq!(vm.resto[1], "nenhuma entrega registrada");
}

#[test]
fn detalhes_de_pr_e_sessao() {
    let vm = LoopVm::de(&fila(), AGORA);
    let d = vm.detalhe_pr(0).expect("PR");
    assert_eq!(d.titulo, "PR #1234");
    assert!(d.linhas.contains(&("Rodadas de review".into(), "2".into())));
    let s = vm.detalhe_sessao(0).expect("sessão");
    assert_eq!(s.titulo, "demo");
    assert_eq!(s.meta, "pid 4242");
    assert!(vm.detalhe_pr(9).is_none());
}
