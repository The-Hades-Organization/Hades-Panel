//! View-model de Branch e PRs, Notificações, Onboarding, Plugins e Herdr (`pages/code.tsx`, `notify.tsx`,
//! `onboard.tsx`, `plugins.tsx`, `herdr.tsx`): o JSON do core vira os grupos de linhas do `sistema_vm`.

use serde_json::Value;

use super::sistema_vm::{Abrir, Grupo, Linha, Lista};
use crate::vm::{Tom, plural};

fn t(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn s(v: &Value, k: &str) -> String {
    t(&v[k])
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn juntar(partes: &[String]) -> String {
    partes
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ")
}

fn linha(titulo: String, detalhe: String, selo: Option<(String, Tom)>) -> Linha {
    Linha {
        titulo,
        detalhe,
        selo,
        abre: None,
    }
}

fn grupo(titulo: &str, linhas: Vec<Linha>, grupos: &mut Vec<Grupo>) {
    if !linhas.is_empty() {
        grupos.push(Grupo {
            titulo: titulo.into(),
            linhas,
        });
    }
}

fn curto(sha: &str) -> String {
    sha.chars().take(8).collect()
}

/// `branch` da amostra (`code.tsx`): a base, as PRs incluídas, os conflitos e os documentos do rebuild.
pub fn code(b: &Value) -> Lista {
    let d = &b["develop"];
    let novos = d["newCommits"].as_u64().unwrap_or(0);
    let prs = arr(&b["prs"]);
    let conflitos = arr(&b["conflicts"]);
    let mut grupos = Vec::new();
    let base = s(b, "base");
    if !base.is_empty() || !d.is_null() {
        let selo = (novos > 0).then(|| {
            (
                plural(novos as usize, "commit novo", "commits novos"),
                Tom::Aviso,
            )
        });
        grupo(
            "Base",
            vec![linha(
                juntar(&[s(b, "baseName"), curto(&s(d, "sha"))]),
                juntar(&[s(d, "subject"), s(d, "date")]),
                selo,
            )],
            &mut grupos,
        );
    }
    let linhas = prs
        .iter()
        .map(|p| {
            linha(
                format!("#{} {}", s(p, "number"), s(p, "title")),
                juntar(&[s(p, "base"), curto(&s(p, "sha"))]),
                None,
            )
        })
        .collect();
    grupo("PRs incluídas", linhas, &mut grupos);
    let linhas = conflitos
        .iter()
        .map(|c| {
            let titulo = match c {
                Value::Object(_) => {
                    juntar(&[s(c, "pr"), s(c, "number"), s(c, "title"), s(c, "file")])
                }
                _ => t(c),
            };
            linha(titulo, String::new(), Some(("Conflito".into(), Tom::Ruim)))
        })
        .collect();
    grupo("Conflitos", linhas, &mut grupos);
    let docs = [
        ("Relatório do rebuild", "doc:rebuild.report"),
        ("Log de conflitos", "doc:conflicts.log"),
    ]
    .into_iter()
    .map(|(titulo, src)| Linha {
        abre: Some(Abrir::Log(src.into())),
        ..linha(titulo.into(), src.into(), None)
    })
    .collect();
    grupo("Documentos", docs, &mut grupos);
    let mut resumo = vec![plural(prs.len(), "PR incluída", "PRs incluídas")];
    if !conflitos.is_empty() {
        resumo.push(plural(conflitos.len(), "conflito", "conflitos"));
    }
    Lista {
        linha: juntar(&resumo),
        grupos,
    }
}

/// `falhou` de `notify.tsx`: o canal falhou quando o resultado não é ok, enviado, silêncio nem `-`.
pub fn falhou(v: &str) -> bool {
    let v = v.trim().to_lowercase();
    !["ok", "enviad", "silêncio", "silencio", "-"]
        .iter()
        .any(|p| v.starts_with(p))
}

fn liga(v: &Value) -> (String, Tom) {
    if v.as_bool().unwrap_or(false) {
        ("Ligado".into(), Tom::Ok)
    } else {
        ("Desligado".into(), Tom::Parado)
    }
}

/// `notify` (`notify.tsx`): estado, canais, fontes e o histórico.
pub fn notify(v: &Value) -> Lista {
    let mut grupos = Vec::new();
    let vigia = juntar(&[s(v, "unit"), s(v, "unitState")]);
    let quiet = if v["quietNow"].as_bool().unwrap_or(false) {
        format!("silêncio agora ({})", s(v, "quiet"))
    } else {
        s(v, "quiet")
    };
    grupo(
        "Canais",
        vec![
            linha("Notificações".into(), vigia, Some(liga(&v["enabled"]))),
            linha("Desktop".into(), String::new(), Some(liga(&v["desktop"]))),
            linha(
                "ntfy".into(),
                juntar(&[s(v, "server"), s(v, "topic")]),
                Some(liga(&v["ntfy"])),
            ),
            linha("Silêncio".into(), quiet, None),
        ],
        &mut grupos,
    );
    let fontes = v["sources"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, f)| {
                    let err = s(f, "err");
                    let selo = if err.is_empty() {
                        ("Ok".into(), Tom::Ok)
                    } else {
                        ("Erro".into(), Tom::Ruim)
                    };
                    linha(k.clone(), err, Some(selo))
                })
                .collect()
        })
        .unwrap_or_default();
    grupo("Fontes", fontes, &mut grupos);
    let historico = arr(&v["history"]);
    let linhas = historico
        .iter()
        .map(|h| {
            let canais = h["channels"].as_object();
            let ruins: Vec<String> = canais
                .map(|m| {
                    m.iter()
                        .filter(|(_, r)| falhou(&t(r)))
                        .map(|(k, _)| k.clone())
                        .collect()
                })
                .unwrap_or_default();
            let selo = if ruins.is_empty() {
                (s(h, "sev"), Tom::Ok)
            } else {
                (format!("falhou: {}", ruins.join(", ")), Tom::Ruim)
            };
            linha(
                s(h, "text"),
                juntar(&[s(h, "ts"), s(h, "event"), s(h, "obj")]),
                Some(selo),
            )
        })
        .collect();
    grupo("Últimas notificações", linhas, &mut grupos);
    let estado = if v["enabled"].as_bool().unwrap_or(false) {
        "ligadas"
    } else {
        "desligadas"
    };
    Lista {
        linha: juntar(&[
            estado.into(),
            plural(historico.len(), "no histórico", "no histórico"),
        ]),
        grupos,
    }
}

