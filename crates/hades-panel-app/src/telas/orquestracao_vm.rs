//! View-model da tela Orquestração (`kit/panel/ui/pages/orchestration.tsx`), sem gpui: agentes ativos e encerrados,
//! perguntas abertas, consolidações e decisões autônomas, montados sobre o `snapshot` de
//! `hades_panel_core::orquestracao`.

use hades_ui::components::FioState;
use serde_json::Value;

use crate::vm::{Tom, plural};

/// `ENDED` (`orchestration.tsx:12`).
const ENCERRADOS: [&str; 2] = ["done", "archived"];

/// `AGENT_LIVE` (`orchestration.tsx:9-10`): rótulo, tom e fio do estado do agente no herdr. O fio "frouxo" de quem
/// espera por você não existe no Hades-UI; vai como `Medido`.
pub fn estado_vivo(live: &str) -> (String, Tom, FioState) {
    let (r, t, f) = match live {
        "working" => ("trabalhando", Tom::Ocupado, FioState::Fiando),
        "idle" => ("ocioso", Tom::Ok, FioState::Parado),
        "blocked" => ("espera por você", Tom::Aviso, FioState::Medido),
        "unknown" => ("sem sessão", Tom::Parado, FioState::Parado),
        "fora" => ("fora do herdr", Tom::Parado, FioState::Parado),
        "" => ("—", Tom::Parado, FioState::Parado),
        outro => return (outro.to_owned(), Tom::Parado, FioState::Parado),
    };
    (r.to_owned(), t, f)
}

/// `FILE_LABEL` (`orchestration.tsx:11`).
pub fn rotulo_arquivo(arquivo: &str) -> &str {
    match arquivo {
        "brief" => "Brief",
        "questions" => "Perguntas",
        "report" => "Relatório",
        "handoff" => "Handoff",
        outro => outro,
    }
}

