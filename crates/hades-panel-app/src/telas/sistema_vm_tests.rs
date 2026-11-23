use serde_json::json;

use super::*;

#[test]
fn docs_agrupa_e_abre_pelo_id() {
    let v = json!({
        "groups": [
            { "title": "Começo", "items": [{ "id": "SKILL.md", "title": "Hades" }] },
            { "title": "Outros", "items": [{ "id": "reference/a.md", "title": "A" }, { "id": "reference/b.md", "title": "B" }] }
        ],
        "version": "6.56.0",
        "path": "/skill"
    });
    let l = docs(&v);
    assert_eq!(l.linha, "3 documentos · skill 6.56.0");
    assert_eq!(l.grupos.len(), 2);
    assert_eq!(
        l.grupos[0].linhas[0].abre,
        Some(Abrir::Doc("SKILL.md".into()))
    );
    assert_eq!(l.grupos[1].linhas[1].titulo, "B");
}

#[test]
fn logs_sem_arquivos_mostra_so_os_documentos() {
    let l = logs(&json!([]), &["STATUS.md", "notes.md"]);
    assert_eq!(l.linha, "0 arquivos de log");
    assert_eq!(l.grupos.len(), 1);
    assert_eq!(
        l.grupos[0].linhas[1].abre,
        Some(Abrir::Log("doc:notes.md".into()))
    );
    let l = logs(&json!(["app.log"]), &[]);
    assert_eq!(l.linha, "1 arquivo de log");
    assert_eq!(
        l.grupos[0].linhas[0].abre,
        Some(Abrir::Log("file:app.log".into()))
    );
}

#[test]
fn qualidade_segue_a_ordem_dos_grupos_e_marca_o_estado() {
    let v = json!({
        "rules": [
            { "id": "t1", "group": "testes", "state": "on", "effSeverity": "bloqueante", "text": "Teste por atômica" },
            { "id": "l1", "group": "linguagem", "state": "off", "effSeverity": "alta" },
            { "id": "x1", "group": "extra", "state": "on", "effSeverity": "" }
        ],
        "conflicts": [{ "id": "c" }],
        "project": "attlas",
        "groups": ["linguagem", "framework", "testes"]
    });
    let l = qualidade(&v);
    assert_eq!(
        l.linha,
        "2 de 3 regras ligadas · projeto attlas · 1 conflito"
    );
    let titulos: Vec<_> = l.grupos.iter().map(|g| g.titulo.as_str()).collect();
    assert_eq!(titulos, ["linguagem", "testes", "extra"]);
    assert_eq!(
        l.grupos[0].linhas[0].selo,
        Some(("desligada".into(), Tom::Parado))
    );
    assert_eq!(l.grupos[0].linhas[0].titulo, "l1");
    assert_eq!(
        l.grupos[1].linhas[0].selo,
        Some(("bloqueante".into(), Tom::Ruim))
    );
    assert_eq!(
        l.grupos[2].linhas[0].selo,
        Some(("ligada".into(), Tom::Parado))
    );
}

#[test]
fn jobs_resume_e_abre_o_log_do_job() {
    let snap = json!({ "jobs": [
        { "id": "j-2", "title": "Build", "cmd": "nx build\n--x", "state": "rodando" },
        { "id": "j-1", "title": "Subir", "cmd": "up", "state": "falhou", "rc": 2 }
    ]});
    let l = jobs(&snap);
    assert_eq!(l.linha, "1 rodando · 2 no histórico");
    let h = &l.grupos[0].linhas;
    assert_eq!(h[0].detalhe, "nx build");
    assert_eq!(h[0].selo, Some(("Rodando".into(), Tom::Ocupado)));
    assert_eq!(h[1].selo, Some(("Falhou · saída 2".into(), Tom::Ruim)));
    assert_eq!(h[1].abre, Some(Abrir::Log("job:j-1".into())));
    let vazio = jobs(&json!({}));
    assert_eq!(vazio.linha, "nada rodando · 0 no histórico");
    assert!(vazio.grupos.is_empty());
}

