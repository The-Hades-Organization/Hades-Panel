//! Tela Consumo (`pages/usage.tsx`): custo por dia contra a linha de base, as janelas do plano, as leituras, por
//! papel, por dia, as sessões acima do teto e os agentes. Lê `usage` e `limits` do `hades-panel-core`.

use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px, relative,
};
use hades_ui::components::{AreaChart, Card, ChartSeries, EmptyState, PageHeader, Spinner};
use hades_ui::tokens::size;
use hades_ui::{Palette, palette};
use serde_json::Value;

use super::loop_::AoAbrir;
use super::ui::{Detalhe, linha_de_estado, texto_meta};
use super::{arr, cauda, dur, ktok, milhar, num, pct1, sim, texto, usd};

/// `CTX_LIMIT`, `POLL_LIMIT` e `REWRITE_LIMIT` de `usage.tsx:15`.
const CTX_LIMITE: f64 = 200_000.0;
const POLL_LIMITE: f64 = 5.0;
const REESCRITA_LIMITE: f64 = 20.0;
/// `AVISO_PCT` de `usage.tsx:73`.
const AVISO_PCT: f64 = 80.0;
/// Agentes mostrados (`slice(0, 25)` de `usage.tsx:517`).
const AGENTES: usize = 25;

/// `ROLE_LABEL` e `roleLabel` de `usage.tsx:7`.
pub fn rotulo_papel(r: &str) -> String {
    let (k, sub) = r.split_once(" · ").map_or((r, false), |(k, _)| (k, true));
    let base = match k {
        "impl" => "Implementação",
        "spec" => "Spec",
        "consolidate" => "Consolidação",
        "review" => "Revisão",
        "orchestrator" => "Orquestrador",
        "interactive" => "Interativa",
        "agente-nao-registrado" => "Agente não registrado",
        _ => k,
    };
    format!("{base}{}", if sub { " · subagentes" } else { "" })
}

/// `ddmm` de `usage.tsx:12`.
pub fn ddmm(dia: &str) -> String {
    if dia.len() < 10 {
        return dia.to_owned();
    }
    format!("{}/{}", &dia[8..10], &dia[5..7])
}

