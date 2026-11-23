use serde_json::json;

use super::*;

fn agente(nome: &str, live: &str, state: &str) -> Value {
    json!({
        "name": nome, "mode": "impl", "state": state, "live": live, "branch": "demo/feat/DEMO-12",
        "pr": 1234, "worktree": "/srv/demo", "openQuestions": [], "questions": 0, "reportDone": false,
        "files": ["brief", "report"],
    })
}

#[test]
fn estado_vivo_segue_agent_live() {
    assert_eq!(
        estado_vivo("working"),
        ("trabalhando".into(), Tom::Ocupado, FioState::Fiando)
    );
    assert_eq!(estado_vivo("blocked").1, Tom::Aviso);
    assert_eq!(estado_vivo("fora").0, "fora do herdr");
    assert_eq!(estado_vivo("").0, "—");
    assert_eq!(estado_vivo("novo").0, "novo");
}

#[test]
fn indisponivel_sem_raiz_de_orquestracao() {
    let o = Orquestracao::de(&json!({
        "available": false, "agents": [], "consolidations": [], "decisions": [],
        "autonomy": {"level": 0, "label": "Pergunta tudo"},
    }));
    assert!(!o.disponivel);
    assert!(o.ativos.is_empty());
    assert_eq!(o.linha, "nenhuma pergunta aberta");
    assert_eq!(o.resto, ["0 agentes ativos, 0 trabalhando"]);
    assert_eq!(o.autonomia, "nível 0, Pergunta tudo");
}

#[test]
fn ativos_ordenados_quem_espera_primeiro_e_encerrados_separados() {
    let mut b = agente("b", "blocked", "running");
    b["openQuestions"] = json!(["Q1 qual base?", "Q2 qual nome?"]);
    b["questions"] = json!(3);
    let mut w = agente("w", "working", "running");
    w["reportDone"] = json!(true);
    let o = Orquestracao::de(&json!({
        "available": true, "reportsDir": ".reports",
        "agents": [agente("i", "idle", "running"), w, b, agente("d", "", "archived")],
        "consolidations": [{"branch": "demo/consolida", "members": [1, 2], "verifyOk": true,
            "verifiedAt": "2026-11-20T10:30:00Z"}],
        "decisions": [], "autonomy": {"level": 2, "label": "Decide o comum"},
    }));
    let nomes: Vec<&str> = o.ativos.iter().map(|a| a.nome.as_str()).collect();
    assert_eq!(nomes, ["b", "w", "i"]);
    assert_eq!(o.encerrados.len(), 1);
    assert_eq!(o.encerrados[0].fim, "arquivado");
    assert_eq!(o.abertas, 2);
    assert_eq!(o.linha, "2 perguntas abertas");
    assert_eq!(
        o.resto,
        [
            "3 agentes ativos, 1 trabalhando",
            "1 relatório pronto: abra em Relatório no cartão",
            "1 consolidação",
        ]
    );
    assert_eq!(
        o.perguntas(),
        [("b", "Q1 qual base?"), ("b", "Q2 qual nome?")]
    );
    assert_eq!(
        o.ativos[0].perguntas(),
        Some(("2 perguntas abertas".into(), Tom::Aviso))
    );
    assert_eq!(o.autonomia, "nível 2, Decide o comum");
}

#[test]
fn agente_monta_ramo_e_respondidas() {
    let mut a = agente("a", "idle", "running");
    a["questions"] = json!(2);
    let a = Agente::de(&a);
    assert_eq!(a.ramo, "demo/feat/DEMO-12 · #1234");
    assert_eq!(a.respondidas, 2);
    assert_eq!(a.perguntas(), Some(("2 respondidas".into(), Tom::Parado)));
    let solto = Agente::de(&json!({"name": "s", "state": "running", "live": "idle"}));
    assert_eq!(solto.ramo, "HEAD destacado");
    assert_eq!(solto.perguntas(), None);
    let fim = Agente::de(&json!({"name": "f", "state": "done"}));
    assert_eq!(fim.ramo, "");
    assert_eq!(fim.fim, "concluído");
}

#[test]
fn consolidacao_conforme_conferencia() {
    let c = Consolidacao::de(&json!({"branch": "x", "members": [7], "verifyOk": false,
        "verifiedAt": "2026-11-20T10:30:00Z"}));
    assert_eq!(c.membros, "#7");
    assert_eq!(c.selo, Some(("falta merge", Tom::Ruim)));
    assert_eq!(c.conferida, "conferida em 20/11 10:30Z");
    let nova = Consolidacao::de(&json!({"branch": "y"}));
    assert_eq!(nova.tom, Tom::Parado);
    assert_eq!(nova.conferida, "ainda não conferida");
}

#[test]
fn decisao_com_alternativas_e_meta() {
    let d = Decisao::de(&json!({"question": "Qual base?", "chosen": "develop",
        "alternatives": ["main"], "level": 2, "needed": 1, "ts": "2026-11-21T08:05:00.000Z"}));
    assert_eq!(d.escolha, "→ develop");
    assert_eq!(d.alternativas.as_deref(), Some("Alternativas: main"));
    assert_eq!(d.meta, "nível atual 2 (exige 1) · 21/11 08:05Z");
    assert_eq!(quando("curto"), "curto");
}

#[test]
fn rotulos_de_arquivo() {
    assert_eq!(rotulo_arquivo("questions"), "Perguntas");
    assert_eq!(rotulo_arquivo("outro"), "outro");
}
