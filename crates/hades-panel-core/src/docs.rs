//! Rotas `docs` e `doc` (`panel/routes-core.ts:196-261`): o índice dos textos da skill e do pacote do projeto ativo,
//! e o texto de um deles sem o front matter.

use std::path::{Path, PathBuf};

use hades_core::project::{pack_of, resolve_project};
use serde_json::{Value, json};

use crate::PanelEnv;
use crate::leitura::{PanelError, js_num, worktree};

/// `DOC_GROUPS` (`routes-core.ts:196`).
const DOC_GROUPS: &[(&str, &[&str])] = &[
    (
        "Visão geral",
        &["SKILL.md", "reference/pedidos.md", "reference/env.md"],
    ),
    (
        "Operação",
        &[
            "reference/run-hades.md",
            "reference/config.md",
            "reference/panel.md",
            "reference/onboard.md",
            "reference/branch.md",
        ],
    ),
    (
        "Ambiente",
        &["reference/architecture.md", "reference/tunnel.md"],
    ),
    (
        "Loop",
        &[
            "reference/loop/orchestrate.md",
            "reference/loop/consolidate.md",
            "reference/loop/brief-template.md",
            "reference/loop/gates.md",
            "reference/loop/collector.md",
            "reference/loop/report.md",
            "reference/loop/deliver.md",
            "reference/loop/inbox.md",
            "reference/loop/review.md",
            "reference/loop/cascade.md",
            "reference/loop/ops.md",
        ],
    ),
    ("Evolução", &["reference/evolving.md", "CHANGELOG.md"]),
];

fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// `docTitle` (`routes-core.ts:207`): a primeira linha `# `, sem o marcador.
fn doc_title(text: Option<&str>, fallback: &str) -> String {
    text.unwrap_or_default()
        .split('\n')
        .find(|l| l.starts_with("# "))
        .map_or_else(|| fallback.to_owned(), |l| l[2..].trim().to_owned())
}

/// `stem` (`routes-core.ts:209`): o nome sem a última extensão.
fn stem(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.rfind('.').map_or(base, |i| &base[..i]).to_owned()
}

fn md_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// `packDocs` (`routes-core.ts:212-225`): o pacote do projeto ativo, com `env/reference/*.md` e os `refs` do
/// `profile.json`. O TS pede a pasta ao `hades project pack`; aqui sai direto do resolvedor do `hades-core`.
fn pack_docs(env: &PanelEnv) -> Option<(PathBuf, Vec<String>)> {
    let root = pack_of(&resolve_project(env.env(), &worktree(env)))?;
    if !root.is_dir() {
        return None;
    }
    let mut rels: Vec<String> = md_names(&root.join("env/reference"))
        .into_iter()
        .map(|n| format!("env/reference/{n}"))
        .collect();
    let profile = read_text(&root.join("profile.json")).unwrap_or_else(|| "{}".into());
    if let Ok(doc) = serde_json::from_str::<Value>(&profile) {
        let refs = doc.get("refs").and_then(Value::as_object);
        for rel in refs.into_iter().flat_map(|m| m.values()) {
            let rel = crate::leitura::js_string(Some(rel));
            if !rels.contains(&rel) && root.join(&rel).is_file() {
                rels.push(rel);
            }
        }
    }
    Some((
        root,
        rels.into_iter().map(|r| format!("pack/{r}")).collect(),
    ))
}

fn item(id: String, title: String) -> Value {
    json!({ "id": id, "title": title })
}

/// `docsIndex` (`routes-core.ts:227-255`).
pub fn docs_index(env: &PanelEnv) -> Value {
    let skill = hades_core::paths::skill_root(env.env());
    let mut seen: Vec<String> = Vec::new();
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for (title, files) in DOC_GROUPS {
        let mut items = Vec::new();
        for f in *files {
            let path = skill.join(f);
            if !path.exists() {
                continue;
            }
            seen.push((*f).to_owned());
            items.push(item(
                (*f).to_owned(),
                doc_title(read_text(&path).as_deref(), &stem(f)),
            ));
        }
        groups.push(((*title).to_owned(), items));
    }
    let ref_dir = skill.join("reference");
    let extra: Vec<Value> = md_names(&ref_dir)
        .into_iter()
        .filter(|n| !seen.contains(&format!("reference/{n}")))
        .map(|n| {
            let title = doc_title(read_text(&ref_dir.join(&n)).as_deref(), &stem(&n));
            item(format!("reference/{n}"), title)
        })
        .collect();
    if !extra.is_empty() {
        groups.push(("Outros".into(), extra));
    }
    if let Some((pack, ids)) = pack_docs(env).filter(|(_, ids)| !ids.is_empty()) {
        let nome = pack
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let items = ids
            .into_iter()
            .map(|id| {
                let title = doc_title(read_text(&pack.join(&id[5..])).as_deref(), &stem(&id));
                item(id, title)
            })
            .collect();
        groups.insert(groups.len() - 1, (format!("Projeto ({nome})"), items));
    }
    let groups: Vec<Value> = groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(title, items)| json!({ "title": title, "items": items }))
        .collect();
    json!({
        "groups": groups,
        "version": read_text(&skill.join("VERSION")).unwrap_or_default().trim(),
        "path": skill.to_string_lossy(),
    })
}

/// O front matter `---` do começo (`/^---\n[\s\S]*?\n---\n/`).
fn sem_front_matter(text: &str) -> &str {
    let Some(resto) = text.strip_prefix("---\n") else {
        return text;
    };
    const FIM: &str = "\n---\n";
    resto.find(FIM).map_or(text, |i| &resto[i + FIM.len()..])
}

/// `docText` (`routes-core.ts:257-261`): só ids do índice; fora dele é `documento inválido`.
pub fn doc_text(env: &PanelEnv, id: &str) -> Result<Value, PanelError> {
    let index = docs_index(env);
    let allowed = index["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["items"].as_array().into_iter().flatten())
        .any(|i| i["id"] == id);
    if !allowed {
        return Err(PanelError::bad("documento inválido"));
    }
    let path = match id.strip_prefix("pack/") {
        Some(rel) => pack_docs(env).map(|(p, _)| p).unwrap_or_default().join(rel),
        None => hades_core::paths::skill_root(env.env()).join(id),
    };
    let text = read_text(&path).unwrap_or_default();
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or_default();
    Ok(json!({ "id": id, "text": sem_front_matter(&text), "mtime": js_num(mtime) }))
}

#[cfg(test)]
#[path = "docs_tests.rs"]
mod tests;
