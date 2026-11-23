//! Rotas `notify`, `onboard` e `plugins` (`panel/routes-misc.ts:182-191,263-270`, `panel/plugins-page.ts:60-80,116-120`):
//! o painel repassa o JSON dos scripts da pasta de dados (`notify.sh`, `onboard.sh`, `plugins.sh`).

use std::process::{Command, Stdio};

use serde_json::Value;

use crate::PanelEnv;

/// `NOTIFY_TIMEOUT_MS` (`routes-misc.ts:34`).
const NOTIFY_TIMEOUT_S: u64 = 20;
/// `ONBOARD_STATUS_TIMEOUT_MS` (`routes-misc.ts:32`).
const ONBOARD_STATUS_TIMEOUT_S: u64 = 45;
/// `LIST_TIMEOUT_MS` (`plugins-page.ts:20`).
const PLUGINS_LIST_TIMEOUT_S: u64 = 60;
/// Código de saída do `timeout` quando o tempo esgota.
const ESGOTOU: i32 = 124;

struct Saida {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Saida {
    fn esgotou(&self) -> bool {
        self.status == Some(ESGOTOU)
    }
}

/// `bash <data>/<nome> <args>` com `HADES_DATA` da pasta de dados (`hadesEnv`, `routes-misc.ts:83`), sob `timeout`.
fn script(e: &PanelEnv, nome: &str, args: &[&str], segundos: u64, extra: &[(&str, &str)]) -> Saida {
    let data = e.data();
    let mut cmd = Command::new("timeout");
    cmd.arg(segundos.to_string())
        .arg("bash")
        .arg(data.join(nome))
        .args(args)
        .env("HADES_DATA", data)
        .stdin(Stdio::null());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    match cmd.output() {
        Ok(o) => Saida {
            status: o.status.code(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        },
        Err(err) => Saida {
            status: None,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

/// `notifyStatus` (`routes-misc.ts:182-191,312`) no painel local (`--reveal`): saída vazia é `{}`.
pub fn notify_status(e: &PanelEnv) -> Result<Value, String> {
    let s = script(
        e,
        "notify.sh",
        &["status", "--json", "--reveal"],
        NOTIFY_TIMEOUT_S,
        &[],
    );
    notify_de(&s.stdout)
}

/// `parseOrEmpty` (`routes-misc.ts:185-191`).
pub fn notify_de(stdout: &str) -> Result<Value, String> {
    let t = if stdout.is_empty() { "{}" } else { stdout };
    serde_json::from_str(t).map_err(|_| "notify.sh não devolveu JSON".to_owned())
}

/// `onboardStatus` (`routes-misc.ts:263-270`); `quick` pula rede e docker.
pub fn onboard_status(e: &PanelEnv, quick: bool) -> Result<Value, String> {
    let mut args = vec!["status", "--json"];
    if quick {
        args.push("--quick");
    }
    let s = script(e, "onboard.sh", &args, ONBOARD_STATUS_TIMEOUT_S, &[]);
    onboard_de(s.status, &s.stdout, &s.stderr)
}

/// O corpo de `onboardStatus`: rc diferente de zero ou saída vazia é erro com a última linha do stderr.
pub fn onboard_de(status: Option<i32>, stdout: &str, stderr: &str) -> Result<Value, String> {
    if status != Some(0) || stdout.trim().is_empty() {
        let last = stderr
            .trim()
            .lines()
            .rfind(|l| !l.is_empty())
            .unwrap_or("sem saída");
        return Err(format!("onboard.sh falhou: {last}"));
    }
    serde_json::from_str(stdout).map_err(|_| "onboard.sh não devolveu JSON".to_owned())
}

/// `list` (`plugins-page.ts:116-120`) sobre o `cli(["list"])` (`plugins-page.ts:60-80`), no painel local.
pub fn plugins_list(e: &PanelEnv) -> Result<Value, String> {
    let s = script(
        e,
        "plugins.sh",
        &["list", "--json"],
        PLUGINS_LIST_TIMEOUT_S,
        &[("HADES_PANEL_JOB", "painel")],
    );
    if s.esgotou() {
        return Err("a operação passou do tempo (HADES_PLUGIN_GIT_TIMEOUT)".into());
    }
    plugins_de(s.status, &s.stdout)
}

/// A última linha do `plugins.sh … --json`: objeto com `ok: true` e rc 0, mais `remote: false`.
pub fn plugins_de(status: Option<i32>, stdout: &str) -> Result<Value, String> {
    const FALHOU: &str = "o plugins.sh falhou (detalhe no log do painel)";
    let last = stdout.trim().lines().next_back().unwrap_or_default();
    let Ok(Value::Object(mut out)) = serde_json::from_str::<Value>(last) else {
        return Err(FALHOU.into());
    };
    if status != Some(0) || out.get("ok") != Some(&Value::Bool(true)) {
        return Err(match out.get("error") {
            Some(Value::String(s)) => s.clone(),
            Some(v) if !v.is_null() => v.to_string(),
            _ => "falha ao listar".into(),
        });
    }
    out.insert("remote".into(), Value::Bool(false));
    Ok(Value::Object(out))
}

#[cfg(test)]
#[path = "ambiente_tests.rs"]
mod tests;
