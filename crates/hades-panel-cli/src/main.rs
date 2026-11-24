//! `hades-panel`: o `hades panel dump` (`src/commands/panel.ts`) em Rust. As rotas entram com os leitores do
//! `hades-panel-core`.

mod dump;

use std::process::ExitCode;

use hades_core::Env;
use hades_core::errors::render_error;
use hades_core::output::{exit, out, warn};
use hades_panel_core::PanelEnv;

const PREFIX: &str = "hades";

/// `USAGE` (`commands/panel.ts:8`).
const USAGE: &str = "Uso: hades panel serve [--port <porta>]   (HADES_DATA = pasta de dados; padrão 21220)
     hades panel service start|stop|restart|status|url|autostart   o painel como serviço do usuário (kit/panel.sh)
     hades panel dump <rota> [k=v ...]   o JSON de uma rota GET do painel, sem subir HTTP (rc 2 lista as rotas)
     hades panel action <id> [k=v ...] --json   lança a ação da rota POST /api/action, sem HTTP (rc 2 lista as ações)
     hades panel action --list --json   as ações com id, rótulo, ops e as que pedem confirmação
     hades panel app   abre o app gpui do painel (hades-panel-app, HADES_PANEL_APP_BIN), desacoplado do terminal";

/// `panelCommand` (`commands/panel.ts:12`), só com o `dump`.
fn panel(env: Env, args: &[String]) -> i32 {
    match args.split_first() {
        Some((sub, rest)) if sub == "dump" => run_dump(env, rest),
        _ => {
            warn(USAGE);
            exit::USAGE
        }
    }
}

/// `dump` (`commands/panel.ts:35-46`): sem rota, o uso; sem `HADES_DATA`, falha antes de procurar a rota.
fn run_dump(env: Env, args: &[String]) -> i32 {
    let Some((route, query)) = args.split_first() else {
        warn(USAGE);
        return exit::USAGE;
    };
    let panel = match PanelEnv::from_env(env) {
        Ok(p) => p,
        Err(e) => {
            warn(&render_error(&e, PREFIX));
            return e.code;
        }
    };
    let (rc, body) = dump::panel_dump(&panel, route, query);
    match body {
        dump::Body::Json(v) => out(&hades_core::jsonfmt::json_indent(&v, 2)),
        dump::Body::Text(t) => print!("{t}"),
    }
    rc
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let rc = panel(Env::from_process(), &args);
    ExitCode::from(u8::try_from(rc).unwrap_or(1))
}
