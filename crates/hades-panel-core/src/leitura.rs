//! Ajudantes comuns das leituras do painel: erro de rota, valores de configuração, `env.sh` de `$D` e JSON de disco.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hades_core::config::{effective, read_env_sh};
use hades_core::fsx::list_dir;
use serde_json::{Map, Number, Value};

use crate::PanelEnv;

/// Código do `BadRequest` do painel (`panel/http.ts`).
pub const BAD_REQUEST: &str = "panel_bad_request";

/// Erro de uma rota: a mensagem e, quando a rota tem, o código estável (`{error, code}` no `dump`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelError {
    pub message: String,
    pub code: Option<&'static str>,
}

impl PanelError {
    /// `BadRequest` (`panel/http.ts`): rc 1 com `code: "panel_bad_request"`.
    pub fn bad(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: Some(BAD_REQUEST),
        }
    }

    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }

    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("error".into(), Value::String(self.message.clone()));
        if let Some(code) = self.code {
            m.insert("code".into(), Value::String(code.into()));
        }
        Value::Object(m)
    }
}

/// `configValues` (`panel/routes-core.ts:45`): o valor efetivo de cada chave do catálogo (`config.env` e catálogo,
/// sem variável de ambiente); `{}` quando o catálogo falha.
pub fn config_values(env: &PanelEnv) -> HashMap<String, String> {
    effective(&crate::config::store(env))
        .map(|items| items.into_iter().map(|e| (e.item.key, e.value)).collect())
        .unwrap_or_default()
}

/// `env.locks`: `LOCKS` do `$D/env.sh`, `None` quando ausente ou vazio.
pub fn locks(env: &PanelEnv) -> Option<PathBuf> {
    let sh = read_env_sh(env.data());
    sh.get("LOCKS").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `env.worktree`: `W` do `$D/env.sh`, senão `$D` sem o sufixo `-data`.
pub fn worktree(env: &PanelEnv) -> PathBuf {
    let sh = read_env_sh(env.data());
    if let Some(w) = sh.get("W").filter(|v| !v.is_empty()) {
        return PathBuf::from(w);
    }
    let data = env.data().to_string_lossy();
    let cut = data.char_indices().rev().nth(4).map_or(0, |(i, _)| i);
    PathBuf::from(&data[..cut])
}

/// `readJson`: só objeto JSON vale.
pub fn read_json(path: &Path) -> Option<Map<String, Value>> {
    hades_core::work::read_json(path)
}

/// `jsonFiles`: os `*.json` da pasta, em ordem.
pub fn json_files(dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = list_dir(dir)
        .into_iter()
        .filter(|f| f.ends_with(".json"))
        .collect();
    files.sort();
    files
}

/// `Number` do JS como nó JSON: inteiro sem `.0`, `NaN` e infinito viram `null`.
pub fn js_num(n: f64) -> Value {
    if !n.is_finite() {
        return Value::Null;
    }
    if n.fract() == 0.0 && n.abs() < 9e15 {
        return Value::Number(Number::from(n as i64));
    }
    Number::from_f64(n).map_or(Value::Null, Value::Number)
}

/// Verdade do JS para um valor JSON.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// `String(v)` do JS para os campos lidos de JSON (texto, número ou booleano).
pub fn js_string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) => "null".into(),
        Some(other) => other.to_string(),
    }
}

/// Diretório de trabalho do processo, para as leituras que seguem o `process.cwd()` do TS.
pub fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn erro_com_e_sem_codigo() {
        assert_eq!(
            PanelError::bad("x").to_value(),
            json!({"error": "x", "code": "panel_bad_request"})
        );
        assert_eq!(PanelError::plain("y").to_value(), json!({"error": "y"}));
    }

    #[test]
    fn numero_do_js() {
        assert_eq!(js_num(0.0), json!(0));
        assert_eq!(js_num(1.5), json!(1.5));
        assert_eq!(js_num(f64::NAN), Value::Null);
    }

    #[test]
    fn verdade_do_js() {
        assert!(!truthy(&json!("")));
        assert!(!truthy(&json!(0)));
        assert!(truthy(&json!("idle")));
        assert!(truthy(&json!({})));
    }
}
