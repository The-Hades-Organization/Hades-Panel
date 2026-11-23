//! Leitura que o app faz sobre o `hades-panel-core`; nada aqui roda na thread da interface.

use hades_core::Env;
use hades_panel_core::PanelEnv;
use serde_json::Value;

/// Uma amostra: o `snapshot` e a fila de gates, lidos na mesma ida ao disco.
#[derive(Clone, Debug)]
pub struct Dados {
    pub t_ms: i64,
    pub snapshot: Value,
    pub fila: Value,
}

/// Sem `HADES_DATA` o painel não tem o que ler: a mensagem vai para o estado vazio da página.
pub const SEM_DADOS: &str = "HADES_DATA não definido: abra o painel pelo `hades panel app`.";

pub fn carregar(env: Env) -> Result<Dados, String> {
    let env = PanelEnv::from_env(env).map_err(|_| SEM_DADOS.to_owned())?;
    Ok(Dados {
        t_ms: env.now_ms().unwrap_or(0),
        snapshot: hades_panel_core::amostra::snapshot(&env),
        fila: hades_panel_core::fila::fila(&env),
    })
}