#[test]
fn extensoes_resume_e_monta_o_detalhe() {
    let v = json!({ "version": 1, "extensions": [
        { "name": "herdr", "description": "Multiplexador", "capabilities": ["canvas.terminal"], "enabled": true,
          "installed": true, "path": "/bin/herdr", "version": "0.9.5", "minVersion": "0.9.0", "problems": [],
          "install": "Instale o herdr.", "configKeys": ["HADES_HERDR_BIN"], "enableKey": "HADES_EXT_HERDR" },
        { "name": "mark", "description": "Voz", "capabilities": ["voice", "outra"], "enabled": true,
          "installed": true, "path": "/bin/mark", "version": "0.1.0", "minVersion": "0.2.0",
          "problems": [{ "code": "ext.version-below-min", "field": "version.min", "message": "versão 0.1.0 abaixo da mínima 0.2.0" }],
          "install": "Instale o Mark.", "configKeys": [], "enableKey": "HADES_EXT_MARK" },
        { "name": "cardinal", "description": "Diagramas", "capabilities": [], "enabled": false,
          "installed": false, "path": null, "version": null, "minVersion": null, "problems": [],
          "install": "Compile.", "configKeys": ["HADES_CARDINAL_BIN", "HADES_CARDINAL_WEB"], "enableKey": "HADES_EXT_CARDINAL" }
    ]});
    let valores = valores_config(&json!({ "items": [
        { "key": "HADES_HERDR_BIN", "value": "herdr" },
        { "key": "HADES_CARDINAL_WEB", "value": "" }
    ]}));
    let l = extensoes(&v, &valores);
    assert_eq!(
        l.linha,
        "2 de 3 ligadas · 1 com problema · ferramentas externas, não plugins"
    );
    let h = &l.grupos[0].linhas;
    assert_eq!(h[0].titulo, "herdr — Multiplexador");
    assert_eq!(h[0].detalhe, "versão 0.9.5 (mínima 0.9.0) · /bin/herdr");
    assert_eq!(h[0].selo, Some(("Ligada · instalada".into(), Tom::Ok)));
    assert_eq!(
        h[1].selo,
        Some(("Ligada · com problema".into(), Tom::Aviso))
    );
    assert_eq!(
        h[2].selo,
        Some(("Desligada · não instalada".into(), Tom::Parado))
    );
    assert_eq!(h[2].detalhe, "versão desconhecida (sem versão mínima)");
    let Some(Abrir::Texto(t)) = &h[0].abre else {
        panic!("herdr abre o detalhe")
    };
    assert!(t.contains("**Capacidades:** terminal no canvas"));
    assert!(t.contains("- `HADES_HERDR_BIN` = `herdr`"));
    let Some(Abrir::Texto(t)) = &h[1].abre else {
        panic!("mark abre o detalhe")
    };
    assert!(t.contains("voz · outra"));
    assert!(t.contains("- versão 0.1.0 abaixo da mínima 0.2.0 `ext.version-below-min`"));
    assert!(t.contains("Sem chaves próprias."));
    let Some(Abrir::Texto(t)) = &h[2].abre else {
        panic!("cardinal abre o detalhe")
    };
    assert!(t.contains("**Capacidades:** nenhuma"));
    assert!(t.contains("- `HADES_CARDINAL_BIN` = (sem valor)"));
    assert!(t.contains("- `HADES_CARDINAL_WEB` = (vazio)"));
}

#[test]
fn extensoes_vazia_sem_grupo() {
    let l = extensoes(&json!({ "version": 1, "extensions": [] }), &json!(null));
    assert_eq!(
        l.linha,
        "0 de 0 ligadas · ferramentas externas, não plugins"
    );
    assert!(l.grupos.is_empty());
}