fn pct_partes(v: f64) -> String {
    format!("{v}%").replace('.', ",")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Leitura {
    pub rotulo: &'static str,
    pub valor: String,
    pub unidade: String,
    pub detalhe: String,
    /// Fração da barra (valor) e do traço (referência) sobre a escala.
    pub barra: f32,
    pub marca: Option<f32>,
    pub aviso: bool,
}

/// `Medidor` de `usage.tsx:18`: a escala é 1,15 × o maior entre valor e referência.
fn leitura(
    rotulo: &'static str,
    valor: String,
    unidade: impl Into<String>,
    detalhe: String,
    v: f64,
    marca: Option<f64>,
    aviso: bool,
) -> Leitura {
    let max = (v.max(marca.unwrap_or(0.0)) * 1.15).max(f64::MIN_POSITIVE);
    let frac = |x: f64| (x / max).clamp(0.0, 1.0) as f32;
    Leitura {
        rotulo,
        valor,
        unidade: unidade.into(),
        detalhe,
        barra: frac(v),
        marca: marca.map(frac),
        aviso,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Janela {
    pub rotulo: String,
    pub pct: f64,
    pub texto: String,
    pub aviso: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dia {
    pub rotulo: String,
    pub custo: f64,
    pub polling: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Agente {
    pub nome: String,
    pub papel: String,
    pub modelos: String,
    pub custo: String,
    pub turnos: String,
    pub contexto: String,
    pub detalhe: Detalhe,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConsumoVm {
    pub principal: String,
    pub resto: Vec<String>,
    pub nota: String,
    /// `None` quando o `/api/limits` ainda não respondeu; `Some(vazio)` é "sem amostras".
    pub janelas: Option<Vec<Janela>>,
    pub leituras: Vec<Leitura>,
    pub papeis_resumo: String,
    pub papeis: Vec<[String; 6]>,
    pub dias: Vec<Dia>,
    pub acima_teto: Vec<[String; 5]>,
    pub agentes_resumo: String,
    pub agentes: Vec<Agente>,
}

/// O que a tela mostra antes dos números.
#[derive(Clone, Debug, PartialEq)]
pub enum Estado {
    Lendo,
    Calculando,
    Pronto(Box<ConsumoVm>),
}

fn janelas(limites: &Value) -> Option<Vec<Janela>> {
    if limites.is_null() {
        return None;
    }
    if !sim(&limites["coleta"]) {
        return Some(Vec::new());
    }
    Some(
        arr(&limites["janelas"])
            .iter()
            .map(|j| {
                let pct = num(&j["pct"]).unwrap_or(0.0);
                Janela {
                    rotulo: texto(&j["rotulo"]),
                    pct,
                    texto: format!(
                        "renova {} · medido há {}",
                        texto(&j["renova"]),
                        dur(num(&j["idadeS"]))
                    ),
                    aviso: pct >= AVISO_PCT,
                }
            })
            .collect(),
    )
}

fn agente(a: &Value) -> Agente {
    let modelos = a["models"]
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let pr = texto(&a["pr"]);
    let nome = texto(&a["name"]);
    let divergencia = &a["divergence"];
    let diverge = if arr(&divergencia["models"]).is_empty() {
        String::new()
    } else {
        format!(
            "diverge de {}: +US$ {:.2}",
            texto(&divergencia["configured"]),
            num(&divergencia["extraCost"]).unwrap_or(0.0)
        )
        .replace('.', ",")
    };
    let detalhe = Detalhe::new(
        nome.clone(),
        [rotulo_papel(&texto(&a["role"])), pr.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| if s == pr { format!("#{s}") } else { s })
            .collect::<Vec<_>>()
            .join(" · "),
    )
    .linha("Modelos", modelos.clone())
    .linha("Divergência", diverge)
    .linha("Custo", usd(num(&a["cost"])))
    .linha(
        "Turnos",
        num(&a["turns"]).map_or(String::new(), |t| milhar(t as i64)),
    )
    .linha("Contexto médio", ktok(num(&a["ctxAvg"])))
    .linha("Contexto máximo", ktok(num(&a["ctx_max"])))
    .linha(
        "Teto do papel",
        if sim(&a["overCap"]) {
            format!("acima do teto de {}", ktok(num(&a["ctxCap"])))
        } else {
            String::new()
        },
    )
    .linha("Polling", usd(num(&a["poll_cost"])))
    .linha(
        "Sessão",
        if sim(&a["reused"]) {
            "reaproveitada".into()
        } else {
            String::new()
        },
    );
    Agente {
        nome: if pr.is_empty() {
            nome
        } else {
            format!("{nome} #{pr}")
        },
        papel: rotulo_papel(&texto(&a["role"])),
        modelos,
        custo: usd(num(&a["cost"])),
        turnos: num(&a["turns"]).map_or("—".into(), |t| milhar(t as i64)),
        contexto: ktok(num(&a["ctxAvg"])),
        detalhe,
    }
}

impl Estado {
    /// `Usage()` de `usage.tsx:309` sobre `/api/usage` (`{ data, age, refreshing }`) e `/api/limits`.
    pub fn de(usage: Option<&Value>, limites: &Value) -> Self {
        let Some(x) = usage else {
            return Self::Lendo;
        };
        let d = &x["data"];
        if d.is_null() {
            return Self::Calculando;
        }
        let total = &d["total"];
        let custo_total = num(&total["cost"]).unwrap_or(0.0);
        let dias_n = num(&d["days"]).unwrap_or(1.0).max(1.0);
        let por_dia = num(&d["perDay"]).unwrap_or(custo_total / dias_n);
        let base = &d["baseline"];
        let base_dia = num(&base["perDay"]).filter(|_| !base.is_null());
        let papeis = arr(&d["byRole"]);
        let agentes_papel: Vec<&Value> = papeis
            .iter()
            .filter(|r| {
                let role = texto(&r["role"]);
                let k = role.split(" · ").next().unwrap_or_default();
                !["orchestrator", "interactive"].contains(&k) && !role.contains("subagente")
            })
            .collect();
        let ag_custo: f64 = agentes_papel.iter().filter_map(|r| num(&r["cost"])).sum();
        let ag_turnos: f64 = agentes_papel.iter().filter_map(|r| num(&r["turns"])).sum();
        let ag_ctx = if ag_turnos > 0.0 {
            agentes_papel
                .iter()
                .map(|r| num(&r["ctxAvg"]).unwrap_or(0.0) * num(&r["turns"]).unwrap_or(0.0))
                .sum::<f64>()
                / ag_turnos
        } else {
            0.0
        };
        let poll = num(&total["pollCost"]).unwrap_or(0.0);
        let poll_pct = if custo_total > 0.0 {
            100.0 * poll / custo_total
        } else {
            0.0
        };
        let reescrita = num(&total["rewriteCost"]).unwrap_or(0.0);
        let refreshing = sim(&x["refreshing"]);
        let resto = vec![
            match base_dia {
                Some(b) if b > 0.0 => {
                    let delta = (100.0 * (por_dia - b) / b).round();
                    format!(
                        "{}{delta}% sobre a linha de base",
                        if delta > 0.0 { "+" } else { "" }
                    )
                }
                _ => "sem linha de base".into(),
            },
            format!("{} dias desde {}", dias_n, ddmm(&texto(&d["since"]))),
            format!(
                "{} turnos",
                milhar(num(&total["turns"]).unwrap_or(0.0) as i64)
            ),
            format!(
                "atualizado há {}{}",
                dur(Some(num(&x["age"]).unwrap_or(0.0))),
                if refreshing { ", atualizando…" } else { "" }
            ),
        ];
        let leituras = vec![
            leitura(
                "Custo por dia",
                usd(Some(por_dia)),
                "por dia (equivalente em API)",
                base_dia.map_or("sem linha de base".into(), |b| {
                    format!(
                        "linha de base {}/dia, gravada em {}",
                        usd(Some(b)),
                        ddmm(&texto(&base["savedAt"]))
                    )
                }),
                por_dia,
                base_dia,
                false,
            ),
            leitura(
                "Polling",
                format!("{}%", poll_pct.round()),
                "do custo",
                format!("{} em turnos que só esperavam", usd(Some(poll))),
                poll_pct,
                Some(POLL_LIMITE),
                poll_pct > POLL_LIMITE,
            ),
            leitura(
                "Contexto médio dos agentes",
                ktok(Some(ag_ctx)),
                "por turno",
                format!(
                    "agentes = {}% do custo · referência {}",
                    if custo_total > 0.0 {
                        (100.0 * ag_custo / custo_total).round()
                    } else {
                        0.0
                    },
                    ktok(Some(CTX_LIMITE))
                ),
                ag_ctx,
                Some(CTX_LIMITE),
                ag_ctx > CTX_LIMITE,
            ),
            leitura(
                "Reescrita depois de pausa",
                usd(Some(reescrita)),
                "",
                format!("em {dias_n} dias · contexto regravado após 1 h parado"),
                reescrita,
                Some(REESCRITA_LIMITE),
                reescrita > REESCRITA_LIMITE,
            ),
        ];
        let papeis_linhas = papeis
            .iter()
            .map(|r| {
                [
                    rotulo_papel(&texto(&r["role"])),
                    texto(&r["sessions"]),
                    usd(num(&r["cost"])),
                    pct_partes(num(&r["share"]).unwrap_or(0.0)),
                    pct_partes(num(&r["pollShare"]).unwrap_or(0.0)),
                    ktok(num(&r["ctxAvg"])),
                ]
            })
            .collect();
        let dias = arr(&d["byDay"])
            .iter()
            .map(|y| Dia {
                rotulo: ddmm(&texto(&y["day"])),
                custo: num(&y["cost"]).unwrap_or(0.0),
                polling: num(&y["pollCost"]).unwrap_or(0.0),
            })
            .collect();
        let acima_teto = arr(&d["overCap"])
            .iter()
            .map(|s| {
                let cwd = texto(&s["cwd"]);
                [
                    Some(cauda(&cwd)).filter(|c| !c.is_empty()).unwrap_or(cwd),
                    rotulo_papel(&texto(&s["role"])),
                    texto(&s["turns"]),
                    ktok(num(&s["ctxMax"])),
                    ktok(num(&s["ctxCap"])),
                ]
            })
            .collect();
        let todos = arr(&d["byAgent"]);
        Self::Pronto(Box::new(ConsumoVm {
            principal: format!(
                "{} por dia (equivalente em API, estimativa)",
                usd(Some(por_dia))
            ),
            resto,
            nota: texto(&d["note"]),
            janelas: janelas(limites),
            leituras,
            papeis_resumo: format!("{} em {dias_n} dias", usd(Some(custo_total))),
            papeis: papeis_linhas,
            dias,
            acima_teto,
            agentes_resumo: if todos.len() > AGENTES {
                format!("{AGENTES} mais caros de {}", todos.len())
            } else {
                "por agente do herdr".into()
            },
            agentes: todos.iter().take(AGENTES).map(agente).collect(),
        }))
    }
}

pub fn tela(e: &Estado, descricao: &str, abrir: AoAbrir, cx: &App) -> gpui::AnyElement {
    let vm = match e {
        Estado::Lendo => {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new("consumo-lendo"))
                .into_any_element();
        }
        Estado::Calculando => {
            return EmptyState::new("Calculando o consumo…")
                .hint("Leva alguns segundos.")
                .fill()
                .into_any_element();
        }
        Estado::Pronto(vm) => vm,
    };
    let p = palette(cx);
    let mut pagina = div()
        .id("tela-consumo")
        .flex_1()
        .min_w_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        .child(PageHeader::new("Consumo").description(descricao.to_owned()))
        .child(linha_de_estado(&p, &vm.principal, &vm.resto));
    if !vm.nota.is_empty() {
        pagina = pagina.child(texto_meta(&p, vm.nota.clone()));
    }
    if let Some(j) = &vm.janelas {
        pagina = pagina.child(janelas_el(&p, j));
    }
    pagina = pagina
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(12.0))
                .children(vm.leituras.iter().map(|l| leitura_el(&p, l))),
        )
        .child(papeis_el(&p, vm))
        .child(dias_el(&p, &vm.dias));
    if !vm.acima_teto.is_empty() {
        pagina = pagina.child(
            Card::new()
                .title("Sessões acima do teto")
                .meta(format!(
                    "contexto máximo contra o teto do papel · {}",
                    vm.acima_teto.len()
                ))
                .child(tabela(
                    &p,
                    &["Sessão", "Papel", "Turnos", "Máximo", "Teto"],
                    vm.acima_teto.iter().map(|r| r.to_vec()),
                )),
        );
    }
    pagina.child(agentes_el(&p, vm, abrir)).into_any_element()
}

fn janelas_el(p: &Palette, j: &[Janela]) -> Card {
    let card = Card::new().title("Janelas do plano");
    if j.is_empty() {
        return card.meta("sem amostras").child(texto_meta(
            p,
            "Sem medida do % das janelas de 5 h e semanal. Para medir, ponha `hades usage sample` no statusLine do Claude Code.",
        ));
    }
    card.meta("% usado, medido pelo Claude Code").child(
        div()
            .flex()
            .flex_wrap()
            .gap(px(20.0))
            .children(j.iter().map(|j| {
                let cor = if j.aviso { p.warn } else { p.fg };
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(div().text_size(px(size::LABEL)).child(j.rotulo.clone()))
                    .child(
                        div()
                            .text_size(px(size::DISPLAY))
                            .text_color(cor)
                            .child(pct1(j.pct)),
                    )
                    .child(
                        div().h(px(4.0)).bg(p.border).child(
                            div()
                                .h_full()
                                .w(relative((j.pct / 100.0).clamp(0.0, 1.0) as f32))
                                .bg(cor),
                        ),
                    )
                    .child(texto_meta(p, j.texto.clone()))
            })),
    )
}

fn leitura_el(p: &Palette, l: &Leitura) -> impl IntoElement {
    let mut barra = div()
        .relative()
        .h(px(12.0))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(4.0))
                .h(px(4.0))
                .bg(p.border),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(4.0))
                .h(px(4.0))
                .w(relative(l.barra))
                .bg(if l.aviso { p.warn } else { p.fg }),
        );
    if let Some(m) = l.marca {
        barra = barra.child(
            div()
                .absolute()
                .top_0()
                .h(px(12.0))
                .w(px(1.0))
                .left(relative(m))
                .bg(p.fg),
        );
    }
    div()
        .flex_1()
        .min_w(px(220.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(16.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .child(texto_meta(p, l.rotulo))
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(size::DISPLAY))
                        .text_color(if l.aviso { p.warn } else { p.fg })
                        .child(l.valor.clone()),
                )
                .child(texto_meta(p, l.unidade.clone())),
        )
        .child(barra)
        .child(texto_meta(p, l.detalhe.clone()))
}

