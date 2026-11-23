//! Leitores do núcleo do painel do Hades sobre o `hades-core`, sem gpui: só disco e `/proc`.

pub mod amostra;
pub mod config;
pub mod consumo;
pub mod env;
pub mod fila;
pub mod leitura;
pub mod orquestracao;
pub mod revisoes;
pub mod vivo;

#[cfg(test)]
mod teste_vivo;

pub use env::PanelEnv;
