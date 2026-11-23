//! `panelDump` (`panel/dump.ts:81`): a rota pedida contra a tabela de leituras do painel.

use hades_panel_core::PanelEnv;
use serde_json::{Value, json};

/// Rotas com leitor no Rust. As rotas que terminam em `/` levam um nome depois (`<rota><nome>`).
const ROUTES: &[&str] = &[];

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
pub fn panel_dump(env: &PanelEnv, route: &str, query: &[String]) -> (i32, Value) {
    let path = route_path(route);
    match route_key(path).and_then(|key| read(env, key, &path[key.len()..], query)) {
        Some(body) => (0, body),
        None => (2, unknown(route)),
    }
}

/// Leitor de cada rota de `ROUTES`; os leitores entram com os módulos do `hades-panel-core`.
fn read(_env: &PanelEnv, _key: &str, _rest: &str, _query: &[String]) -> Option<Value> {
    None
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
    fn toda_rota_e_desconhecida_por_enquanto() {
        let env = PanelEnv::from_env(Env::empty().with("HADES_DATA", "/sandbox/dados")).unwrap();
        let (rc, body) = panel_dump(&env, "/api/config?x=1", &[]);
        assert_eq!(rc, 2);
        assert_eq!(
            hades_core::jsonfmt::json_indent(&body, 2),
            "{\n  \"error\": \"rota desconhecida: /api/config?x=1\",\n  \"code\": \"unknown_route\",\n  \"routes\": []\n}"
        );
    }
}
