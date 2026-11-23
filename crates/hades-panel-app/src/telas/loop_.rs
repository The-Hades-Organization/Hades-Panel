//! Tela Loop (`pages/loop.tsx`): escalações ao dono primeiro, depois as PRs mexidas, as ações das últimas 24 h, as
//! sessões vivas e os locks. Lê o `loop`, `agents` e `locks` da fila de gates.

use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use hades_ui::components::{Card, Dot, EmptyState, Fio, FioState, PageHeader, StatusBadge};
use hades_ui::tokens::size;
use hades_ui::{Palette, Tone, palette};
use serde_json::Value;

use super::ui::{Detalhe, colunas, linha_de_estado, texto_meta};
use super::{arr, cauda, desde_iso, dur, mb, num, plural, sim, texto};

/// `DELIVERY_STATE` de `loop.tsx:16`.
fn estado_entrega(k: &str) -> &str {
    match k {
        "in-progress" => "em andamento",
        "open" => "com PR aberta",
        _ => k,
    }
}

/// `VEREDITO_PR` de `loop.tsx:15`.
fn veredito_pr(k: &str) -> &str {
    match k {
        "red" => "Bloqueia",
        "yellow" => "Pede ajustes",
        "green" => "Aprova",
        "" => "sem revisão local",
        _ => k,
    }
}

