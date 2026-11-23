//! Leitores do núcleo do painel do Hades sobre o `hades-core`, sem gpui: só disco e `/proc`.

pub mod ambiente;
pub mod amostra;
pub mod config;
pub mod consumo;
pub mod docs;
pub mod env;
pub mod fila;
pub mod herdr;
pub mod leitura;
pub mod logs;
pub mod orquestracao;
pub mod qualidade;
pub mod revisoes;
pub mod vivo;

#[cfg(test)]
mod teste_vivo;

pub use env::PanelEnv;
