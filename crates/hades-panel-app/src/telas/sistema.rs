//! Telas de leitura Documentação, Logs, Qualidade, Execuções e Extensões (`pages/docs.tsx`, `logs.tsx`,
//! `quality.tsx`, `jobs.tsx`, `extensoes.tsx`): grupos de linhas sobre o `sistema_vm`; o documento, o log ou o
//! detalhe da extensão abre num `Sheet`. Ligar e desligar regra ou extensão e interromper execução ficam para o PR
//! das ações.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px,
};
use hades_core::Env;
use std::process::Command;

use hades_panel_core::{PanelEnv, amostra, config, docs, logs, qualidade};
use hades_ui::components::{
    Button, ButtonKind, ButtonSize, Card, EmptyState, Page, PageHeader, StatusBadge,
};
use hades_ui::palette;
use hades_ui::tokens::{FONT_MONO, size};
use serde_json::Value;

use super::sistema_vm::{self, Abrir, Lista};
use super::{Arquivo, arquivo_sheet, panel_env, recarga_periodica, tone};
use crate::vm::{Tarefa, plural};

/// Qual das telas de leitura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leitura {
    Docs,
    Logs,
    Qualidade,
    Jobs,
    Extensoes,
}

impl Leitura {
    pub fn de(id: &str) -> Option<Self> {
        Some(match id {
            "docs" => Self::Docs,
            "logs" => Self::Logs,
            "quality" => Self::Qualidade,
            "jobs" => Self::Jobs,
            "extensoes" => Self::Extensoes,
            _ => return None,
        })
    }

    fn titulo(self) -> &'static str {
        match self {
            Self::Docs => "Documentação",
            Self::Logs => "Logs",
            Self::Qualidade => "Qualidade",
            Self::Jobs => "Execuções",
            Self::Extensoes => "Extensões",
        }
    }

    fn descricao(self) -> &'static str {
        match self {
            Self::Docs => "SKILL.md, referências e changelog",
            Self::Logs => "Arquivos de log e documentos de estado do ambiente",
            Self::Qualidade => {
                "Regras de qualidade que entram nos briefs, na entrega, no inbox e na revisão"
            }
            Self::Jobs => "Ações disparadas pelo painel, com o log de cada uma",
            Self::Extensoes => "Ferramentas externas: instalada, versão, saúde",
        }
    }

    fn vazio(self) -> (&'static str, &'static str) {
        match self {
            Self::Docs => (
                "Nenhum documento",
                "A skill instalada não tem documentos para listar.",
            ),
            Self::Logs => ("Nenhum log", "Os .log do diretório de dados aparecem aqui."),
            Self::Qualidade => (
                "Nenhuma regra",
                "O catálogo de qualidade da skill está vazio.",
            ),
            Self::Jobs => (
                "Nenhuma execução ainda",
                "As ações do ciclo disparadas no painel aparecem aqui, com o log.",
            ),
            Self::Extensoes => (
                "Nenhuma extensão declarada",
                "A pasta extensions/ da skill não tem manifesto válido.",
            ),
        }
    }

    /// Só Execuções muda sozinha enquanto a tela está aberta.
    fn viva(self) -> bool {
        self == Self::Jobs
    }

    fn ler(self, e: &PanelEnv) -> Result<Lista, String> {
        Ok(match self {
            Self::Docs => sistema_vm::docs(&docs::docs_index(e)),
            Self::Logs => sistema_vm::logs(&logs::log_files_json(e), logs::DOCS_ALLOWED),
            Self::Qualidade => sistema_vm::qualidade(&qualidade::quality_state(e)),
            Self::Jobs => sistema_vm::jobs(&amostra::snapshot(e)),
            Self::Extensoes => {
                let valores = config::state_json(e)
                    .map(|c| sistema_vm::valores_config(&c))
                    .unwrap_or_default();
                sistema_vm::extensoes(&ext_list()?, &valores)
            }
        })
    }
}

/// Roda `hades ext list --json` (o mesmo JSON de `GET /api/extensions`). Bloqueia até ~5 s por extensão (versão e
/// saúde): chame no executor de fundo.
fn ext_list() -> Result<Value, String> {
    let out = Command::new("hades")
        .args(["ext", "list", "--json"])
        .output()
        .map_err(|e| format!("hades não encontrado no PATH: {e}"))?;
    if !out.status.success() {
        let erro = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(format!("hades ext list falhou: {erro}"));
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("hades ext list não devolveu JSON: {e}"))
}