fn passo_estado(estado: &str) -> (&'static str, Tom) {
    match estado {
        "feito" => ("Feito", Tom::Ok),
        "pendente" => ("Pendente", Tom::Aviso),
        _ => ("Não se aplica", Tom::Parado),
    }
}

/// `onboard status` (`onboard.tsx`): um grupo por recurso, cada passo abre o detalhe e o comando.
pub fn onboard(v: &Value) -> Lista {
    let grupos = arr(&v["features"])
        .iter()
        .filter_map(|f| {
            let linhas: Vec<Linha> = arr(&f["steps"])
                .iter()
                .map(|p| {
                    let (rotulo, tom) = passo_estado(&s(p, "state"));
                    let detalhe = s(p, "detail");
                    let comando = s(p, "command");
                    let texto = [
                        detalhe,
                        if comando.is_empty() {
                            String::new()
                        } else {
                            format!("```\n{comando}\n```")
                        },
                    ]
                    .into_iter()
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                    Linha {
                        titulo: s(p, "name"),
                        detalhe: s(p, "note"),
                        selo: Some((rotulo.into(), tom)),
                        abre: (!texto.is_empty()).then_some(Abrir::Texto(texto)),
                    }
                })
                .collect();
            (!linhas.is_empty()).then(|| Grupo {
                titulo: juntar(&[s(f, "label"), s(f, "choice")]),
                linhas,
            })
        })
        .collect();
    let pr = &v["progress"];
    let linha = if v["complete"].as_bool().unwrap_or(false) {
        "tudo configurado".to_owned()
    } else {
        juntar(&[
            plural(
                pr["pending"].as_u64().unwrap_or(0) as usize,
                "passo pendente",
                "passos pendentes",
            ),
            plural(
                pr["unchosen"].as_u64().unwrap_or(0) as usize,
                "recurso sem escolha",
                "recursos sem escolha",
            ),
        ])
    };
    Lista { linha, grupos }
}

fn pack_estado(estado: &str) -> (&'static str, Tom) {
    match estado {
        "ativo" => ("Ativo", Tom::Ok),
        "desligado" => ("Desligado", Tom::Parado),
        _ => ("Outro projeto", Tom::Parado),
    }
}

