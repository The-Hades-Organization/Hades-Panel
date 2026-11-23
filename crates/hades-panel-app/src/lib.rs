//! App gpui do painel do Hades: casca com o trilho, a Visão geral, a barra de estado e a paleta Ctrl K. As outras
//! páginas entram nos PRs seguintes e, até lá, mostram o estado vazio.

pub mod service;
pub mod vm;

use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use hades_core::Env;
use hades_ui::components::{
    CommandPalette, Dot, EmptyState, Kbd, PageHeader, Rail, RailBrand, RailGroup, RailItem,
    Sparkline, StatusBar,
};
use hades_ui::icons::{Icon, IconName};
use hades_ui::tokens::size;
use hades_ui::{Palette, Tone, palette};

use crate::service::Dados;
use crate::vm::{Historico, Leitura, Pagina, Tarefa, Tom, VisaoGeral};

pub const NOME: &str = "Hades";

/// Intervalo da amostra periódica: snapshot e fila relidos no executor de fundo.
const PERIODO_MS: u64 = 15_000;

pub struct PanelView {
    env: Env,
    pagina: &'static Pagina,
    recolhida: bool,
    paleta: bool,
    foco: FocusHandle,
    dados: Option<Dados>,
    aviso: Option<String>,
    historico: Historico,
    tarefa: Tarefa,
}

fn tone(t: Tom) -> Tone {
    match t {
        Tom::Ok => Tone::Ok,
        Tom::Aviso => Tone::Warn,
        Tom::Ruim => Tone::Bad,
        Tom::Parado => Tone::Neutral,
        Tom::Ocupado => Tone::Busy,
    }
}

fn cor(p: &Palette, t: Option<Tom>) -> gpui::Hsla {
    match t {
        Some(Tom::Aviso) => p.warn,
        Some(Tom::Ruim) => p.bad,
        _ => p.fg,
    }
}

impl PanelView {
    pub fn new(env: Env, foco: FocusHandle, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            env,
            pagina: vm::por_id(vm::VISAO_GERAL).expect("Visão geral no trilho"),
            recolhida: false,
            paleta: false,
            foco,
            dados: None,
            aviso: None,
            historico: Historico::default(),
            tarefa: Tarefa::default(),
        };
        v.recarregar(cx);
        v
    }

    /// Lê snapshot e fila no executor de fundo; resultado de uma geração velha é descartado.
    fn recarregar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.tarefa.iniciar() else {
            return;
        };
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { service::carregar(env) })
                .await;
            let _ = this.update(cx, |v, cx| {
                if !v.tarefa.concluir(g) {
                    return;
                }
                match r {
                    Ok(d) => {
                        v.historico
                            .registrar(vm::ponto(d.t_ms, &d.snapshot, &d.fila));
                        v.dados = Some(d);
                        v.aviso = None;
                    }
                    Err(e) => v.aviso = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Amostra periódica enquanto a janela existir.
    pub fn iniciar(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(PERIODO_MS))
                    .await;
                if this.update(cx, |v, cx| v.recarregar(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn ir(&mut self, pagina: &'static Pagina, cx: &mut Context<Self>) {
        self.pagina = pagina;
        cx.notify();
    }

    fn rail(&self, cx: &mut Context<Self>) -> Rail {
        let p = palette(cx);
        let recolhimento = cx.entity();
        let selecao = cx.entity();
        let grupos = vm::GRUPOS.iter().map(|(titulo, paginas)| {
            RailGroup::new(
                *titulo,
                paginas
                    .iter()
                    .map(|pg| RailItem::new(pg.rotulo).icon(pg.icone)),
            )
        });
        let projeto = self
            .dados
            .as_ref()
            .and_then(|d| d.snapshot["instance"]["project"].as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("—")
            .to_owned();
        let rodape = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .text_size(px(size::META))
            .text_color(p.muted_fg)
            .child(projeto)
            .child(Kbd::new("Ctrl K"));
        Rail::new(grupos)
            .brand(RailBrand::hades())
            .theme_toggle()
            .footer(rodape)
            .collapsed(self.recolhida)
            .on_collapse(move |recolher, _, cx| {
                recolhimento.update(cx, |v, cx| {
                    v.recolhida = recolher;
                    cx.notify();
                });
            })
            .on_select(move |rotulo, _, cx| {
                if let Some(pg) = vm::por_rotulo(rotulo) {
                    selecao.update(cx, |v, cx| v.ir(pg, cx));
                }
            })
            .selected(SharedString::from(self.pagina.rotulo))
    }

    fn corpo(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if !self.pagina.pronta {
            return EmptyState::new(self.pagina.rotulo)
                .hint("Esta página chega numa próxima versão do painel.")
                .fill()
                .into_any_element();
        }
        match (&self.dados, &self.aviso) {
            (Some(d), _) => self.visao_geral(d, cx).into_any_element(),
            (None, Some(e)) => EmptyState::new("Sem dados do Hades")
                .hint(e.clone())
                .fill()
                .into_any_element(),
            (None, None) => EmptyState::new("Lendo o estado do Hades…")
                .fill()
                .into_any_element(),
        }
    }

    fn visao_geral(&self, d: &Dados, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let v = VisaoGeral::de(&d.snapshot, &d.fila);
        let mut pagina = div()
            .id("visao-geral")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .p(px(24.0))
            .child(PageHeader::new("Visão geral").description(self.pagina.descricao));
        if v.pedem > 0 {
            pagina = pagina.child(pedem_voce(&p, v.pedem));
        }
        pagina
            .child(estado(&p, &v))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .children(v.leituras.iter().map(|l| leitura(&p, l))),
            )
            .child(tendencia(&p, &self.historico))
    }

    fn paleta(&self, cx: &mut Context<Self>) -> CommandPalette {
        let fechar = cx.entity();
        let ir = cx.entity();
        CommandPalette::new("paleta", vm::itens_paleta())
            .placeholder("Ir para uma página…")
            .on_close(move |window, cx| {
                fechar.update(cx, |v, cx| {
                    v.paleta = false;
                    window.focus(&v.foco, cx);
                    cx.notify();
                });
            })
            .on_pick(move |id, _, cx| {
                if let Some(pg) = vm::destino_paleta(id) {
                    ir.update(cx, |v, cx| v.ir(pg, cx));
                }
            })
    }
}

fn pedem_voce(p: &Palette, n: u64) -> impl IntoElement {
    let texto = if n == 1 {
        "Um agente pede resposta.".to_owned()
    } else {
        format!("{n} agentes pedem resposta.")
    };
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(20.0))
        .py(px(16.0))
        .border_1()
        .border_color(p.warn)
        .bg(p.card)
        .child(Icon::new(IconName::Hand).color(p.warn))
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(size::SECTION))
                        .child(format!("Pedem você {n}")),
                )
                .child(
                    div()
                        .text_size(px(size::BODY))
                        .text_color(p.muted_fg)
                        .child(texto),
                ),
        )
}

