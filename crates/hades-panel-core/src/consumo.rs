//! Rotas `usage` e `limits` (`panel/routes-misc.ts:201-225`, `lib/limites-aviso.ts:32`).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use moirai_core::limits::{Atuais, JANELAS, chaves, medidas, renova};
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use crate::leitura::{config_values, cwd, js_num};
use crate::vivo::Vivo;

/// `USAGE_STALE_S` (`routes-misc.ts:36`).
const USAGE_STALE_S: f64 = 900.0;
const DEFAULT_DAYS: &str = "7";

fn usage_file(env: &PanelEnv) -> PathBuf {
    env.env()
        .home()
        .join(".claude")
        .join("hades")
        .join("usage")
        .join("latest.json")
}

fn now_s() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `{data, age}` do `latest.json`: os dois `null` quando o arquivo falta ou não é JSON.
fn read_usage(file: &Path, now_s: f64) -> (Value, Option<f64>) {
    let parsed = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let mtime = std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());
    match (parsed, mtime) {
        (Some(data), Some(m)) => (data, Some(now_s - m)),
        _ => (Value::Null, None),
    }
}

/// `usage()` (`routes-misc.ts:201`): o último consumo coletado; vencido ou ausente dispara o coletor.
pub fn usage(env: &PanelEnv, vivo: &dyn Vivo) -> Value {
    usage_at(env, vivo, now_s())
}

fn usage_at(env: &PanelEnv, vivo: &dyn Vivo, now_s: f64) -> Value {
    let (data, age) = read_usage(&usage_file(env), now_s);
    let stale = age.is_none_or(|a| a > USAGE_STALE_S);
    if stale {
        let cfg = config_values(env);
        let days = cfg
            .get("HADES_USAGE_DAYS")
            .filter(|d| !d.is_empty())
            .map_or(DEFAULT_DAYS, String::as_str);
        vivo.coletar_consumo(&hades_core::paths::skill_root(env.env()), days);
    }
    json!({ "data": data, "age": age.map_or(Value::Null, js_num), "refreshing": stale })
}

/// `painelDeLimites` (`limites-aviso.ts:32`).
pub fn painel_de_limites(atual: &Atuais, now_ms: f64) -> Value {
    let janelas: Vec<Value> = JANELAS
        .iter()
        .filter_map(|&janela| {
            let a = atual.get(janela)?;
            let at = hades_core::journal::parse_iso(&a.at).map_or(f64::NAN, |ms| ms as f64);
            let idade = ((now_ms - at) / 1000.0).round();
            let idade = if idade.is_nan() {
                idade
            } else {
                idade.max(0.0)
            };
            let mut m = Map::new();
            m.insert("janela".into(), Value::String(janela.key().into()));
            m.insert("rotulo".into(), Value::String(janela.rotulo().into()));
            m.insert("pct".into(), js_num((a.pct * 10.0).round() / 10.0));
            m.insert("resetsAt".into(), js_num(a.resets_at));
            m.insert("renova".into(), Value::String(renova(janela, a.resets_at)));
            m.insert("idadeS".into(), js_num(idade));
            Some(Value::Object(m))
        })
        .collect();
    json!({ "coleta": !janelas.is_empty(), "janelas": janelas })
}

/// `limits()` (`routes-misc.ts:222`): o % medido das janelas, das amostras locais.
pub fn limits(env: &PanelEnv) -> Value {
    let now = now_s() * 1000.0;
    let m = medidas(env.env(), now, &chaves(env.env(), &cwd()));
    painel_de_limites(&m.atual, now)
}

#[cfg(test)]
#[path = "consumo_tests.rs"]
mod tests;