/// Texto do `Sheet`: o documento em markdown; o log como bloco de código.
fn abrir_texto(e: &PanelEnv, a: &Abrir) -> Result<Value, String> {
    match a {
        Abrir::Doc(id) => docs::doc_text(e, id)
            .map(|v| serde_json::json!({ "file": id, "text": v["text"] }))
            .map_err(|e| e.message),
        Abrir::Log(src) => logs::read_log(e, src, logs::LOG_LINES_DEFAULT, None, &[])
            .map(|v| {
                let t = v["text"].as_str().unwrap_or_default();
                serde_json::json!({ "file": src, "text": format!("```\n{t}\n```") })
            })
            .map_err(|e| e.message),
        Abrir::Texto(t) => Ok(serde_json::json!({ "file": "", "text": t })),
    }
}

pub struct SistemaView {
    env: Env,
    leitura: Leitura,
    dados: Option<Result<Lista, String>>,
    tarefa: Tarefa,
    detalhe: Option<Arquivo>,
}

impl SistemaView {
    pub fn new(env: Env, leitura: Leitura, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            env,
            leitura,
            dados: None,
            tarefa: Tarefa::default(),
            detalhe: None,
        };
        v.recarregar(cx);
        if leitura.viva() {
            recarga_periodica(cx, Self::recarregar);
        }
        v
    }

    fn recarregar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.tarefa.iniciar() else {
            return;
        };
        let (env, leitura) = (self.env.clone(), self.leitura);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { panel_env(env).and_then(|e| leitura.ler(&e)) })
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

    fn abrir(&mut self, a: Abrir, titulo: String, cx: &mut Context<Self>) {
        self.detalhe = Some(Arquivo::lendo(titulo));
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { panel_env(env).and_then(|e| abrir_texto(&e, &a)) })
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

    fn grupo(&self, gi: usize, g: &sistema_vm::Grupo, cx: &mut Context<Self>) -> Card {
        let p = palette(cx);
        let linhas = g.linhas.iter().enumerate().map(|(i, l)| {
            let mut topo = div().flex().items_center().gap(px(12.0)).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(l.titulo.clone())),
            );
            if let Some((rotulo, tom)) = &l.selo {
                topo = topo.child(StatusBadge::new(
                    SharedString::from(rotulo.clone()),
                    tone(*tom),
                ));
            }
            if let Some(a) = l.abre.clone() {
                let ent = cx.entity();
                let titulo = l.titulo.clone();
                topo = topo.child(
                    Button::new(("abrir", gi * 10_000 + i), "Abrir")
                        .kind(ButtonKind::Ghost)
                        .size(ButtonSize::Xs)
                        .on_click(move |_, _, cx| {
                            let (a, titulo) = (a.clone(), titulo.clone());
                            ent.update(cx, |v, cx| v.abrir(a, titulo, cx));
                        }),
                );
            }
            let mut linha = div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(p.border)
                .child(topo);
            if !l.detalhe.is_empty() {
                linha = linha.child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(size::META))
                        .text_color(p.muted_fg)
                        .truncate()
                        .child(SharedString::from(l.detalhe.clone())),
                );
            }
            linha
        });
        Card::new()
            .title(SharedString::from(g.titulo.clone()))
            .meta(plural(g.linhas.len(), "item", "itens"))
            .child(div().flex().flex_col().children(linhas))
    }
}

impl Render for SistemaView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let l = self.leitura;
        let header = PageHeader::new(l.titulo()).description(l.descricao());
        let corpo: Vec<AnyElement> = match self.dados.clone() {
            None => vec![EmptyState::new("Lendo…").into_any_element()],
            Some(Err(e)) => vec![
                EmptyState::new("Sem dados do Hades")
                    .hint(e)
                    .into_any_element(),
            ],
            Some(Ok(lista)) => {
                let mut out = vec![
                    div()
                        .text_size(px(size::META))
                        .text_color(p.muted_fg)
                        .child(SharedString::from(lista.linha.clone()))
                        .into_any_element(),
                ];
                if lista.grupos.iter().all(|g| g.linhas.is_empty()) {
                    let (t, h) = l.vazio();
                    out.push(EmptyState::new(t).hint(h).into_any_element());
                }
                for (i, g) in lista.grupos.iter().enumerate() {
                    out.push(self.grupo(i, g, cx).into_any_element());
                }
                out
            }
        };
        let fechar = cx.entity();
        let sheet = self.detalhe.as_ref().map(|d| {
            arquivo_sheet(
                "sistema-arquivo",
                d,
                "O arquivo está vazio ou não existe.",
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
            .child(Page::new("sistema").header(header).children(corpo))
            .children(sheet)
    }
}
