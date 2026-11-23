//! Telas com entidade própria (Orquestração, Revisões, Configurações e as de leitura do `sistema`): criadas ao entrar na página e descartadas
//! ao sair, com o view-model puro ao lado (`<tela>_vm.rs`).

use std::time::Duration;

use gpui::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, SharedString, Styled,
    div, px,
};
use hades_core::Env;
use hades_panel_core::PanelEnv;
use hades_ui::components::{EmptyState, MarkdownView, Sheet};
use hades_ui::{Tone, palette};
use serde_json::Value;

use crate::service::SEM_DADOS;
use crate::vm::Tom;

/// Recarga das telas com dado vivo (agentes, revisões).
pub const RECARGA: Duration = Duration::from_secs(5);

pub fn tone(t: Tom) -> Tone {
    match t {
        Tom::Ok => Tone::Ok,
        Tom::Aviso => Tone::Warn,
        Tom::Ruim => Tone::Bad,
        Tom::Parado => Tone::Neutral,
        Tom::Ocupado => Tone::Busy,
    }
}

pub fn panel_env(env: Env) -> Result<PanelEnv, String> {
    PanelEnv::from_env(env).map_err(|_| SEM_DADOS.to_owned())
}

/// A tela aberta no corpo do app.
pub enum Tela {
    Orquestracao(Entity<super::orquestracao::OrquestracaoView>),
    Revisoes(Entity<super::revisoes::RevisoesView>),
    Config(Entity<super::config::ConfigView>),
    Sistema(Entity<super::sistema::SistemaView>),
}

impl Tela {
    /// A entidade da página `id`, se ela tem tela própria.
    pub fn abrir(id: &str, env: &Env, cx: &mut App) -> Option<Self> {
        let env = env.clone();
        Some(match id {
            "orchestration" => {
                Self::Orquestracao(cx.new(|cx| super::orquestracao::OrquestracaoView::new(env, cx)))
            }
            "reviews" => Self::Revisoes(cx.new(|cx| super::revisoes::RevisoesView::new(env, cx))),
            "config" => Self::Config(cx.new(|cx| super::config::ConfigView::new(env, cx))),
            _ => {
                let l = super::sistema::Leitura::de(id)?;
                Self::Sistema(cx.new(|cx| super::sistema::SistemaView::new(env, l, cx)))
            }
        })
    }

    pub fn elemento(&self) -> AnyElement {
        match self {
            Self::Orquestracao(e) => e.clone().into_any_element(),
            Self::Revisoes(e) => e.clone().into_any_element(),
            Self::Config(e) => e.clone().into_any_element(),
            Self::Sistema(e) => e.clone().into_any_element(),
        }
    }
}

/// Chama `f` a cada `RECARGA` enquanto a entidade existir.
pub fn recarga_periodica<V: 'static>(cx: &mut Context<V>, f: fn(&mut V, &mut Context<V>)) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(RECARGA).await;
            if this.update(cx, f).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Um arquivo markdown aberto no `Sheet` lateral: relatório de revisão ou brief/relatório de agente.
#[derive(Clone, Debug, PartialEq)]
pub struct Arquivo {
    pub titulo: String,
    /// `None` enquanto lê; `Ok(None)` quando o arquivo não existe.
    pub leitura: Option<Result<(String, Option<String>), String>>,
}

impl Arquivo {
    pub fn lendo(titulo: impl Into<String>) -> Self {
        Self {
            titulo: titulo.into(),
            leitura: None,
        }
    }

    /// `{file, text}` das rotas `file` e `review_report` do core.
    pub fn de_json(r: Result<Value, String>) -> Result<(String, Option<String>), String> {
        r.map(|v| {
            let arquivo = v
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let texto = v.get("text").and_then(Value::as_str).map(str::to_owned);
            (arquivo, texto)
        })
    }
}

pub fn arquivo_sheet(
    id: &'static str,
    d: &Arquivo,
    vazio: &'static str,
    fechar: impl Fn(&mut gpui::Window, &mut App) + 'static,
    cx: &App,
) -> Sheet {
    let p = palette(cx);
    let mut s = Sheet::new(id, SharedString::from(d.titulo.clone()))
        .width(720.0)
        .on_close(fechar);
    let corpo: AnyElement = match &d.leitura {
        None => EmptyState::new("Lendo…").into_any_element(),
        Some(Err(e)) => EmptyState::new("Não foi possível ler o arquivo")
            .hint(e.clone())
            .into_any_element(),
        Some(Ok((arquivo, texto))) => {
            s = s.meta(SharedString::from(arquivo.clone()));
            match texto {
                Some(t) => MarkdownView::new(SharedString::from(t.clone())).into_any_element(),
                None => EmptyState::new("Sem arquivo")
                    .hint(vazio)
                    .into_any_element(),
            }
        }
    };
    s.child(
        div()
            .px(px(24.0))
            .py(px(20.0))
            .text_color(p.fg)
            .child(corpo),
    )
}
