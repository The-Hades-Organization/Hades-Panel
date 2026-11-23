//! View-model da tela Revisões (`kit/panel/ui/pages/reviews.tsx`), sem gpui: em curso, histórico e o loop de
//! revisão, sobre `reviews` e `reviews_loop` de `hades_panel_core::revisoes`.

use hades_ui::components::FioState;
use serde_json::Value;

use super::orquestracao_vm::quando;
use crate::vm::{Tom, plural};

fn texto(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn numero(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn sim(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// `usd` de `reviews.tsx:15`.
pub fn usd(v: f64) -> String {
    format!("US$ {v:.2}").replace('.', ",")
}

/// `tok` de `reviews.tsx:16`.
pub fn tok(v: f64) -> String {
    if v >= 1e6 {
        let casas = if v >= 1e7 { 0 } else { 1 };
        format!("{:.*}M", casas, v / 1e6).replace('.', ",")
    } else if v >= 1000.0 {
        format!("{}k", (v / 1000.0).round())
    } else {
        format!("{v}")
    }
}

/// `dur` do core web: segundos em "1 h 2 min", "3 min 4 s" ou "5 s".
pub fn dur(seg: f64) -> String {
    let s = seg.round() as u64;
    match (s / 3600, (s % 3600) / 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// `FASE_ROTULO` + `faseDe` (`reviews.tsx:11-12`).
pub fn fase(f: &str) -> String {
    match f {
        "" => "iniciando",
        "fila" => "na fila",
        "fetch" => "buscando a PR",
        "checkout" => "preparando o worktree",
        "revisão" => "revisando",
        "publicação" => "publicando",
        outro => return outro.to_owned(),
    }
    .to_owned()
}

/// `howLine` (`reviews.tsx:21`): comando, modelo e esforço.
pub fn como(r: &Value) -> String {
    let cmd = texto(r, "cmd");
    let cmd = match cmd.as_str() {
        "review-pr" => "/review-pr".to_owned(),
        "review-agents" => "/review-agents".to_owned(),
        _ => cmd,
    };
    let modelo = texto(r, "model");
    let mut m = modelo.chars();
    let modelo = m
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(m).collect());
    let esforco = texto(r, "effort");
    let esforco = match esforco.as_str() {
        "low" => "baixo".to_owned(),
        "medium" => "médio".to_owned(),
        "high" => "alto".to_owned(),
        _ => esforco,
    };
    format!("{cmd} · {modelo} · esforço {esforco}")
}

/// O resultado em uma linha (`Resultado`, `reviews.tsx:157`).
#[derive(Clone, Debug, PartialEq)]
pub enum Resultado {
    Falhou(Option<String>),
    Interrompida,
    Veredito {
        rotulo: &'static str,
        tom: Tom,
        contagens: Option<String>,
    },
}

impl Resultado {
    pub fn de(r: &Value) -> Self {
        match texto(r, "state").as_str() {
            "falhou" => return Self::Falhou(Some(texto(r, "error")).filter(|e| !e.is_empty())),
            "ok" => {}
            _ => return Self::Interrompida,
        }
        let (rotulo, tom) = match texto(r, "verdict").as_str() {
            "red" => ("Bloqueia", Tom::Ruim),
            "yellow" => ("Pede ajustes", Tom::Aviso),
            "green" => ("Aprova", Tom::Ok),
            _ => ("Concluída, sem veredito", Tom::Parado),
        };
        let n = |k: &str| numero(r, k).map(|n| n as usize);
        let contagens: Vec<String> = [
            n("blocking").map(|n| plural(n, "bloqueante", "bloqueantes")),
            n("important").map(|n| plural(n, "importante", "importantes")),
            n("inline").map(|n| format!("{n} in-line")),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self::Veredito {
            rotulo,
            tom,
            contagens: (!contagens.is_empty()).then(|| contagens.join(" · ")),
        }
    }
}

/// `Publicacao` (`reviews.tsx:176`): texto e, quando a publicação não se confirmou, o aviso.
pub fn publicacao(r: &Value) -> (&'static str, Option<Tom>) {
    let post = sim(r, "post");
    if sim(r, "posted") {
        ("comentada no GitHub", None)
    } else if post && texto(r, "state") == "ok" {
        ("publicação não confirmada", Some(Tom::Aviso))
    } else if post {
        ("publicação pedida", None)
    } else {
        ("só local", None)
    }
}

/// Uma revisão, em curso ou no histórico.
#[derive(Clone, Debug, PartialEq)]
pub struct Revisao {
    pub id: String,
    pub pr: String,
    pub titulo: Option<String>,
    pub autor: Option<String>,
    pub como: String,
    pub comecou: String,
    pub fase: String,
    pub fio: FioState,
    pub resultado: Resultado,
    pub publicacao: (&'static str, Option<Tom>),
    /// Custo, duração e tokens, já em texto.
    pub meta: Vec<String>,
    pub relatorio: bool,
    pub url: Option<String>,
    pub em_curso: bool,
}

impl Revisao {
    pub fn de(r: &Value) -> Self {
        let tokens = r.get("tokens").map(|t| {
            ["input", "output", "cacheRead", "cacheWrite"]
                .iter()
                .map(|k| numero(t, k).unwrap_or(0.0))
                .sum::<f64>()
        });
        let meta = [
            numero(r, "costUsd").map(usd),
            numero(r, "durationMs")
                .filter(|d| *d > 0.0)
                .map(|d| dur(d / 1000.0)),
            tokens
                .filter(|t| *t > 0.0)
                .map(|t| format!("{} tokens", tok(t))),
        ]
        .into_iter()
        .flatten()
        .collect();
        let f = texto(r, "fase");
        Self {
            id: texto(r, "id"),
            pr: format!("#{}", texto(r, "pr")),
            titulo: Some(texto(r, "title")).filter(|s| !s.is_empty()),
            autor: Some(texto(r, "author")).filter(|s| !s.is_empty()),
            como: como(r),
            comecou: quando(&texto(r, "started")),
            fio: if f == "fila" {
                FioState::Parado
            } else {
                FioState::Fiando
            },
            fase: fase(&f),
            resultado: Resultado::de(r),
            publicacao: publicacao(r),
            meta,
            relatorio: sim(r, "report"),
            url: Some(texto(r, "url")).filter(|s| !s.is_empty()),
            em_curso: texto(r, "state") == "rodando",
        }
    }
}

/// Uma rodada do loop de revisão.
#[derive(Clone, Debug, PartialEq)]
pub struct Rodada {
    pub titulo: String,
    pub resumo: String,
}

impl Rodada {
    pub fn de(r: &Value) -> Self {
        let n = |k: &str| numero(r, k).unwrap_or(0.0);
        let mut resumo = vec![
            texto(r, "outcome"),
            plural(n("reviewed") as usize, "revisada", "revisadas"),
            plural(n("skipped") as usize, "pulada", "puladas"),
        ];
        if n("costUsd") > 0.0 {
            resumo.push(usd(n("costUsd")));
        }
        Self {
            titulo: format!(
                "Rodada {} · {}",
                texto(r, "round"),
                quando(&texto(r, "startedAt"))
            ),
            resumo: resumo.join(" · "),
        }
    }
}

/// O loop de revisão (`reviews_loop`): ausente quando o repositório não tem loop.
#[derive(Clone, Debug, PartialEq)]
pub struct Loop {
    pub estado: String,
    pub tom: Tom,
    pub detalhe: String,
    pub rodadas: Vec<Rodada>,
}

impl Loop {
    pub fn de(l: &Value) -> Option<Self> {
        let st = l.get("status").filter(|s| s.is_object());
        let rodadas: Vec<Rodada> = l
            .get("rounds")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(Rodada::de).collect())
            .unwrap_or_default();
        if st.is_none() && rodadas.is_empty() {
            return None;
        }
        let ativo = sim(l, "active");
        let estado = st.map_or_else(|| "parada".to_owned(), |s| texto(s, "state"));
        let tom = match (ativo, estado.as_str()) {
            (true, "rodando") => Tom::Ocupado,
            (true, "pausada") => Tom::Aviso,
            _ => Tom::Parado,
        };
        let detalhe = st.map_or_else(String::new, |s| {
            let mut d = vec![format!("rodada {}", texto(s, "round"))];
            if let Some(n) = s.get("nextAt").and_then(Value::as_str) {
                d.push(format!("próxima {}", quando(n)));
            }
            if let Some(m) = s.get("reason").and_then(Value::as_str) {
                d.push(m.to_owned());
            }
            d.join(" · ")
        });
        Some(Self {
            estado: if ativo { estado } else { "parada".to_owned() },
            tom,
            detalhe,
            rodadas,
        })
    }
}

/// A página inteira.
#[derive(Clone, Debug, PartialEq)]
pub struct Revisoes {
    pub em_curso: Vec<Revisao>,
    pub historico: Vec<Revisao>,
    pub linha: String,
    pub loop_: Option<Loop>,
}

impl Revisoes {
    pub fn de(reviews: &Value, loop_: &Value) -> Self {
        let todas: Vec<Revisao> = reviews
            .get("items")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(Revisao::de).collect())
            .unwrap_or_default();
        let (em_curso, historico): (Vec<_>, Vec<_>) = todas.into_iter().partition(|r| r.em_curso);
        let max = numero(reviews, "maxParallel").unwrap_or(0.0) as usize;
        let linha = if em_curso.is_empty() {
            format!("nenhuma revisão em curso · até {max} em paralelo")
        } else {
            format!(
                "{} de até {max} em paralelo",
                plural(em_curso.len(), "em curso", "em curso")
            )
        };
        Self {
            em_curso,
            historico,
            linha,
            loop_: Loop::de(loop_),
        }
    }
}

#[cfg(test)]
#[path = "revisoes_vm_tests.rs"]
mod tests;