/// `plugins list` (`plugins.tsx`): os packs com o estado; ligar e desligar ficam no web.
pub fn plugins(v: &Value) -> Lista {
    let packs = arr(&v["packs"]);
    let linhas = packs
        .iter()
        .map(|p| {
            let err = s(p, "error");
            let selo = if err.is_empty() {
                let (r, tom) = pack_estado(&s(p, "state"));
                (r.to_owned(), tom)
            } else {
                ("Erro".into(), Tom::Ruim)
            };
            let mut avisos = vec![s(p, "branch"), curto(&s(p, "commit"))];
            if let Some(n) = p["behind"].as_u64().filter(|n| *n > 0) {
                avisos.push(format!("{n} atrás"));
            }
            if p["dirty"].as_bool().unwrap_or(false) {
                avisos.push("alterações locais".into());
            }
            if p["unpublished"].as_bool().unwrap_or(false) {
                avisos.push("não publicado".into());
            }
            if !err.is_empty() {
                avisos.push(err);
            }
            linha(s(p, "name"), juntar(&avisos), Some(selo))
        })
        .collect();
    let mut grupos = Vec::new();
    grupo("Plugins", linhas, &mut grupos);
    Lista {
        linha: juntar(&[
            plural(packs.len(), "plugin", "plugins"),
            s(v, "version"),
            "ligar e desligar no painel web".into(),
        ]),
        grupos,
    }
}

fn agente_estado(estado: &str) -> (&'static str, Tom) {
    match estado {
        "working" => ("Trabalhando", Tom::Ocupado),
        "blocked" => ("Pede você", Tom::Aviso),
        "idle" | "done" => ("Parado", Tom::Parado),
        _ => ("Sem agente", Tom::Parado),
    }
}

/// `herdr overview` (`herdr.tsx`, lista): as sessões e a árvore da sessão escolhida; cada pane abre a leitura.
pub fn herdr(v: &Value) -> Result<Lista, String> {
    if v["available"] == Value::Bool(false) {
        return Err(s(v, "reason"));
    }
    let sessao = s(v, "session");
    let mut grupos = Vec::new();
    let sessoes = arr(&v["sessions"]);
    let linhas = sessoes
        .iter()
        .map(|x| {
            let selo = if x["running"].as_bool().unwrap_or(false) {
                ("Rodando".into(), Tom::Ok)
            } else {
                ("Parada".into(), Tom::Parado)
            };
            let detalhe = juntar(&[
                plural(
                    x["agents"].as_u64().unwrap_or(0) as usize,
                    "agente",
                    "agentes",
                ),
                format!("{} trabalhando", x["working"].as_u64().unwrap_or(0)),
                format!("{} pedem você", x["blocked"].as_u64().unwrap_or(0)),
            ]);
            linha(s(x, "name"), detalhe, Some(selo))
        })
        .collect();
    grupo("Sessões", linhas, &mut grupos);
    for w in arr(&v["tree"]) {
        let linhas = arr(&w["tabs"])
            .iter()
            .flat_map(|tab| arr(&tab["panes"]).iter().map(move |p| (tab, p)))
            .map(|(tab, p)| {
                let titulo = [s(p, "name"), s(p, "title"), s(p, "id")]
                    .into_iter()
                    .find(|x| !x.is_empty())
                    .unwrap_or_default();
                let (r, tom) = agente_estado(&s(p, "status"));
                Linha {
                    titulo,
                    detalhe: juntar(&[s(tab, "label"), s(p, "agent"), s(p, "cwd")]),
                    selo: Some((r.into(), tom)),
                    abre: Some(Abrir::Pane {
                        sessao: sessao.clone(),
                        id: s(p, "id"),
                    }),
                }
            })
            .collect();
        let titulo = [s(w, "label"), s(&w["worktree"], "repo"), s(w, "id")]
            .into_iter()
            .find(|x| !x.is_empty())
            .unwrap_or_default();
        grupo(&titulo, linhas, &mut grupos);
    }
    let mut resumo = vec![plural(sessoes.len(), "sessão", "sessões")];
    if !sessao.is_empty() {
        resumo.push(format!("árvore de {sessao}"));
    }
    resumo.push(s(v, "reason"));
    Ok(Lista {
        linha: juntar(&resumo),
        grupos,
    })
}

#[cfg(test)]
#[path = "ambiente_vm_tests.rs"]
mod tests;
