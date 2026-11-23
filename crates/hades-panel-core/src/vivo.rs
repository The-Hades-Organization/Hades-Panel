//! Serviços vivos que o painel consulta (herdr, systemd do usuário, git, coletor de consumo), atrás de uma trait para
//! os testes trocarem por respostas fixas.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// `LIVE_TIMEOUT_MS` (`panel/orchestration.ts`).
const HERDR_TIMEOUT: Duration = Duration::from_secs(5);
/// Teto do `git config --get remote.origin.url` (`panel/reviews-loop.ts`).
const GIT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);

pub trait Vivo {
    /// `herdr agent list`: `.result.agents`, ou vazio em qualquer falha.
    fn herdr_agents(&self, bin: &str) -> Vec<Value>;
    /// A unit do usuário está ativa (`systemctl --user is-active`).
    fn unit_ativa(&self, unit: &str) -> bool;
    /// `git -C <dir> config --get remote.origin.url`, aparado; `None` em falha.
    fn remote_url(&self, dir: &Path) -> Option<String>;
    /// Dispara o coletor de consumo em segundo plano, sem esperar.
    fn coletar_consumo(&self, skill_dir: &Path, days: &str);
}

/// Os serviços de verdade.
pub struct Sistema;

impl Vivo for Sistema {
    fn herdr_agents(&self, bin: &str) -> Vec<Value> {
        let Some(raw) = run(Command::new(bin).args(["agent", "list"]), HERDR_TIMEOUT) else {
            return Vec::new();
        };
        let raw = if raw.is_empty() { "{}".to_owned() } else { raw };
        match serde_json::from_str::<Value>(&raw) {
            Ok(v) => match v.get("result").and_then(|r| r.get("agents")) {
                Some(Value::Array(a)) => a.clone(),
                _ => Vec::new(),
            },
            Err(_) => Vec::new(),
        }
    }

    fn unit_ativa(&self, unit: &str) -> bool {
        Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", unit])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn remote_url(&self, dir: &Path) -> Option<String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(dir)
            .args(["config", "--get", "remote.origin.url"]);
        run(&mut cmd, GIT_TIMEOUT).map(|s| s.trim().to_owned())
    }

    fn coletar_consumo(&self, skill_dir: &Path, days: &str) {
        let _ = Command::new("bun")
            .arg(skill_dir.join("src").join("cli.ts"))
            .args(["usage", "--days", days])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Roda o comando com teto de tempo; `stdout` só quando sai com sucesso.
fn run(cmd: &mut Command, limite: Duration) -> Option<String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let leitor = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let inicio = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if inicio.elapsed() < limite => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let out = leitor.join().ok()?;
    status.success().then_some(out)
}
