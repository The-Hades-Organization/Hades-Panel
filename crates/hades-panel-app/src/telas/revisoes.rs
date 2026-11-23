//! Tela Revisões (`kit/panel/ui/pages/reviews.tsx`): em curso, loop de revisão e histórico. Lançar revisão fica
//! para o PR das ações; o relatório abre num `Sheet`.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px,
};
use hades_core::Env;
use hades_panel_core::revisoes;
use hades_panel_core::vivo::Sistema;
use hades_ui::components::{
    Button, ButtonKind, ButtonSize, Card, EmptyState, Fio, Page, PageHeader, StatusBadge,
};
use hades_ui::tokens::{FONT_MONO, size};
use hades_ui::{Palette, palette};

use super::revisoes_vm::{Loop, Resultado, Revisao, Revisoes};
use super::{Arquivo, arquivo_sheet, panel_env, recarga_periodica, tone};
use crate::vm::{Tarefa, Tom, plural};

const DESCRICAO: &str = "Revisões sobre PRs escolhidas; histórico das últimas 20";

pub struct RevisoesView {
    env: Env,
    dados: Option<Result<Revisoes, String>>,
    tarefa: Tarefa,
    detalhe: Option<Arquivo>,
}

impl RevisoesView {
    pub fn new(env: Env, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            env,
            dados: None,
            tarefa: Tarefa::default(),
            detalhe: None,
        };
        v.recarregar(cx);
        recarga_periodica(cx, Self::recarregar);
        v
    }

    fn recarregar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.tarefa.iniciar() else {
            return;
        };
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    panel_env(env).map(|e| {
                        Revisoes::de(
                            &revisoes::reviews(&e, &Sistema),
                            &revisoes::reviews_loop(&e, &Sistema),
                        )
                    })
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.tarefa.concluir(g) {
                    v.dados = Some(r);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn abrir(&mut self, id: String, titulo: String, cx: &mut Context<Self>) {
        self.detalhe = Some(Arquivo::lendo(titulo));
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    panel_env(env)
                        .and_then(|e| revisoes::review_report(&e, &id).map_err(|e| e.message))
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if let Some(d) = v.detalhe.as_mut() {
                    d.leitura = Some(Arquivo::de_json(r));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn historico(&self, r: &Revisao, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let mut topo = div().flex().items_baseline().gap(px(12.0)).child(
            div()
                .font_family(FONT_MONO)
                .child(SharedString::from(r.pr.clone())),
        );
        if let Some(t) = &r.titulo {
            topo = topo.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(t.clone())),
            );
        }
        if let Some(a) = &r.autor {
            topo = topo.child(meta(&p, a.clone(), None));
        }
        topo = topo.child(div().ml_auto().child(meta(&p, r.comecou.clone(), None)));
        let resultado = match &r.resultado {
            Resultado::Falhou(e) => div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(StatusBadge::new("Falhou", tone(Tom::Ruim)))
                .children(e.clone().map(|e| meta(&p, e, None))),
            Resultado::Interrompida => {
                div().child(StatusBadge::new("Interrompida", tone(Tom::Parado)))
            }
            Resultado::Veredito {
                rotulo,
                tom,
                contagens,
            } => div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(StatusBadge::new(*rotulo, tone(*tom)))
                .children(contagens.clone().map(|c| meta(&p, format!("· {c}"), None))),
        };
        let mut linha_meta = vec![r.como.clone(), r.publicacao.0.to_owned()];
        linha_meta.extend(r.meta.iter().cloned());
        let mut acoes = div().flex().gap(px(4.0));
        if r.relatorio {
            let ent = cx.entity();
            let (id, titulo) = (r.id.clone(), format!("Relatório de {} · {}", r.pr, r.como));
            acoes = acoes.child(
                Button::new(("relatorio", i), "Relatório")
                    .kind(ButtonKind::Ghost)
                    .size(ButtonSize::Xs)
                    .on_click(move |_, _, cx| {
                        let (id, titulo) = (id.clone(), titulo.clone());
                        ent.update(cx, |v, cx| v.abrir(id, titulo, cx));
                    }),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(p.border)
            .child(topo)
            .child(resultado)
            .child(meta(&p, linha_meta.join(" · "), r.publicacao.1))
            .child(acoes)
    }
}

fn meta(p: &Palette, s: impl Into<SharedString>, t: Option<Tom>) -> gpui::Div {
    let cor = match t {
        Some(Tom::Aviso) => p.warn,
        Some(Tom::Ruim) => p.bad,
        _ => p.muted_fg,
    };
    div()
        .text_size(px(size::META))
        .text_color(cor)
        .child(s.into())
}

fn em_curso(p: &Palette, lista: &[Revisao]) -> Card {
    let linhas = lista.iter().enumerate().map(|(i, r)| {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .child(SharedString::from(r.pr.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(
                                r.titulo.clone().unwrap_or_else(|| r.como.clone()),
                            )),
                    )
                    .child(meta(p, format!("começou {}", r.comecou), None)),
            )
            .child(Fio::new(("curso", i), r.fio))
            .child(meta(
                p,
                format!("{} · {} · {}", r.fase, r.como, r.publicacao.0),
                None,
            ))
    });
    Card::new()
        .title("Em curso")
        .meta(plural(lista.len(), "sessão", "sessões"))
        .child(div().flex().flex_col().children(linhas))
}

fn loop_card(p: &Palette, l: &Loop) -> Card {
    let rodadas = l.rodadas.iter().map(|r| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .py(px(6.0))
            .child(SharedString::from(r.titulo.clone()))
            .child(meta(p, r.resumo.clone(), None))
    });
    let mut c = Card::new()
        .title("Loop de revisão")
        .status(StatusBadge::new(
            SharedString::from(l.estado.clone()),
            tone(l.tom),
        ));
    if !l.detalhe.is_empty() {
        c = c.description(l.detalhe.clone());
    }
    c.child(div().flex().flex_col().children(rodadas))
}

