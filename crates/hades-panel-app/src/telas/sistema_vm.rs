//! View-model das telas de leitura do grupo Sistema e Ambiente (Documentação, Logs, Qualidade e Execuções): o JSON
//! das rotas `docs`, `log-files`, `quality` e do `jobs` da amostra, vira grupos de linhas que abrem um arquivo.

use serde_json::Value;

use crate::vm::{Tom, plural};

/// Uma linha clicável: `abre` é o que a tela pede ao core para mostrar no `Sheet`.
#[derive(Clone, Debug, PartialEq)]
pub struct Linha {
    pub titulo: String,
    pub detalhe: String,
    pub selo: Option<(String, Tom)>,
    pub abre: Option<Abrir>,
}

/// O que a linha abre: um documento da skill (`doc`) ou uma fonte de log (`log`).
#[derive(Clone, Debug, PartialEq)]
pub enum Abrir {
    Doc(String),
    Log(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grupo {
    pub titulo: String,
    pub linhas: Vec<Linha>,
}

/// O estado de uma tela: a linha de resumo e os grupos.
#[derive(Clone, Debug, PartialEq)]
pub struct Lista {
    pub linha: String,
    pub grupos: Vec<Grupo>,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// `docs` (`docs_index`): grupos de documentos da skill e do pack do projeto.
pub fn docs(v: &Value) -> Lista {
    let grupos: Vec<Grupo> = arr(&v["groups"])
        .iter()
        .map(|g| Grupo {
            titulo: s(g, "title"),
            linhas: arr(&g["items"])
                .iter()
                .map(|i| Linha {
                    titulo: s(i, "title"),
                    detalhe: s(i, "id"),
                    selo: None,
                    abre: Some(Abrir::Doc(s(i, "id"))),
                })
                .collect(),
        })
        .collect();
    let n = grupos.iter().map(|g| g.linhas.len()).sum();
    let versao = s(v, "version");
    let mut linha = plural(n, "documento", "documentos");
    if !versao.is_empty() {
        linha = format!("{linha} · skill {versao}");
    }
    Lista { linha, grupos }
}

/// `log-files` + os documentos de estado de `DOCS_ALLOWED`.
pub fn logs(arquivos: &Value, docs: &[&str]) -> Lista {
    let arquivos: Vec<Linha> = arr(arquivos)
        .iter()
        .filter_map(Value::as_str)
        .map(|n| Linha {
            titulo: n.to_owned(),
            detalhe: String::new(),
            selo: None,
            abre: Some(Abrir::Log(format!("file:{n}"))),
        })
        .collect();
    let documentos = docs
        .iter()
        .map(|n| Linha {
            titulo: (*n).to_owned(),
            detalhe: String::new(),
            selo: None,
            abre: Some(Abrir::Log(format!("doc:{n}"))),
        })
        .collect();
    let linha = plural(arquivos.len(), "arquivo de log", "arquivos de log");
    let mut grupos = Vec::new();
    if !arquivos.is_empty() {
        grupos.push(Grupo {
            titulo: "Arquivos".into(),
            linhas: arquivos,
        });
    }
    grupos.push(Grupo {
        titulo: "Documentos do ambiente".into(),
        linhas: documentos,
    });
    Lista { linha, grupos }
}

fn nivel_tom(nivel: &str) -> Tom {
    match nivel {
        "bloqueante" => Tom::Ruim,
        "alta" => Tom::Aviso,
        _ => Tom::Parado,
    }
}

/// `quality` (`quality_state`): as regras por grupo, na ordem de `groups`, com o estado e a severidade efetiva.
pub fn qualidade(v: &Value) -> Lista {
    let regras = arr(&v["rules"]);
    let mut ordem: Vec<String> = arr(&v["groups"])
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    for r in regras {
        let g = s(r, "group");
        if !ordem.contains(&g) {
            ordem.push(g);
        }
    }
    let ligadas = regras.iter().filter(|r| s(r, "state") == "on").count();
    let grupos = ordem
        .into_iter()
        .filter_map(|g| {
            let linhas: Vec<Linha> = regras
                .iter()
                .filter(|r| s(r, "group") == g)
                .map(|r| {
                    let on = s(r, "state") == "on";
                    let nivel = s(r, "effSeverity");
                    let selo = if on {
                        (
                            if nivel.is_empty() {
                                "ligada".into()
                            } else {
                                nivel.clone()
                            },
                            nivel_tom(&nivel),
                        )
                    } else {
                        ("desligada".into(), Tom::Parado)
                    };
                    let texto = s(r, "text");
                    Linha {
                        titulo: if texto.is_empty() { s(r, "id") } else { texto },
                        detalhe: s(r, "id"),
                        selo: Some(selo),
                        abre: None,
                    }
                })
                .collect();
            (!linhas.is_empty()).then_some(Grupo { titulo: g, linhas })
        })
        .collect();
    let mut linha = format!(
        "{ligadas} de {} ligadas",
        plural(regras.len(), "regra", "regras")
    );
    let projeto = s(v, "project");
    if !projeto.is_empty() {
        linha = format!("{linha} · projeto {projeto}");
    }
    let conflitos = arr(&v["conflicts"]).len();
    if conflitos > 0 {
        linha = format!("{linha} · {}", plural(conflitos, "conflito", "conflitos"));
    }
    Lista { linha, grupos }
}

fn job_estado(estado: &str) -> (&'static str, Tom) {
    match estado {
        "rodando" => ("Rodando", Tom::Ocupado),
        "ok" | "concluido" => ("Concluída", Tom::Ok),
        "falhou" => ("Falhou", Tom::Ruim),
        _ => ("Interrompida", Tom::Parado),
    }
}

/// `jobs` da amostra (`pages/jobs.tsx`): o histórico das execuções do painel, cada uma abrindo o seu log.
pub fn jobs(snapshot: &Value) -> Lista {
    let lista = arr(&snapshot["jobs"]);
    let rodando = lista.iter().filter(|j| s(j, "state") == "rodando").count();
    let linhas = lista
        .iter()
        .map(|j| {
            let (rotulo, tom) = job_estado(&s(j, "state"));
            let rc = j.get("rc").and_then(Value::as_i64);
            let rotulo = match (s(j, "state").as_str(), rc) {
                ("falhou", Some(rc)) => format!("{rotulo} · saída {rc}"),
                _ => rotulo.to_owned(),
            };
            Linha {
                titulo: s(j, "title"),
                detalhe: s(j, "cmd").lines().next().unwrap_or_default().to_owned(),
                selo: Some((rotulo, tom)),
                abre: Some(Abrir::Log(format!("job:{}", s(j, "id")))),
            }
        })
        .collect::<Vec<_>>();
    let mut linha = if rodando > 0 {
        format!("{rodando} rodando")
    } else {
        "nada rodando".to_owned()
    };
    linha = format!("{linha} · {} no histórico", lista.len());
    let grupos = if linhas.is_empty() {
        Vec::new()
    } else {
        vec![Grupo {
            titulo: "Histórico".into(),
            linhas,
        }]
    };
    Lista { linha, grupos }
}

#[cfg(test)]
#[path = "sistema_vm_tests.rs"]
mod tests;