fn texto(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn opcional(v: &Value, k: &str) -> Option<String> {
    Some(texto(v, k)).filter(|s| !s.is_empty())
}

fn textos(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|x| match x {
                    Value::String(s) => s.clone(),
                    outro => outro.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `dd/mm HH:MMZ` de um instante ISO, como `rodadaDe` das Revisões; texto fora do formato volta como veio.
pub fn quando(iso: &str) -> String {
    if iso.len() < 16 || !iso.is_char_boundary(16) {
        return iso.to_owned();
    }
    format!("{}/{} {}Z", &iso[8..10], &iso[5..7], &iso[11..16])
}

/// Um agente, como o `AgentCard` (`orchestration.tsx:103`) e a linha de `Encerrados` (`:153`) o mostram.
#[derive(Clone, Debug, PartialEq)]
pub struct Agente {
    pub nome: String,
    pub modo: String,
    pub card: Option<String>,
    pub vivo: String,
    pub rotulo: String,
    pub tom: Tom,
    pub fio: FioState,
    pub titulo: Option<String>,
    pub worktree: String,
    /// `branch · #pr`; sem branch, "HEAD destacado" no ativo.
    pub ramo: String,
    pub abertas: Vec<String>,
    pub respondidas: i64,
    pub relatorio_pronto: bool,
    pub arquivos: Vec<String>,
    pub encerrado: bool,
    /// "arquivado" ou "concluído", só no encerrado.
    pub fim: &'static str,
}

impl Agente {
    pub fn de(a: &Value) -> Self {
        let live = texto(a, "live");
        let (rotulo, tom, fio) = estado_vivo(&live);
        let estado = texto(a, "state");
        let encerrado = ENCERRADOS.contains(&estado.as_str());
        let abertas = textos(a, "openQuestions");
        let total = a.get("questions").and_then(Value::as_i64).unwrap_or(0);
        let pr = opcional(a, "pr").map(|p| format!("#{p}"));
        let branch = opcional(a, "branch");
        let ramo_branch = match (&branch, encerrado) {
            (Some(b), _) => Some(b.clone()),
            (None, false) => Some("HEAD destacado".to_owned()),
            (None, true) => None,
        };
        let ramo = [ramo_branch, pr]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        Self {
            nome: texto(a, "name"),
            modo: texto(a, "mode"),
            card: opcional(a, "card"),
            vivo: live,
            rotulo,
            tom,
            fio,
            titulo: opcional(a, "title"),
            worktree: texto(a, "worktree"),
            ramo,
            respondidas: total - abertas.len() as i64,
            abertas,
            relatorio_pronto: a
                .get("reportDone")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            arquivos: textos(a, "files"),
            encerrado,
            fim: if estado == "archived" {
                "arquivado"
            } else {
                "concluído"
            },
        }
    }

    /// A linha de perguntas do cartão: as abertas em aviso, ou as respondidas.
    pub fn perguntas(&self) -> Option<(String, Tom)> {
        if !self.abertas.is_empty() {
            let n = self.abertas.len();
            return Some((
                plural(n, "pergunta aberta", "perguntas abertas"),
                Tom::Aviso,
            ));
        }
        (self.respondidas > 0).then(|| {
            let n = self.respondidas as usize;
            (plural(n, "respondida", "respondidas"), Tom::Parado)
        })
    }
}

/// Uma consolidação (`orchestration.tsx:236-251`).
#[derive(Clone, Debug, PartialEq)]
pub struct Consolidacao {
    pub branch: String,
    pub membros: String,
    pub tom: Tom,
    /// "PRs contidas" ou "falta merge", só depois de conferida.
    pub selo: Option<(&'static str, Tom)>,
    pub conferida: String,
}

impl Consolidacao {
    pub fn de(c: &Value) -> Self {
        let ok = c.get("verifyOk").and_then(Value::as_bool).unwrap_or(false);
        let em = opcional(c, "verifiedAt");
        let membros = textos(c, "members")
            .iter()
            .map(|m| format!("#{m}"))
            .collect::<Vec<_>>()
            .join(" ");
        let (tom, selo) = match (&em, ok) {
            (Some(_), true) => (Tom::Ok, Some(("PRs contidas", Tom::Ok))),
            (Some(_), false) => (Tom::Ruim, Some(("falta merge", Tom::Ruim))),
            (None, _) => (Tom::Parado, None),
        };
        Self {
            branch: texto(c, "branch"),
            membros,
            tom,
            selo,
            conferida: em.map_or_else(
                || "ainda não conferida".to_owned(),
                |e| format!("conferida em {}", quando(&e)),
            ),
        }
    }
}

/// Uma decisão autônoma (`orchestration.tsx:258-276`).
#[derive(Clone, Debug, PartialEq)]
pub struct Decisao {
    pub pergunta: String,
    pub escolha: String,
    pub alternativas: Option<String>,
    pub meta: String,
}

impl Decisao {
    pub fn de(d: &Value) -> Self {
        let alt = textos(d, "alternatives");
        Self {
            pergunta: texto(d, "question"),
            escolha: format!("→ {}", texto(d, "chosen")),
            alternativas: (!alt.is_empty()).then(|| format!("Alternativas: {}", alt.join(" · "))),
            meta: format!(
                "nível atual {} (exige {}) · {}",
                texto(d, "level"),
                texto(d, "needed"),
                quando(&texto(d, "ts"))
            ),
        }
    }
}

/// A página inteira (`Orchestration`, `orchestration.tsx:185`).
#[derive(Clone, Debug, PartialEq)]
pub struct Orquestracao {
    pub disponivel: bool,
    pub pasta_relatorios: String,
    /// Ativos, quem espera por você primeiro, depois quem trabalha.
    pub ativos: Vec<Agente>,
    pub encerrados: Vec<Agente>,
    pub consolidacoes: Vec<Consolidacao>,
    pub decisoes: Vec<Decisao>,
    pub autonomia: String,
    pub abertas: usize,
    pub linha: String,
    pub resto: Vec<String>,
}

impl Orquestracao {
    pub fn de(x: &Value) -> Self {
        let todos: Vec<Agente> = x
            .get("agents")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(Agente::de).collect())
            .unwrap_or_default();
        let (encerrados, mut ativos): (Vec<Agente>, Vec<Agente>) =
            todos.into_iter().partition(|a| a.encerrado);
        let ordem = |a: &Agente| match a.vivo.as_str() {
            "blocked" => 0,
            "working" => 1,
            _ => 2,
        };
        ativos.sort_by_key(ordem);
        let abertas: usize = ativos.iter().map(|a| a.abertas.len()).sum();
        let relatorios = ativos.iter().filter(|a| a.relatorio_pronto).count();
        let trabalhando = ativos.iter().filter(|a| a.vivo == "working").count();
        let lista = |k: &str| {
            x.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let consolidacoes: Vec<Consolidacao> = lista("consolidations")
            .iter()
            .map(Consolidacao::de)
            .collect();
        let decisoes = lista("decisions").iter().map(Decisao::de).collect();
        let linha = if abertas > 0 {
            plural(abertas, "pergunta aberta", "perguntas abertas")
        } else {
            "nenhuma pergunta aberta".to_owned()
        };
        let mut resto = vec![format!(
            "{}, {trabalhando} trabalhando",
            plural(ativos.len(), "agente ativo", "agentes ativos")
        )];
        if relatorios > 0 {
            resto.push(format!(
                "{}: abra em Relatório no cartão",
                plural(relatorios, "relatório pronto", "relatórios prontos")
            ));
        }
        if !consolidacoes.is_empty() {
            resto.push(plural(consolidacoes.len(), "consolidação", "consolidações"));
        }
        let aut = x.get("autonomy").cloned().unwrap_or(Value::Null);
        let nivel = aut.get("level").and_then(Value::as_u64).unwrap_or(0);
        let rotulo = opcional(&aut, "label").unwrap_or_else(|| "Pergunta tudo".to_owned());
        Self {
            disponivel: x.get("available").and_then(Value::as_bool).unwrap_or(false),
            pasta_relatorios: texto(x, "reportsDir"),
            ativos,
            encerrados,
            consolidacoes,
            decisoes,
            autonomia: format!("nível {nivel}, {rotulo}"),
            abertas,
            linha,
            resto,
        }
    }

    /// As perguntas abertas de todos os ativos, com o agente de cada uma (`Perguntas`, `orchestration.tsx:77`).
    pub fn perguntas(&self) -> Vec<(&str, &str)> {
        self.ativos
            .iter()
            .flat_map(|a| a.abertas.iter().map(move |q| (a.nome.as_str(), q.as_str())))
            .collect()
    }
}

#[cfg(test)]
#[path = "orquestracao_vm_tests.rs"]
mod tests;
