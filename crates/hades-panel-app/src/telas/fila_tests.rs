use serde_json::json;

use super::*;

fn fila() -> Value {
    json!({
        "available": true,
        "gates": [
            { "pid": 101, "label": "demo-test", "kind": "test", "clone": "/home/acme/demo", "state": "running",
              "runningFor": 90, "rss": 2_147_483_648u64, "cpu": 87.6, "cmd": "cargo test", "step": "compilando",
              "stepAge": 30, "stepMedian": 120 },
            { "pid": 102, "label": "demo-lint", "kind": "lint", "state": "waiting", "age": 300 },
            { "pid": 103, "label": "hades-build", "kind": "build", "state": "remote:worker-1", "age": 20 }
        ],
        "host": { "memAvailable": 8.0 * GB, "memTotal": 32.0 * GB, "load": [1.25], "infraRss": 4.0 * GB },
        "budget": { "reserveGb": 2, "budgetGb": 16 },
        "journal": [
            ["2026-11-23T10:00:00", "enqueue", "test", "demo-test", "2G", "/home/acme/demo", "waited=0s"],
            ["2026-11-23T10:05:00", "done", "test", "demo-test", "2G", "/home/acme/demo", "rc=0", "dur=300"],
            ["2026-11-23T10:06:00", "done", "lint", "demo-lint", "2G", "/home/acme/demo", "rc=137", "dur=60"]
        ],
        "remote": { "workers": [{ "target": "worker-1", "reachable": false }] }
    })
}

#[test]
fn kv_e_texto_de_saida() {
    let row = [
        json!("t"),
        json!("done"),
        json!(""),
        json!(""),
        json!(""),
        json!(""),
        json!("rc=1"),
        json!("dur=5"),
    ];
    assert_eq!(kv(&row, "rc").as_deref(), Some("1"));
    assert_eq!(kv(&row, "host"), None);
    assert_eq!(texto_saida("137"), "estourou a memória (137)");
    assert_eq!(texto_saida("2"), "falhou (saída 2)");
}

#[test]
fn separa_rodando_esperando_e_remoto() {
    let vm = FilaVm::de(&fila());
    assert!(vm.disponivel);
    assert_eq!(vm.principal, "1 rodando");
    assert_eq!(vm.rodando[0].memoria, "2,0 GB");
    assert_eq!(vm.rodando[0].cpu, "88%");
    assert_eq!(vm.rodando[0].passo_tempo, "30 s de ~2 min");
    assert_eq!(vm.rodando[0].meta, "test · demo · local");
    assert_eq!(vm.esperando[0].tempo, "5 min");
    assert_eq!(vm.esperando[0].memoria, "—");
    assert_eq!(vm.remotos[0].estado, Estado::Remoto("worker-1".into()));
    assert!(vm.remotos[0].meta.contains("Hades"));
    assert_eq!(vm.resto[0], "1 esperando, o mais antigo há 5 min");
    assert!(vm.resto[1].contains("worker remoto"));
    assert_eq!(vm.resto[2], "gates usam 2,0 GB de 16 GB de orçamento");
    assert!(!vm.abaixo_do_minimo);
    assert_eq!(
        vm.workers,
        [("worker-1".into(), false, "sem resposta".into())]
    );
    let ids: Vec<&str> = vm.lista().map(|g| g.pid.as_str()).collect();
    assert_eq!(ids, ["101", "103", "102"]);
}

#[test]
fn concluidos_e_journal_com_rc_137() {
    let vm = FilaVm::de(&fila());
    assert_eq!(vm.concluidos.len(), 2);
    assert_eq!(vm.concluidos[0].label, "demo-lint");
    assert!(vm.concluidos[0].ruim);
    assert_eq!(
        vm.concluidos[0].resumo,
        "lint · local · estourou a memória (137) · 1 min"
    );
    assert_eq!(vm.concluidos[1].quando, "10:05:00");
    assert!(!vm.concluidos[1].ruim);
    assert_eq!(vm.journal[0].evento, "falhou");
    assert_eq!(vm.journal[0].tom, Tone::Bad);
    assert_eq!(vm.journal[0].resultado, "estourou a memória (137) · 60");
    assert_eq!(vm.journal[2].evento, "entrou");
    assert_eq!(vm.journal[2].resultado, "esperou 0s");
}

#[test]
fn indisponivel_mantem_os_gates_em_espera() {
    let vm =
        FilaVm::de(&json!({ "available": false, "gates": [{ "pid": 7, "state": "waiting" }] }));
    assert!(!vm.disponivel);
    assert_eq!(vm.esperando.len(), 1);
}

#[test]
fn detalhe_do_gate() {
    let vm = FilaVm::de(&fila());
    let d = vm.detalhe("101").expect("gate");
    assert_eq!(d.titulo, "demo-test");
    assert!(
        d.linhas
            .contains(&("Estado".into(), "rodando há 2 min".into()))
    );
    let e = vm.detalhe("102").expect("gate");
    assert!(!e.linhas.iter().any(|(r, _)| r == "Tempo do passo"));
    assert!(vm.detalhe("999").is_none());
}

#[test]
fn historico_guarda_so_a_ultima_hora() {
    let mut h = HistoricoFila::default();
    h.registrar(0, &json!({ "available": false }));
    assert!(h.is_empty());
    h.registrar(0, &fila());
    h.registrar(30 * 60 * 1000, &fila());
    assert_eq!(h.len(), 2);
    h.registrar(61 * 60 * 1000, &fila());
    assert_eq!(h.len(), 2);
    assert_eq!(h.serie(|p| p.rodando), [1.0, 1.0]);
    assert_eq!(h.serie(|p| p.livre_gb), [8.0, 8.0]);
}
