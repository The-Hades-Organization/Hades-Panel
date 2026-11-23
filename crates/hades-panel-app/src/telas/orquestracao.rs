//! Tela Orquestração (`kit/panel/ui/pages/orchestration.tsx`): agentes do orquestrador, perguntas ao dono,
//! consolidações e decisões autônomas. Só leitura nesta fase; brief, perguntas e relatório abrem num `Sheet`.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px,
};
use hades_core::Env;
use hades_panel_core::orquestracao;
use hades_panel_core::vivo::Sistema;
use hades_ui::components::{
    Button, ButtonKind, ButtonSize, Card, EmptyState, Fio, MetaList, Page, PageHeader, StatusBadge,
};
use hades_ui::tokens::size;
use hades_ui::{Palette, palette};

use super::orquestracao_vm::{Agente, Orquestracao, rotulo_arquivo};
use super::{Arquivo, arquivo_sheet, panel_env, recarga_periodica, tone};
use crate::vm::{Tarefa, Tom};

const DESCRICAO: &str =
    "Agentes do orquestrador, perguntas ao dono, relatórios e PRs de consolidação";

pub struct OrquestracaoView {
    env: Env,
    dados: Option<Result<Orquestracao, String>>,
    tarefa: Tarefa,
    detalhe: Option<Arquivo>,
}

impl OrquestracaoView {
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
                    panel_env(env).map(|e| Orquestracao::de(&orquestracao::snapshot(&e, &Sistema)))
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

    fn abrir(&mut self, nome: String, arquivo: String, cx: &mut Context<Self>) {
        self.detalhe = Some(Arquivo::lendo(format!(
            "{} · {nome}",
            rotulo_arquivo(&arquivo)
        )));
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    panel_env(env).and_then(|e| {
                        orquestracao::file(&e, &nome, &arquivo).map_err(|e| e.message)
                    })
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

    fn agente(&self, a: &Agente, i: usize, cx: &mut Context<Self>) -> Card {
        let p = palette(cx);
        let mut meta = MetaList::new().mono("Worktree", a.worktree.clone());
        if !a.ramo.is_empty() {
            meta = meta.mono("Branch", a.ramo.clone());
        }
        let mut card = Card::new()
            .object()
            .title(a.nome.clone())
            .meta(
                [a.modo.as_str(), a.card.as_deref().unwrap_or("")]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" · "),
            )
            .status(StatusBadge::new(a.rotulo.clone(), tone(a.tom)));
        if let Some(t) = &a.titulo {
            card = card.description(t.clone());
        }
        let mut corpo = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(Fio::new(("fio", i), a.fio))
            .child(meta);
        if let Some((linha, t)) = a.perguntas() {
            corpo = corpo.child(texto(&p, linha, Some(t)));
        }
        let botoes = a.arquivos.iter().enumerate().map(|(j, arq)| {
            let ent = cx.entity();
            let (nome, arquivo) = (a.nome.clone(), arq.clone());
            Button::new(("arq", i * 16 + j), rotulo_arquivo(arq).to_owned())
                .kind(ButtonKind::Ghost)
                .size(ButtonSize::Xs)
                .on_click(move |_, _, cx| {
                    let (nome, arquivo) = (nome.clone(), arquivo.clone());
                    ent.update(cx, |v, cx| v.abrir(nome, arquivo, cx));
                })
        });
        card.child(corpo.child(div().flex().flex_wrap().gap(px(4.0)).children(botoes)))
    }