fn tabela(
    p: &Palette,
    colunas: &[&str],
    linhas: impl Iterator<Item = Vec<String>>,
) -> impl IntoElement {
    let celula = |t: String| {
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(px(size::BODY))
            .child(t)
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .pb(px(6.0))
                .border_b_1()
                .border_color(p.border_strong)
                .children(
                    colunas
                        .iter()
                        .map(|c| texto_meta(p, (*c).to_owned()).flex_1()),
                ),
        )
        .children(linhas.map(|l| {
            div()
                .flex()
                .gap(px(12.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(p.border)
                .children(l.into_iter().map(celula))
        }))
}

fn papeis_el(p: &Palette, vm: &ConsumoVm) -> Card {
    Card::new()
        .title("Por papel")
        .meta(vm.papeis_resumo.clone())
        .child(tabela(
            p,
            &[
                "Papel",
                "Sessões",
                "Custo (US$)",
                "Parte do custo",
                "Polling (% do custo)",
                "Contexto médio",
            ],
            vm.papeis.iter().map(|r| r.to_vec()),
        ))
}

fn dias_el(p: &Palette, dias: &[Dia]) -> Card {
    let card = Card::new().title("Por dia").meta("US$ equivalente em API");
    if dias.is_empty() {
        return card.child(EmptyState::new("Sem dias na janela"));
    }
    let rotulos: Vec<SharedString> = dias.iter().map(|d| d.rotulo.clone().into()).collect();
    card.child(
        AreaChart::new("consumo-dia", rotulos)
            .series(ChartSeries::new(
                "Custo",
                dias.iter().map(|d| d.custo),
                p.metal_ink,
            ))
            .series(ChartSeries::new(
                "Polling",
                dias.iter().map(|d| d.polling),
                p.warn,
            ))
            .y_fmt(|v| usd(Some(v)))
            .height(200.0),
    )
}

fn agentes_el(p: &Palette, vm: &ConsumoVm, abrir: AoAbrir) -> Card {
    let card = Card::new().title("Agentes").meta(vm.agentes_resumo.clone());
    if vm.agentes.is_empty() {
        return card.child(EmptyState::new("Nenhum agente na janela"));
    }
    card.child(
        div()
            .flex()
            .gap(px(12.0))
            .pb(px(6.0))
            .border_b_1()
            .border_color(p.border_strong)
            .children(
                [
                    "Agente",
                    "Papel",
                    "Modelos",
                    "Custo (US$)",
                    "Turnos",
                    "Contexto médio",
                ]
                .map(|c| texto_meta(p, c).flex_1()),
            ),
    )
    .children(vm.agentes.iter().enumerate().map(|(i, a)| {
        let abrir = abrir.clone();
        let det = a.detalhe.clone();
        div()
            .id(("agente", i))
            .flex()
            .gap(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|s| s.bg(p.hover))
            .on_click(move |_, w, cx| abrir(det.clone(), w, cx))
            .children(
                [
                    &a.nome,
                    &a.papel,
                    &a.modelos,
                    &a.custo,
                    &a.turnos,
                    &a.contexto,
                ]
                .map(|t| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(size::BODY))
                        .child(t.clone())
                }),
            )
    }))
}

#[cfg(test)]
#[path = "consumo_tests.rs"]
mod tests;
