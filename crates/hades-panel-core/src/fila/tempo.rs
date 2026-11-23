//! Instantes ISO lidos pela fila e números no formato do `JSON.stringify`.

use serde_json::Value;

/// Dias desde 1970-01-01 da data civil (algoritmo de Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn num(s: &str) -> Option<i64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// `YYYY-MM-DD[THH:MM:SS[.fff]][Z|±HH:MM]` em segundos desde a época. Sem fuso, a hora vale como UTC.
fn parse_iso(text: &str) -> Option<f64> {
    let date = text.get(0..10)?;
    let (y, m, d) = (num(&date[0..4])?, num(&date[5..7])?, num(&date[8..10])?);
    if &date[4..5] != "-" || &date[7..8] != "-" || !(1..=12).contains(&m) || !(1..=31).contains(&d)
    {
        return None;
    }
    let mut secs = days_from_civil(y, m, d) as f64 * 86_400.0;
    let rest = &text[10..];
    if rest.is_empty() {
        return Some(secs);
    }
    let time = rest.strip_prefix('T')?;
    let (hh, mm, ss) = (
        num(time.get(0..2)?)?,
        num(time.get(3..5)?)?,
        num(time.get(6..8)?)?,
    );
    if hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    secs += (hh * 3600 + mm * 60 + ss) as f64;
    let mut tz = &time[8..];
    if let Some(frac) = tz.strip_prefix('.') {
        let digits: String = frac.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        let ms: String = digits.chars().chain("000".chars()).take(3).collect();
        secs += num(&ms)? as f64 / 1000.0;
        tz = &frac[digits.len()..];
    }
    match tz {
        "" | "Z" => Some(secs),
        _ => {
            let sign = match tz.as_bytes()[0] {
                b'+' => -1.0,
                b'-' => 1.0,
                _ => return None,
            };
            let off = tz[1..].replace(':', "");
            if off.len() != 4 {
                return None;
            }
            let (oh, om) = (num(&off[0..2])?, num(&off[2..4])?);
            Some(secs + sign * (oh * 3600 + om * 60) as f64)
        }
    }
}

/// `isoUtcEpoch` (`panel/gatewatch.ts:50-54`): os 19 primeiros caracteres lidos como UTC.
pub fn iso_utc_epoch(value: Option<&str>) -> Option<f64> {
    let text = value?;
    let head = text.get(0..19)?;
    let b = head.as_bytes();
    let shape = b.iter().enumerate().all(|(i, c)| match i {
        4 | 7 => *c == b'-',
        10 => *c == b'T',
        13 | 16 => *c == b':',
        _ => c.is_ascii_digit(),
    });
    if !shape {
        return None;
    }
    parse_iso(&format!("{head}Z"))
}

/// `isoEpoch` (`panel/system.ts:42-45`): `Date.parse` de texto que começa com data.
pub fn iso_epoch(text: &str) -> Option<f64> {
    let b = text.as_bytes();
    let starts = b.len() >= 10
        && b[..10].iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !starts {
        return None;
    }
    parse_iso(text)
}

/// Número como o `JSON.stringify` imprime: inteiro sem `.0`; `NaN` e infinito viram `null`.
pub fn js_num(x: f64) -> Value {
    if !x.is_finite() {
        Value::Null
    } else if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}

pub fn js_opt(x: Option<f64>) -> Value {
    x.map_or(Value::Null, js_num)
}

/// `String(x)` do JS para o que vem de JSON.
pub fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map_or_else(
            || n.to_string(),
            |f| match js_num(f) {
                Value::Number(n) => n.to_string(),
                _ => "NaN".to_owned(),
            },
        ),
        Value::Array(a) => a.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// Valor falso no JS (`!x`).
pub fn falsy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_none_or(|f| f == 0.0 || f.is_nan()),
        Some(_) => false,
    }
}
