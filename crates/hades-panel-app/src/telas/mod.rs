//! Telas do painel além da Visão geral: cada uma num módulo com o view-model puro e a view gpui dela.

pub mod ambiente_vm;
pub mod config;
pub mod config_vm;
pub mod consumo;
pub mod fila;
pub mod loop_;
pub mod orquestracao;
pub mod orquestracao_vm;
pub mod revisoes;
pub mod revisoes_vm;
pub mod sistema;
pub mod sistema_vm;
mod tela;
mod ui;

pub use tela::{Arquivo, RECARGA, Tela, arquivo_sheet, panel_env, recarga_periodica, tone};
pub use ui::{Detalhe, detalhe_sheet};

use serde_json::Value;

pub(crate) const GB: f64 = 1024.0 * 1024.0 * 1024.0;
const MB: f64 = 1024.0 * 1024.0;

/// `dur` de `app.tsx:94`: segundos em s, min, h com uma casa ou d.
pub fn dur(s: Option<f64>) -> String {
    let Some(s) = s.filter(|s| s.is_finite()) else {
        return "—".into();
    };
    if s < 60.0 {
        format!("{} s", s.round())
    } else if s < 3600.0 {
        format!("{} min", (s / 60.0).round())
    } else if s < 86400.0 {
        let h = format!("{:.1}", s / 3600.0);
        format!("{} h", h.trim_end_matches(".0").replace('.', ","))
    } else {
        format!("{} d", (s / 86400.0).round())
    }
}

/// `gb` de `app.tsx:92`: uma casa abaixo de 10 GB, nenhuma acima.
pub fn gb(b: Option<f64>) -> String {
    let Some(b) = b else { return "—".into() };
    let casas = if b >= 10.0 * GB { 0 } else { 1 };
    format!("{:.*} GB", casas, b / GB).replace('.', ",")
}

/// `mb` de `app.tsx:93`: MB inteiros abaixo de 1 GB.
pub fn mb(b: Option<f64>) -> String {
    match b {
        None => "—".into(),
        Some(b) if b >= GB => gb(Some(b)),
        Some(b) => format!("{} MB", (b / MB).round()),
    }
}

/// `usd` de `usage.tsx:10`: dólares inteiros com milhar em ponto.
pub fn usd(v: Option<f64>) -> String {
    let Some(v) = v else { return "—".into() };
    format!("US$ {}", milhar(v.round() as i64))
}

/// `ktok` de `usage.tsx:11`.
pub fn ktok(v: Option<f64>) -> String {
    v.map_or_else(|| "—".into(), |v| format!("{}k", (v / 1000.0).round()))
}

/// `toLocaleString('pt-BR')` de inteiro.
pub fn milhar(n: i64) -> String {
    let d = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Uma casa decimal com vírgula (`toFixed(1).replace('.', ',')`).
pub fn decimal_pt(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

pub fn plural(n: u64, um: &str, varios: &str) -> String {
    format!("{n} {}", if n == 1 { um } else { varios })
}

/// Percentual com uma casa e vírgula, sem `,0` (`pct1` de `usage.tsx:70`).
pub fn pct1(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    format!("{r}%").replace('.', ",")
}

pub(crate) fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

pub(crate) fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn texto(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn sim(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Último trecho de um caminho (`tail` de `queue.tsx:31`).
pub(crate) fn cauda(p: &str) -> String {
    p.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(p)
        .to_owned()
}

/// Segundos desde um ISO; `None` quando o texto não é data.
pub(crate) fn desde_iso(iso: &str, now_s: f64) -> Option<f64> {
    hades_core::journal::parse_iso(iso).map(|ms| now_s - ms as f64 / 1000.0)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
