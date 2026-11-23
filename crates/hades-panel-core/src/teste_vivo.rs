//! Serviços vivos falsos e sandbox de disco para os testes.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use hades_core::Env;
use serde_json::Value;

use crate::PanelEnv;
use crate::vivo::Vivo;

#[derive(Default)]
pub struct Falso {
    pub agents: Vec<Value>,
    pub ativas: Vec<String>,
    pub remote: Option<String>,
    pub coletas: RefCell<Vec<String>>,
}

impl Vivo for Falso {
    fn herdr_agents(&self, _bin: &str) -> Vec<Value> {
        self.agents.clone()
    }
    fn unit_ativa(&self, unit: &str) -> bool {
        self.ativas.iter().any(|u| u == unit)
    }
    fn remote_url(&self, _dir: &Path) -> Option<String> {
        self.remote.clone()
    }
    fn coletar_consumo(&self, _skill_dir: &Path, days: &str) {
        self.coletas.borrow_mut().push(days.to_owned());
    }
}

pub fn sandbox(nome: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hades-panel-core-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data")).unwrap();
    dir
}

pub fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `PanelEnv` com HOME, `$D` e WORK_ROOT dentro do sandbox.
pub fn panel(home: &Path) -> PanelEnv {
    let env = Env::empty()
        .with("HOME", home.to_string_lossy())
        .with("HADES_DATA", home.join("data").to_string_lossy())
        .with("HADES_WORK_ROOT", home.join("work").to_string_lossy());
    PanelEnv::from_env(env).unwrap()
}