/// `VERDICT` de `loop.tsx:8`.
pub fn veredito(k: &str) -> &str {
    match k {
        "fixed" => "Corrigido",
        "applied" => "Aplicado",
        "rebutted" => "Refutado",
        "indeterminate" => "Indeterminado",
        "escalated" => "Escalado",
        "stale-fixed" => "Já corrigido",
        "informative" => "Informativo",
        "read" => "Lido",
        _ => k,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Escalacao {
    pub id: String,
    pub pr: Option<String>,
    pub por_que: String,
    pub ha: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrLoop {
    pub pr: String,
    pub rodadas: u64,
    pub bloqueantes: u64,
    /// Bloqueantes em aberto ou o veredito da revisão local.
    pub situacao: String,
    pub meta: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sessao {
    pub pid: String,
    pub clone: String,
    pub ha: String,
    pub memoria: String,
    pub em_posse: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LockLoop {
    pub nome: String,
    pub modo: String,
    pub com_agente: bool,
    pub ha: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoopVm {
    pub disponivel: bool,
    pub principal: String,
    pub resto: Vec<String>,
    pub escalacoes: Vec<Escalacao>,
    pub prs: Vec<PrLoop>,
    /// Ações por hora, da mais antiga (-24 h) para agora.
    pub por_hora: Vec<f64>,
    pub acoes: u64,
    pub legenda: Vec<String>,
    pub sessoes: Vec<Sessao>,
    pub locks: Vec<LockLoop>,
}

/// Soma de uma hora de `actionsByHour` sem o `hoursAgo` (`hourTotal` de `loop.tsx:101`).
fn total_hora(h: &Value) -> f64 {
    h.as_object().map_or(0.0, |o| {
        o.iter()
            .filter(|(k, _)| k.as_str() != "hoursAgo")
            .filter_map(|(_, v)| num(v))
            .sum()
    })
}

impl LoopVm {
    /// `Loop()` de `loop.tsx:85`, sobre a resposta de `/api/gates` e o relógio em segundos.
    pub fn de(fila: &Value, now_s: f64) -> Self {
        if !sim(&fila["available"]) {
            return Self::default();
        }
        let l = &fila["loop"];
        let ago = |t: &Value| match num(t) {
            Some(t) if t != 0.0 => dur(Some(now_s - t)),
            _ => "—".into(),
        };
        let escalacoes: Vec<Escalacao> = arr(&l["escalations"])
            .iter()
            .map(|e| Escalacao {
                id: texto(&e["id"]),
                pr: Some(texto(&e["pr"])).filter(|p| !p.is_empty()),
                por_que: texto(&e["why"]),
                ha: desde_iso(&texto(&e["askedAt"]), now_s).map_or("—".into(), |s| dur(Some(s))),
            })
            .collect();
        let horas = arr(&l["actionsByHour"]);
        let por_hora: Vec<f64> = horas.iter().rev().map(total_hora).collect();
        let acoes = por_hora.iter().sum::<f64>() as u64;
        let legenda = l["verdicts24h"].as_object().map_or_else(Vec::new, |o| {
            o.iter()
                .map(|(k, n)| format!("{} {}", veredito(k), texto(n)))
                .collect()
        });
        let entregas: Vec<String> = l["deliveries"].as_object().map_or_else(Vec::new, |o| {
            o.iter()
                .map(|(k, n)| format!("{} {}", texto(n), estado_entrega(k)))
                .collect()
        });
        let prs = arr(&l["recentPrs"])
            .iter()
            .map(|p| {
                let rodadas = num(&p["rounds"]).unwrap_or(0.0) as u64;
                let bloqueantes = num(&p["blocking"]).unwrap_or(0.0) as u64;
                PrLoop {
                    pr: texto(&p["pr"]),
                    rodadas,
                    bloqueantes,
                    situacao: if bloqueantes > 0 {
                        plural(bloqueantes, "bloqueante", "bloqueantes")
                    } else {
                        veredito_pr(&texto(&p["review"])).to_owned()
                    },
                    meta: format!(
                        "{} · há {}",
                        plural(rodadas, "rodada", "rodadas"),
                        ago(&p["last"])
                    ),
                }
            })
            .collect();
        let sessoes: Vec<Sessao> = arr(&fila["agents"])
            .iter()
            .map(|a| {
                let posse: Vec<String> = arr(&a["gates"])
                    .iter()
                    .chain(arr(&a["locks"]))
                    .map(texto)
                    .collect();
                Sessao {
                    pid: texto(&a["pid"]),
                    clone: texto(&a["cwd"]),
                    ha: dur(num(&a["age"])),
                    memoria: mb(num(&a["rss"])),
                    em_posse: if posse.is_empty() {
                        "—".into()
                    } else {
                        posse.join(" · ")
                    },
                }
            })
            .collect();
        let locks: Vec<LockLoop> = arr(&fila["locks"])
            .iter()
            .map(|k| LockLoop {
                nome: texto(&k["name"]),
                modo: texto(&k["mode"]),
                com_agente: sim(&k["agentPresent"]),
                ha: dur(num(&k["age"])),
            })
            .collect();
        let n = escalacoes.len() as u64;
        let principal = if n > 0 {
            plural(n, "escalação esperando você", "escalações esperando você")
        } else {
            "nenhuma escalação aberta".into()
        };
        let resto = vec![
            format!("{} em 24 h", plural(acoes, "ação", "ações")),
            if entregas.is_empty() {
                "nenhuma entrega registrada".into()
            } else {
                entregas.join(" · ")
            },
            format!(
                "{}, {}",
                plural(sessoes.len() as u64, "sessão viva", "sessões vivas"),
                plural(locks.len() as u64, "lock", "locks")
            ),
        ];
        Self {
            disponivel: true,
            principal,
            resto,
            escalacoes,
            prs,
            por_hora,
            acoes,
            legenda,
            sessoes,
            locks,
        }
    }

    pub fn detalhe_pr(&self, i: usize) -> Option<Detalhe> {
        let p = self.prs.get(i)?;
        Some(
            Detalhe::new(format!("PR #{}", p.pr), p.meta.clone())
                .linha("Situação", p.situacao.clone())
                .linha("Rodadas de review", p.rodadas.to_string())
                .linha("Bloqueantes abertos", p.bloqueantes.to_string()),
        )
    }

    pub fn detalhe_sessao(&self, i: usize) -> Option<Detalhe> {
        let s = self.sessoes.get(i)?;
        Some(
            Detalhe::new(cauda(&s.clone), format!("pid {}", s.pid))
                .linha("Clone", s.clone.clone())
                .linha("Há", s.ha.clone())
                .linha("Memória", s.memoria.clone())
                .linha("Em posse", s.em_posse.clone()),
        )
    }
}

/// Onde o clique numa linha abre o detalhe.
pub type AoAbrir = std::rc::Rc<dyn Fn(Detalhe, &mut Window, &mut App)>;

pub fn tela(vm: &LoopVm, descricao: &str, abrir: AoAbrir, cx: &App) -> gpui::AnyElement {
    if !vm.disponivel {
        return EmptyState::new("Estado do loop indisponível")
            .hint("Não achei o diretório de locks (variável LOCKS do env.sh).")
            .fill()
            .into_any_element();
    }
    let p = palette(cx);
    div()
        .id("tela-loop")
        .flex_1()
        .min_w_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        .child(PageHeader::new("Loop").description(descricao.to_owned()))
        .child(linha_de_estado(&p, &vm.principal, &vm.resto))
        .child(escalacoes(&p, vm))
        .child(prs(&p, vm, abrir.clone()))
        .child(acoes(&p, vm))
        .child(sessoes(&p, vm, abrir))
        .child(locks(&p, vm))
        .into_any_element()
}

fn escalacoes(p: &Palette, vm: &LoopVm) -> Card {
    let card = Card::new().title("Escalações abertas");
    if vm.escalacoes.is_empty() {
        return card.meta("0 · tudo respondido");
    }
    card.meta("responda na página Claude, com /hades answer <id>")
        .children(vm.escalacoes.iter().map(|e| {
            let cabeca = [e.pr.as_ref().map(|p| format!("#{p}")), Some(e.id.clone())]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(p.border)
                .child(texto_meta(p, format!("{cabeca} · há {}", e.ha)))
                .child(div().text_size(px(size::BODY)).child(e.por_que.clone()))
        }))
}

fn prs(p: &Palette, vm: &LoopVm, abrir: AoAbrir) -> Card {
    let card = Card::new()
        .title("PRs com atividade recente")
        .meta("48 h · um nó por rodada de review");
    if vm.prs.is_empty() {
        return card.child(EmptyState::new("Nenhuma PR mexida nas últimas 48 h"));
    }
    card.children(vm.prs.iter().enumerate().map(|(i, pr)| {
        let abrir = abrir.clone();
        let det = vm.detalhe_pr(i);
        let estado = if pr.bloqueantes > 0 {
            FioState::Parado
        } else {
            FioState::Medido
        };
        div()
            .id(("pr", i))
            .flex()
            .items_center()
            .gap(px(16.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .on_click(move |_, w, cx| {
                if let Some(d) = det.clone() {
                    abrir(d, w, cx);
                }
            })
            .child(
                div()
                    .w(px(72.0))
                    .text_size(px(size::BODY))
                    .child(format!("#{}", pr.pr)),
            )
            .child(div().flex_1().child(Fio::new(("fio-pr", i), estado)))
            .child(if pr.bloqueantes > 0 {
                StatusBadge::new(pr.situacao.clone(), Tone::Warn).into_any_element()
            } else {
                texto_meta(p, pr.situacao.clone()).into_any_element()
            })
            .child(texto_meta(p, pr.meta.clone()))
    }))
}

fn acoes(p: &Palette, vm: &LoopVm) -> Card {
    let card = Card::new().title("Comentários de review tratados em 24 h");
    if vm.acoes == 0 {
        return card.child(EmptyState::new("Nenhuma ação nas últimas 24 h"));
    }
    card.child(
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(colunas(p, &vm.por_hora, 96.0))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(texto_meta(p, "-24 h"))
                    .child(texto_meta(p, "agora")),
            )
            .child(texto_meta(p, vm.legenda.join(" · "))),
    )
}

fn sessoes(p: &Palette, vm: &LoopVm, abrir: AoAbrir) -> Card {
    let card = Card::new().title("Sessões vivas").meta("Claude Code");
    if vm.sessoes.is_empty() {
        return card.child(EmptyState::new("Nenhuma sessão viva"));
    }
    card.children(vm.sessoes.iter().enumerate().map(|(i, s)| {
        let abrir = abrir.clone();
        let det = vm.detalhe_sessao(i);
        div()
            .id(("sessao", i))
            .flex()
            .items_center()
            .gap(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|st| st.bg(p.hover))
            .on_click(move |_, w, cx| {
                if let Some(d) = det.clone() {
                    abrir(d, w, cx);
                }
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(size::BODY))
                    .child(s.clone.clone()),
            )
            .child(texto_meta(p, s.ha.clone()))
            .child(div().text_size(px(size::BODY)).child(s.memoria.clone()))
    }))
}

fn locks(p: &Palette, vm: &LoopVm) -> Card {
    let card = Card::new()
        .title("Locks de PR e deliver")
        .meta("libere com /hades locks");
    if vm.locks.is_empty() {
        return card.child(EmptyState::new("Nenhum lock"));
    }
    card.children(vm.locks.iter().enumerate().map(|(i, l)| {
        let tom = if l.com_agente { Tone::Ok } else { Tone::Warn };
        let mut linha = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(p.border)
            .child(Dot::new(("lock", i), tom))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(size::BODY))
                    .child(l.nome.clone()),
            )
            .child(texto_meta(p, l.modo.clone()))
            .child(div().flex_1());
        if !l.com_agente {
            linha = linha.child(StatusBadge::new("provável lock preso", Tone::Warn));
        }
        linha.child(texto_meta(p, format!("há {}", l.ha)))
    }))
}

#[cfg(test)]
#[path = "loop_tests.rs"]
mod tests;