impl Render for RevisoesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let header = PageHeader::new("Revisões").description(DESCRICAO);
        let corpo: Vec<AnyElement> = match self.dados.clone() {
            None => vec![EmptyState::new("Lendo as revisões…").into_any_element()],
            Some(Err(e)) => vec![
                EmptyState::new("Sem dados do Hades")
                    .hint(e)
                    .into_any_element(),
            ],
            Some(Ok(r)) => {
                let mut out = vec![meta(&p, r.linha.clone(), None).into_any_element()];
                if !r.em_curso.is_empty() {
                    out.push(em_curso(&p, &r.em_curso).into_any_element());
                }
                if let Some(l) = &r.loop_ {
                    out.push(loop_card(&p, l).into_any_element());
                }
                if r.historico.is_empty() {
                    out.push(
                        EmptyState::new("Nenhuma revisão ainda")
                            .hint("As revisões lançadas pelo painel ou pelo /hades aparecem aqui.")
                            .into_any_element(),
                    );
                } else {
                    let linhas: Vec<AnyElement> = r
                        .historico
                        .iter()
                        .enumerate()
                        .map(|(i, x)| self.historico(x, i, cx).into_any_element())
                        .collect();
                    out.push(
                        Card::new()
                            .title("Histórico")
                            .meta(plural(r.historico.len(), "revisão", "revisões"))
                            .child(div().flex().flex_col().children(linhas))
                            .into_any_element(),
                    );
                }
                out
            }
        };
        let fechar = cx.entity();
        let sheet = self.detalhe.as_ref().map(|d| {
            arquivo_sheet(
                "rev-relatorio",
                d,
                "A sessão falhou ou não terminou.",
                move |_, cx| {
                    fechar.update(cx, |v, cx| {
                        v.detalhe = None;
                        cx.notify();
                    })
                },
                cx,
            )
        });
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .child(Page::new("revisoes").header(header).children(corpo))
            .children(sheet)
    }
}
