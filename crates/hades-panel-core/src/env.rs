//! `PanelEnv`: de onde o painel lê. A pasta de dados (`$D`, `HADES_DATA`), a raiz de trabalho (`WORK_ROOT`), a raiz do
//! `/proc` (`HADES_PROC_ROOT`) e o relógio injetável do `hades-core`.

use std::path::{Path, PathBuf};

use hades_core::Env;
use hades_core::clock::{self, ClockError};
use hades_core::output::{HadesError, exit};

/// Mensagem do `die` de `commands/panel.ts:24` e `:42` quando falta `HADES_DATA`.
pub const SEM_DADOS: &str = "HADES_DATA não definido: aponte para a pasta de dados do painel";

#[derive(Clone, Debug)]
pub struct PanelEnv {
    env: Env,
    data: PathBuf,
    work_root: PathBuf,
    proc_root: PathBuf,
}

impl PanelEnv {
    /// `HADES_DATA` obrigatório, como em `commands/panel.ts:41-42`: ausente ou vazio é `die` com `EXIT.fail`.
    pub fn from_env(env: Env) -> Result<Self, HadesError> {
        let Some(data) = env.get("HADES_DATA").map(PathBuf::from) else {
            return Err(HadesError::with_code(SEM_DADOS, exit::FAIL));
        };
        let work_root = work_root(&env);
        let proc_root = proc_root(&env);
        Ok(Self {
            env,
            data,
            work_root,
            proc_root,
        })
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    /// `$D`: a pasta de dados do painel.
    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn work_root(&self) -> &Path {
        &self.work_root
    }

    pub fn proc_root(&self) -> &Path {
        &self.proc_root
    }

    /// `procPath` (`panel/system.ts:11`): caminho dentro do `/proc` lido pelo painel.
    pub fn proc_path<I, S>(&self, parts: I) -> PathBuf
    where
        I: IntoIterator<Item = S>,
        S: AsRef<Path>,
    {
        parts
            .into_iter()
            .fold(self.proc_root.clone(), |acc, p| acc.join(p))
    }

    /// `clockNow` (`src/lib/clock.ts:2`) pelo `hades-core`: `HADES_NOW` fixa o agora das fixtures.
    pub fn now_ms(&self) -> Result<i64, ClockError> {
        clock::now_ms(&self.env)
    }
}

/// `workRoot` (`panel/gatewatch.ts:71-75`): `HADES_WORK_ROOT`, senão a primeira linha de `~/.claude/hades/work-root`,
/// senão `~/.claude/hades/work`.
fn work_root(env: &Env) -> PathBuf {
    if let Some(dir) = env.get("HADES_WORK_ROOT") {
        return PathBuf::from(dir);
    }
    let hades = env.home().join(".claude").join("hades");
    let line = std::fs::read_to_string(hades.join("work-root"))
        .ok()
        .and_then(|t| t.split('\n').next().map(|l| l.trim().to_owned()))
        .unwrap_or_default();
    if line.is_empty() {
        hades.join("work")
    } else {
        PathBuf::from(line)
    }
}

/// `procRoot` (`panel/system.ts:10`): `HADES_PROC_ROOT || "/proc"`.
fn proc_root(env: &Env) -> PathBuf {
    PathBuf::from(env.get("HADES_PROC_ROOT").unwrap_or("/proc"))
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
