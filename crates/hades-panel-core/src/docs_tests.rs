use serde_json::json;

use super::*;
use crate::teste_vivo::{sandbox, write};

fn env_com_skill(h: &Path) -> PanelEnv {
    let env = hades_core::Env::empty()
        .with("HOME", h.to_string_lossy())
        .with("HADES_DATA", h.join("data").to_string_lossy())
        .with("HADES_WORK_ROOT", h.join("work").to_string_lossy())
        .with("HADES_SKILL_DIR", h.join("skill").to_string_lossy());
    PanelEnv::from_env(env).unwrap()
}

#[test]
fn indice_agrupa_titulos_e_outros() {
    let h = sandbox("docs-indice");
    let s = h.join("skill");
    write(&s.join("SKILL.md"), "---\nname: x\n---\n# Hades\n\ntexto");
    write(&s.join("reference/panel.md"), "sem título");
    write(&s.join("reference/zeta.md"), "# Zeta  \n");
    write(&s.join("reference/notas.txt"), "# fora");
    write(&s.join("VERSION"), "1.2.3\n");
    let v = docs_index(&env_com_skill(&h));
    assert_eq!(
        v["groups"],
        json!([
            {"title": "Visão geral", "items": [{"id": "SKILL.md", "title": "Hades"}]},
            {"title": "Operação", "items": [{"id": "reference/panel.md", "title": "panel"}]},
            {"title": "Outros", "items": [{"id": "reference/zeta.md", "title": "Zeta"}]},
        ])
    );
    assert_eq!(v["version"], json!("1.2.3"));
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["groups", "version", "path"]);
}

#[test]
fn texto_tira_front_matter_e_recusa_id_fora_do_indice() {
    let h = sandbox("docs-texto");
    write(&h.join("skill/SKILL.md"), "---\nname: x\n---\n# Hades\n");
    let env = env_com_skill(&h);
    let v = doc_text(&env, "SKILL.md").unwrap();
    assert_eq!(v["text"], json!("# Hades\n"));
    assert_eq!(v["id"], json!("SKILL.md"));
    let e = doc_text(&env, "../segredo.md").unwrap_err();
    assert_eq!(e.message, "documento inválido");
}

#[test]
fn front_matter_vazio_e_sem_fim() {
    assert_eq!(sem_front_matter("---\n\n---\nx"), "x");
    assert_eq!(sem_front_matter("---\nsem fim"), "---\nsem fim");
    assert_eq!(sem_front_matter("# a\n---\nb\n---\n"), "# a\n---\nb\n---\n");
    assert_eq!(stem("reference/loop/ops.md"), "ops");
    assert_eq!(stem("a.b.md"), "a.b");
}
