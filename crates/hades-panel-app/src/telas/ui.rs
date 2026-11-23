//! Peças de view comuns às telas: a linha de estado do topo, o detalhe em `Sheet` e a barra de colunas.

use gpui::{App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use hades_ui::components::Sheet;
use hades_ui::tokens::size;
use hades_ui::{Palette, palette};

/// O que o `Sheet` de detalhe mostra: título, meta em mono e pares rótulo · valor.
#[derive(Clone, Debug, PartialEq)]
pub struct Detalhe {
    pub titulo: String,
    pub meta: String,
    pub linhas: Vec<(String, String)>,
}

impl Detalhe {
    pub fn new(titulo: impl Into<String>, meta: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            meta: meta.into(),
            linhas: Vec::new(),
        }
    }

    /// Acrescenta a linha só quando o valor não está vazio.
    pub fn linha(mut self, rotulo: &str, valor: impl Into<String>) -> Self {
        let valor = valor.into();
        if !valor.is_empty() {
            self.linhas.push((rotulo.to_owned(), valor));
        }
        self
    }
}

pub fn detalhe_sheet(
    d: &Detalhe,
    cx: &App,
    fechar: impl Fn(&mut Window, &mut App) + 'static,
) -> Sheet {
    let p = palette(cx);
    Sheet::new("detalhe", d.titulo.clone())
        .meta(d.meta.clone())
        .on_close(fechar)
        .children(d.linhas.iter().map(|(r, v)| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_size(px(size::META))
                        .text_color(p.muted_fg)
                        .child(r.clone()),
                )
                .child(div().text_size(px(size::BODY)).child(v.clone()))
        }))
}

/// `LinhaDeEstado` do web: a frase principal grande e os fatos em linha, separados por ponto.
pub fn linha_de_estado(p: &Palette, principal: &str, resto: &[String]) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(size::DISPLAY))
                .child(format!("{principal}.")),
        )
        .child(
            div()
                .text_size(px(size::BODY))
                .text_color(p.muted_fg)
                .child(resto.join(" · ")),
        )
}

/// Barras verticais lado a lado, com a altura proporcional ao maior valor; zero vira um traço.
pub fn colunas(p: &Palette, valores: &[f64], altura: f32) -> impl IntoElement {
    let pico = valores.iter().copied().fold(1.0_f64, f64::max);
    div()
        .flex()
        .items_end()
        .gap(px(1.0))
        .h(px(altura))
        .border_b_1()
        .border_color(p.border_strong)
        .children(valores.iter().map(|&v| {
            let h = if v > 0.0 {
                (v / pico * f64::from(altura)).max(f64::from(altura) * 0.06) as f32
            } else {
                1.0
            };
            div()
                .flex_1()
                .min_w_0()
                .h(px(h))
                .bg(if v > 0.0 { p.fg } else { p.border })
        }))
}

/// Linha clicável de lista: rótulo à esquerda, meta à direita.
pub fn texto_meta(p: &Palette, t: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(size::META))
        .text_color(p.muted_fg)
        .child(t.into())
}
