//! `panelDump` (`panel/dump.ts:81`): a rota pedida contra a tabela de leituras do painel.

use hades_panel_core::PanelEnv;
use hades_panel_core::config::{self, ExportOptions};
use serde_json::{Value, json};

/// Rotas com leitor no Rust. As rotas que terminam em `/` levam um nome depois (`<rota><nome>`).
const ROUTES: &[&str] = &["config", "config/inherited", "config/export", "doctor"];

/// Corpo da rota: JSON indentado, ou o texto cru das rotas `text/plain` (`config/export`).
#[derive(Debug, PartialEq)]
pub enum Body {
    Json(Value),
    Text(String),
}

/// Caminho da rota sem `/`, `/api/` e a barra final (`panel/dump.ts:82-83`).
fn route_path(route: &str) -> &str {
    let path = route.split('?').next().unwrap_or_default();
    let path = path.strip_prefix('/').unwrap_or(path);
    let path = path.strip_prefix("api/").unwrap_or(path);
    path.strip_suffix('/').unwrap_or(path)
}

/// Chave da tabela que atende o caminho (`panel/dump.ts:87`).
fn route_key(path: &str) -> Option<&'static str> {
    let exata = (!path.ends_with('/'))
        .then(|| ROUTES.iter().copied().find(|k| *k == path))
        .flatten();
    exata.or_else(|| {
        ROUTES
            .iter()
            .copied()
            .find(|k| k.ends_with('/') && path.starts_with(k) && path.len() > k.len())
    })
}

/// `panelDump` (`panel/dump.ts:81-90`): rota desconhecida é rc 2 com a lista das rotas.
pub fn panel_dump(env: &PanelEnv, route: &str, args: &[String]) -> (i32, Body) {
    let path = route_path(route);
    let q = Query::parse(route, args);
    match route_key(path).map(|key| read(env, key, &q)) {
        Some(Ok(body)) => (0, body),
        Some(Err(message)) => (1, Body::Json(json!({ "error": message }))),
        None => (2, Body::Json(unknown(route))),
    }
}

/// Leitor de cada rota de `ROUTES`.
fn read(env: &PanelEnv, key: &str, q: &Query) -> Result<Body, String> {
    let fail = |e: hades_core::config::ConfigError| e.0;
    match key {
        "config" => config::state_json(env).map(Body::Json).map_err(fail),
        "config/inherited" => Ok(Body::Json(config::inherited_json(env.env()))),
        "config/export" => {
            let opts = ExportOptions {
                all: q.get("all") == Some("1"),
                identity: q.get("identity") == Some("1"),
            };
            let now = env.now_ms().map_err(|e| e.to_string())?;
            config::export_text(env.env(), &config::store(env), opts, now)
                .map(Body::Text)
                .map_err(fail)
        }
        "doctor" => config::doctor_json(env).map(Body::Json).map_err(fail),
        _ => Err(format!("rota sem leitor: {key}")),
    }
}

/// A query da rota (`?a=1&b=2`) seguida dos `k=v` do argv, como o `URLSearchParams` do TS: o primeiro valor de cada
/// chave vence e pedaço vazio é ignorado.
struct Query(Vec<(String, String)>);

impl Query {
    fn parse(route: &str, args: &[String]) -> Self {
        let search = route.split_once('?').map(|(_, s)| s).unwrap_or_default();
        let pairs = search
            .split('&')
            .chain(args.iter().map(String::as_str))
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (form_decode(k), form_decode(v))
            })
            .collect();
        Self(pairs)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// `application/x-www-form-urlencoded`: `+` é espaço e `%XX` é byte; `%` sem dois hexas fica como está.
fn form_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if hex(i + 1).is_some() && hex(i + 2).is_some() => {
                out.push((hex(i + 1).unwrap_or(0) * 16 + hex(i + 2).unwrap_or(0)) as u8);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unknown(route: &str) -> Value {
    let routes: Vec<String> = ROUTES
        .iter()
        .map(|k| {
            if k.ends_with('/') {
                format!("{k}<nome>")
            } else {
                (*k).to_owned()
            }
        })
        .collect();
    json!({
        "error": format!("rota desconhecida: {route}"),
        "code": "unknown_route",
        "routes": routes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hades_core::Env;

    #[test]
    fn caminho_sem_prefixo_nem_barra_final() {
        assert_eq!(route_path("config"), "config");
        assert_eq!(route_path("/api/config/"), "config");
        assert_eq!(route_path("api/config?x=1"), "config");
        assert_eq!(route_path("/orchestration/file"), "orchestration/file");
    }

    #[test]
    fn rota_desconhecida_lista_as_rotas() {
        let env = PanelEnv::from_env(Env::empty().with("HADES_DATA", "/sandbox/dados")).unwrap();
        let (rc, body) = panel_dump(&env, "/api/nada?x=1", &[]);
        assert_eq!(rc, 2);
        let Body::Json(body) = body else {
            panic!("esperava JSON")
        };
        assert_eq!(body["error"], "rota desconhecida: /api/nada?x=1");
        assert_eq!(body["routes"], json!(ROUTES));
    }

    #[test]
    fn query_decodifica_e_o_primeiro_valor_vence() {
        let q = Query::parse(
            "config/export?all=1&&nome=a+b%C3%A9&all=0",
            &["identity=1".to_owned(), "all=2".to_owned()],
        );
        assert_eq!(q.get("all"), Some("1"));
        assert_eq!(q.get("nome"), Some("a bé"));
        assert_eq!(q.get("identity"), Some("1"));
        assert_eq!(form_decode("100%"), "100%");
    }
}