fn estado(p: &Palette, v: &VisaoGeral) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .text_size(px(size::DISPLAY))
                .child(Dot::new("estado", tone(v.tom)).pulse(v.tom == Tom::Ocupado))
                .child(format!("{}.", v.estado)),
        )
        .child(
            div()
                .text_size(px(size::BODY))
                .text_color(p.muted_fg)
                .child(v.fatos.clone()),
        )
}

fn leitura(p: &Palette, l: &Leitura) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .min_w(px(200.0))
        .flex_1()
        .p(px(16.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .child(
            div()
                .text_size(px(size::META))
                .text_color(p.muted_fg)
                .child(l.rotulo),
        )
        .child(
            div()
                .text_size(px(size::DISPLAY))
                .text_color(cor(p, l.tom))
                .child(l.valor.clone()),
        )
        .child(
            div()
                .text_size(px(size::META))
                .text_color(p.muted_fg)
                .child(l.detalhe.clone()),
        )
}

fn tendencia(p: &Palette, h: &Historico) -> impl IntoElement {
    let caixa = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .child(div().text_size(px(size::SECTION)).child("Última hora"));
    if h.len() < 2 {
        return caixa.child(
            div()
                .text_size(px(size::BODY))
                .text_color(p.muted_fg)
                .child(
                    "Coletando a tendência: um ponto a cada 15 s enquanto o painel está aberto.",
                ),
        );
    }
    let linha = |rotulo: &'static str, valores: Vec<f64>, cor: gpui::Hsla| {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(140.0))
                    .text_size(px(size::META))
                    .text_color(p.muted_fg)
                    .child(rotulo),
            )
            .child(Sparkline::new(valores).color(cor).size(320.0, 28.0))
    };
    caixa
        .child(linha("Memória do Hades", h.serie(|x| x.memoria), p.vivo))
        .child(linha("Fila de gates", h.serie(|x| x.fila), p.info))
        .child(linha("Agentes ativos", h.serie(|x| x.agentes), p.ok))
}

impl Render for PanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let rail = self.rail(cx);
        let corpo = self.corpo(cx);
        let (esquerda, direita) = self.dados.as_ref().map_or_else(
            || vm::barra_estado(&serde_json::Value::Null),
            |d| vm::barra_estado(&d.snapshot),
        );
        let mut raiz = div()
            .id("painel")
            .track_focus(&self.foco)
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, _, cx| {
                let m = &ev.keystroke.modifiers;
                if vm::abre_paleta(&ev.keystroke.key, m.control, m.platform) {
                    v.paleta = true;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.fg)
            .child(div().flex_1().flex().min_h_0().child(rail).child(corpo))
            .child(
                StatusBar::new(esquerda, direita)
                    .version(concat!("hades-panel ", env!("CARGO_PKG_VERSION"))),
            );
        if self.paleta {
            raiz = raiz.child(self.paleta(cx));
        }
        raiz
    }
}

pub fn abrir_janela(cx: &mut App) {
    let aberta = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
        window.set_window_title(NOME);
        let view: Entity<PanelView> = cx.new(|cx| {
            PanelView::iniciar(cx);
            PanelView::new(Env::from_process(), cx.focus_handle(), cx)
        });
        let foco = view.read(cx).foco.clone();
        window.focus(&foco, cx);
        view
    });
    if aberta.is_err() {
        cx.quit();
    }
}
