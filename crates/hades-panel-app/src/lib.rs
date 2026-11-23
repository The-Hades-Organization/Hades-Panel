//! App gpui do painel do Hades: a casca com o trilho do Hades-UI. As páginas entram nos PRs seguintes.

use gpui::{
    App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div,
};
use hades_ui::components::{EmptyState, Rail, RailBrand, RailGroup, RailItem, StatusBar};
use hades_ui::icons::IconName;
use hades_ui::palette;

pub const NOME: &str = "Hades";

/// Única página da casca por enquanto.
const VISAO_GERAL: &str = "Visão geral";

pub struct PanelView {
    recolhida: bool,
}

impl PanelView {
    fn rail(&self, cx: &mut Context<Self>) -> Rail {
        let recolhimento = cx.entity();
        Rail::new([RailGroup::new(
            "Painel",
            [RailItem::new(VISAO_GERAL).icon(IconName::LayoutDashboard)],
        )])
        .brand(RailBrand::hades())
        .theme_toggle()
        .collapsed(self.recolhida)
        .on_collapse(move |recolher, _, cx| {
            recolhimento.update(cx, |v, cx| {
                v.recolhida = recolher;
                cx.notify();
            });
        })
        .selected(SharedString::from(VISAO_GERAL))
    }
}

impl Render for PanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let rail = self.rail(cx);
        let corpo = div().flex_1().flex().min_h_0().child(
            EmptyState::new("Painel em construção")
                .hint("As páginas chegam nas próximas versões.")
                .fill(),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.fg)
            .child(div().flex_1().flex().min_h_0().child(rail).child(corpo))
            .child(
                StatusBar::new("", "").version(concat!("hades-panel ", env!("CARGO_PKG_VERSION"))),
            )
    }
}

pub fn abrir_janela(cx: &mut App) {
    let aberta = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
        window.set_window_title(NOME);
        cx.new(|_| PanelView { recolhida: false })
    });
    if aberta.is_err() {
        cx.quit();
    }
}
