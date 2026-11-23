//! Ferramentas embutidas: o registro das linkadas neste binário (uma feature do Cargo por ferramenta, plano F5 painel
//! decisão 8) e a moldura que desenha a tela escolhida no trilho do painel pela `render_tela` da view da ferramenta.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{AnyElement, App, AppContext, Context, Entity, IntoElement, Render, Window};
use hades_core::Env;
use hades_ui::{Embutida, FerramentaEmbutida, TelaInfo};

use crate::embutidas_vm::Ligada;

/// A view criada dentro do painel: as Configurações vêm do painel.
pub const OPCOES: Embutida = Embutida { integrado: true };

/// Uma view de ferramenta aberta, sem o tipo dela.
pub trait Aberta {
    fn elemento(&self) -> AnyElement;
    /// Troca a tela desenhada.
    fn ir(&self, tela: &'static str, cx: &mut App);
    fn executar(&self, comando: &str, window: &mut Window, cx: &mut App);
}

type Abrir = fn(Env, PathBuf, &'static str, &mut Window, &mut App) -> Rc<dyn Aberta>;

/// Uma ferramenta linkada: o contrato e como abrir a view dela.
#[derive(Clone, Copy)]
pub struct Registro {
    pub ligada: Ligada,
    pub abrir: Abrir,
}

#[cfg_attr(
    not(any(
        feature = "moirai",
        feature = "cerberus",
        feature = "minos",
        feature = "thanatos",
        feature = "caronte"
    )),
    allow(dead_code)
)]
fn registro<T>(telas: &'static [TelaInfo]) -> Registro
where
    T: FerramentaEmbutida<Ambiente = (Env, PathBuf)>,
{
    Registro {
        ligada: Ligada {
            info: T::info(),
            telas,
        },
        abrir: abrir::<T>,
    }
}

/// As ferramentas linkadas, na ordem do trilho do painel web (`app.tsx:674`, com Moirai e Minos).
#[allow(clippy::vec_init_then_push)]
pub fn registros() -> Vec<Registro> {
    #[allow(unused_mut)]
    let mut out: Vec<Registro> = Vec::new();
    #[cfg(feature = "moirai")]
    out.push(registro::<moirai_app::MoiraiView>(moirai_app::telas(
        OPCOES,
    )));
    #[cfg(feature = "cerberus")]
    out.push(registro::<cerberus_app::CerberusView>(cerberus_app::telas(
        OPCOES,
    )));
    #[cfg(feature = "minos")]
    out.push(registro::<minos_app::MinosView>(minos_app::telas(OPCOES)));
    #[cfg(feature = "thanatos")]
    out.push(registro::<thanatos_app::ThanatosView>(telas_thanatos()));
    #[cfg(feature = "caronte")]
    out.push(registro::<caronte_app::CaronteView>(caronte_app::telas(
        OPCOES,
    )));
    out
}

/// O Thanatos `v0.1.0` não exporta `telas`; integrado, ele tira a última tela, Configurações
/// (`thanatos-app/src/view.rs`, `fn telas`).
#[cfg(feature = "thanatos")]
fn telas_thanatos() -> &'static [TelaInfo] {
    use hades_ui::FerramentaEmbutida as _;
    let t = thanatos_app::ThanatosView::info().telas;
    &t[..t.len().saturating_sub(1)]
}

/// As ferramentas ligadas, só o contrato.
pub fn ligadas() -> Vec<Ligada> {
    registros().into_iter().map(|r| r.ligada).collect()
}

/// Desenha a tela `tela` da view embutida e redesenha quando ela muda.
pub struct Moldura<T: FerramentaEmbutida> {
    view: Entity<T>,
    tela: &'static str,
}

impl<T: FerramentaEmbutida> Render for Moldura<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tela = self.tela;
        self.view
            .update(cx, |v, cx| v.render_tela(tela, window, cx))
    }
}

#[cfg_attr(
    not(any(
        feature = "moirai",
        feature = "cerberus",
        feature = "minos",
        feature = "thanatos",
        feature = "caronte"
    )),
    allow(dead_code)
)]
fn abrir<T>(
    env: Env,
    cwd: PathBuf,
    tela: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Rc<dyn Aberta>
where
    T: FerramentaEmbutida<Ambiente = (Env, PathBuf)>,
{
    let moldura = cx.new(|cx| {
        let view = cx.new(|cx| T::embutida((env, cwd), Some(tela), OPCOES, window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        Moldura { view, tela }
    });
    Rc::new(moldura)
}

impl<T: FerramentaEmbutida> Aberta for Entity<Moldura<T>> {
    fn elemento(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn ir(&self, tela: &'static str, cx: &mut App) {
        self.update(cx, |m, cx| {
            m.tela = tela;
            cx.notify();
        });
    }

    fn executar(&self, comando: &str, window: &mut Window, cx: &mut App) {
        let view = self.read(cx).view.clone();
        view.update(cx, |v, cx| v.executar(comando, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<&'static str> {
        ligadas().iter().map(|l| l.info.id).collect()
    }

    #[test]
    fn cada_feature_liga_a_sua_ferramenta() {
        let ids = ids();
        for (id, ligada) in [
            ("moirai", cfg!(feature = "moirai")),
            ("cerberus", cfg!(feature = "cerberus")),
            ("minos", cfg!(feature = "minos")),
            ("thanatos", cfg!(feature = "thanatos")),
            ("caronte", cfg!(feature = "caronte")),
        ] {
            assert_eq!(ids.contains(&id), ligada, "{id}");
        }
    }

    #[test]
    fn integrado_nao_traz_as_configuracoes_da_ferramenta() {
        for l in ligadas() {
            assert!(!l.telas.is_empty(), "{}", l.info.id);
            assert!(l.telas.len() < l.info.telas.len(), "{}", l.info.id);
            assert!(
                l.telas.iter().all(|t| !t.rotulo.starts_with("Config")),
                "{}",
                l.info.id
            );
            assert!(l.info.ids_repetidos().is_empty(), "{}", l.info.id);
        }
    }
}