    fn pagina(&self, o: &Orquestracao, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = palette(cx);
        let mut out: Vec<AnyElement> = Vec::new();
        let tom_linha = (o.abertas > 0).then_some(Tom::Aviso);
        out.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(texto(&p, o.linha.clone(), tom_linha))
                .child(texto(
                    &p,
                    format!("{} · autonomia {}", o.resto.join(" · "), o.autonomia),
                    Some(Tom::Parado),
                ))
                .into_any_element(),
        );
        let perguntas = o.perguntas();
        if !perguntas.is_empty() {
            let linhas = perguntas.iter().map(|(a, q)| {
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_color(p.muted_fg)
                            .child(SharedString::from(a.to_string())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(SharedString::from(q.to_string())),
                    )
            });
            out.push(
                Card::new()
                    .title("Perguntas ao dono")
                    .meta(crate::vm::plural(perguntas.len(), "aberta", "abertas"))
                    .child(div().flex().flex_col().gap(px(8.0)).children(linhas))
                    .into_any_element(),
            );
        }
        if o.ativos.is_empty() {
            out.push(EmptyState::new("Nenhum agente ativo").into_any_element());
        } else {
            let cards: Vec<Card> = o
                .ativos
                .iter()
                .enumerate()
                .map(|(i, a)| self.agente(a, i, cx))
                .collect();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(cards)
                    .into_any_element(),
            );
        }
        if !o.consolidacoes.is_empty() {
            let linhas = o.consolidacoes.iter().map(|c| {
                let mut l = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .font_family(hades_ui::tokens::FONT_MONO)
                            .child(SharedString::from(c.branch.clone())),
                    )
                    .child(texto(&p, c.membros.clone(), Some(Tom::Parado)));
                if let Some((s, t)) = c.selo {
                    l = l.child(StatusBadge::new(s, tone(t)));
                }
                l.child(texto(&p, c.conferida.clone(), Some(Tom::Parado)))
            });
            out.push(
                Card::new()
                    .title("Consolidações")
                    .child(div().flex().flex_col().gap(px(8.0)).children(linhas))
                    .into_any_element(),
            );
        }
        if !o.decisoes.is_empty() {
            let linhas = o.decisoes.iter().map(|d| {
                let mut l = div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(SharedString::from(d.pergunta.clone()))
                    .child(SharedString::from(d.escolha.clone()));
                if let Some(a) = &d.alternativas {
                    l = l.child(texto(&p, a.clone(), Some(Tom::Parado)));
                }
                l.child(texto(&p, d.meta.clone(), Some(Tom::Parado)))
            });
            out.push(
                Card::new()
                    .title("Decisões autônomas")
                    .child(div().flex().flex_col().gap(px(12.0)).children(linhas))
                    .into_any_element(),
            );
        }
        if !o.encerrados.is_empty() {
            let linhas = o.encerrados.iter().map(|a| {
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(SharedString::from(a.nome.clone()))
                    .child(texto(
                        &p,
                        [a.ramo.as_str(), a.fim]
                            .iter()
                            .filter(|s| !s.is_empty())
                            .copied()
                            .collect::<Vec<_>>()
                            .join(" · "),
                        Some(Tom::Parado),
                    ))
            });
            out.push(
                Card::new()
                    .title("Encerrados")
                    .meta(crate::vm::plural(o.encerrados.len(), "agente", "agentes"))
                    .child(div().flex().flex_col().gap(px(6.0)).children(linhas))
                    .into_any_element(),
            );
        }
        out
    }
}

fn texto(p: &Palette, s: impl Into<SharedString>, t: Option<Tom>) -> gpui::Div {
    let cor = match t {
        Some(Tom::Aviso) => p.warn,
        Some(Tom::Ruim) => p.bad,
        Some(Tom::Parado) => p.muted_fg,
        _ => p.fg,
    };
    div()
        .text_size(px(size::META))
        .text_color(cor)
        .child(s.into())
}

impl Render for OrquestracaoView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = PageHeader::new("Orquestração").description(DESCRICAO);
        let corpo: Vec<AnyElement> = match &self.dados {
            None => vec![EmptyState::new("Lendo a orquestração…").into_any_element()],
            Some(Err(e)) => vec![EmptyState::new("Sem dados do Hades").hint(e.clone()).into_any_element()],
            Some(Ok(o)) if !o.disponivel => vec![
                EmptyState::new("Nenhuma orquestração neste projeto")
                    .hint("O orquestrador grava os agentes em LOCKS/orchestration; nada foi registrado ainda.")
                    .into_any_element(),
            ],
            Some(Ok(o)) => {
                let o = o.clone();
                self.pagina(&o, cx)
            }
        };
        let fechar = cx.entity();
        let sheet = self.detalhe.as_ref().map(|d| {
            arquivo_sheet(
                "orq-arquivo",
                d,
                "O agente ainda não escreveu este arquivo.",
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
            .child(Page::new("orquestracao").header(header).children(corpo))
            .children(sheet)
    }
}
