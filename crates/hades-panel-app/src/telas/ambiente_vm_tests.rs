use serde_json::json;

use super::*;

#[test]
fn code_mostra_base_prs_conflitos_e_documentos() {
    let l = code(&json!({
        "baseName": "develop", "base": "origin/develop",
        "develop": {"sha": "abcdef1234", "subject": "feat: x", "newCommits": 2},
        "prs": [{"number": 7, "title": "Tela", "base": "develop", "sha": "1234567890"}],
        "conflicts": ["#8 a.ts"],
    }));
    assert_eq!(l.linha, "1 PR incluída · 1 conflito");
    let titulos: Vec<&str> = l.grupos.iter().map(|g| g.titulo.as_str()).collect();
    assert_eq!(
        titulos,
        ["Base", "PRs incluídas", "Conflitos", "Documentos"]
    );
    assert_eq!(l.grupos[0].linhas[0].titulo, "develop · abcdef12");
    assert_eq!(
        l.grupos[0].linhas[0].selo.as_ref().unwrap().0,
        "2 commits novos"
    );
    assert_eq!(l.grupos[1].linhas[0].titulo, "#7 Tela");
    assert_eq!(
        l.grupos[3].linhas[0].abre,
        Some(Abrir::Log("doc:rebuild.report".into()))
    );
}

#[test]
fn notify_marca_canal_que_falhou() {
    assert!(!falhou("enviado"));
    assert!(!falhou("Silêncio"));
    assert!(falhou("erro 500"));
    let l = notify(&json!({
        "enabled": true, "desktop": true, "ntfy": false,
        "sources": {"gate": {"err": "x"}},
        "history": [
            {"ts": "10:00", "event": "gate", "text": "falhou", "sev": "alta", "channels": {"desktop": "ok", "ntfy": "erro"}},
        ],
    }));
    assert_eq!(l.linha, "ligadas · 1 no histórico");
    let h = &l.grupos[2].linhas[0];
    assert_eq!(
        h.selo.as_ref().unwrap(),
        &("falhou: ntfy".into(), Tom::Ruim)
    );
    assert_eq!(l.grupos[1].linhas[0].selo.as_ref().unwrap().1, Tom::Ruim);
}

#[test]
fn onboard_agrupa_por_recurso_e_abre_o_comando() {
    let l = onboard(&json!({
        "features": [{"label": "Túnel", "choice": "selecionada", "steps": [
            {"name": "Token", "state": "pendente", "note": "falta", "detail": "Gere o token.", "command": "hades env tunnel"},
            {"name": "Unit", "state": "feito"},
        ]}],
        "progress": {"pending": 1, "unchosen": 0},
        "complete": false,
    }));
    assert_eq!(l.linha, "1 passo pendente · 0 recursos sem escolha");
    assert_eq!(l.grupos[0].titulo, "Túnel · selecionada");
    assert_eq!(
        l.grupos[0].linhas[0].abre,
        Some(Abrir::Texto(
            "Gere o token.\n\n```\nhades env tunnel\n```".into()
        ))
    );
    assert_eq!(l.grupos[0].linhas[1].abre, None);
    assert_eq!(
        onboard(&json!({"complete": true})).linha,
        "tudo configurado"
    );
}

#[test]
fn plugins_lista_estado_e_avisos() {
    let l = plugins(&json!({"version": "1.2", "packs": [
        {"name": "a", "state": "ativo", "branch": "main", "commit": "abcdef123", "behind": 2, "dirty": true},
        {"name": "b", "state": "desligado", "error": "sem repo"},
    ]}));
    assert_eq!(l.linha, "2 plugins · 1.2 · ligar e desligar no painel web");
    let a = &l.grupos[0].linhas[0];
    assert_eq!(a.detalhe, "main · abcdef12 · 2 atrás · alterações locais");
    assert_eq!(a.selo.as_ref().unwrap().0, "Ativo");
    assert_eq!(l.grupos[0].linhas[1].selo.as_ref().unwrap().1, Tom::Ruim);
}

#[test]
fn herdr_monta_sessoes_e_panes() {
    assert_eq!(
        herdr(&json!({"available": false, "reason": "herdr indisponível"})).unwrap_err(),
        "herdr indisponível"
    );
    let l = herdr(&json!({
        "available": true, "session": "default", "reason": null,
        "sessions": [{"name": "default", "running": true, "agents": 1, "working": 1, "blocked": 0}],
        "tree": [{"id": "w1", "label": "repo", "tabs": [{"label": "aba", "panes": [
            {"id": "p1", "name": "rs-1", "agent": "claude", "status": "working", "cwd": "/a"},
        ]}]}],
    }))
    .unwrap();
    assert_eq!(l.linha, "1 sessão · árvore de default");
    assert_eq!(l.grupos[1].titulo, "repo");
    let p = &l.grupos[1].linhas[0];
    assert_eq!(p.titulo, "rs-1");
    assert_eq!(p.detalhe, "aba · claude · /a");
    assert_eq!(
        p.abre,
        Some(Abrir::Pane {
            sessao: "default".into(),
            id: "p1".into()
        })
    );
}
