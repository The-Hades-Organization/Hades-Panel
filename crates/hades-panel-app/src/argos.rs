//! O Argos fica fora do binário (plano F5 painel §2.5): o estado vem de `argos-app --json` e o botão abre a janela
//! dele como processo. Sem o binário no PATH, a página diz "não instalado" e dá o link do canvas no painel web.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

pub const BINARIO: &str = "argos-app";

/// Rota do canvas no painel web (`route` de `kit/panel/ui/app.tsx:712`, página `argos`).
pub const ROTA_WEB: &str = "#/argos";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Estado {
    NaoInstalado,
    /// Instalado, mas sem `--json` nesta versão (ou a saída não é um objeto JSON).
    SemEstado,
    /// Os campos de primeiro nível com valor simples, na ordem da saída.
    Campos(Vec<(String, String)>),
    Falhou(String),
}

/// `argos-app` no `PATH`, como o shell o acharia.
pub fn achar(path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .map(|d| d.join(BINARIO))
        .find(|p| executavel(p))
}

fn executavel(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// O estado a partir do código de saída e da saída de `argos-app --json`.
pub fn estado_de(rc: Option<i32>, saida: &str) -> Estado {
    if rc != Some(0) {
        return Estado::SemEstado;
    }
    let Ok(Value::Object(m)) = serde_json::from_str::<Value>(saida.trim()) else {
        return Estado::SemEstado;
    };
    let campos = m
        .iter()
        .filter_map(|(k, v)| {
            let t = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "sim" } else { "não" }.to_owned(),
                Value::Array(a) => a.len().to_string(),
                _ => return None,
            };
            Some((k.clone(), t))
        })
        .collect();
    Estado::Campos(campos)
}

/// Roda `argos-app --json`. Bloqueia: chame no executor de fundo.
pub fn consultar() -> Estado {
    let Some(bin) = achar(std::env::var_os("PATH").as_deref()) else {
        return Estado::NaoInstalado;
    };
    match Command::new(bin)
        .arg("--json")
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) => estado_de(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        Err(e) => Estado::Falhou(e.to_string()),
    }
}

/// Abre a janela do Argos num processo à parte.
pub fn abrir_janela() -> Result<(), String> {
    let bin = achar(std::env::var_os("PATH").as_deref())
        .ok_or_else(|| format!("{BINARIO} não está no PATH"))?;
    Command::new(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Link do canvas no painel web, a partir da URL do painel (`hades panel service url`).
pub fn link_canvas(url_painel: Option<&str>) -> String {
    match url_painel.map(str::trim).filter(|u| u.starts_with("http")) {
        Some(u) => format!("{}/{ROTA_WEB}", u.trim_end_matches('/')),
        None => "`hades panel service url`, página Argos".to_owned(),
    }
}

/// A URL do painel web. Bloqueia: chame no executor de fundo.
pub fn url_painel() -> Option<String> {
    let out = Command::new("hades")
        .args(["panel", "service", "url"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_binario_no_path_nao_acha() {
        assert_eq!(achar(None), None);
        let d = std::env::temp_dir().join("hades-panel-argos-vazio");
        std::fs::create_dir_all(&d).expect("pasta");
        assert_eq!(achar(Some(d.as_os_str())), None);
    }

    #[test]
    fn acha_o_executavel_no_path() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("hades-panel-argos-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("pasta");
        let bin = d.join(BINARIO);
        std::fs::write(&bin, "#!/bin/sh\n").expect("binário");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("modo");
        let path = std::env::join_paths([Path::new("/nao/existe"), d.as_path()]).expect("PATH");
        assert_eq!(achar(Some(&path)), Some(bin));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn estado_le_os_campos_simples() {
        let e = estado_de(
            Some(0),
            r#"{"canvas":"default","agentes":[1,2,3],"vivo":true,"n":4,"x":{"a":1}}"#,
        );
        assert_eq!(
            e,
            Estado::Campos(vec![
                ("canvas".into(), "default".into()),
                ("agentes".into(), "3".into()),
                ("vivo".into(), "sim".into()),
                ("n".into(), "4".into()),
            ])
        );
    }

    #[test]
    fn versao_sem_json_fica_sem_estado() {
        assert_eq!(estado_de(Some(2), ""), Estado::SemEstado);
        assert_eq!(estado_de(Some(0), "Uso: argos-app"), Estado::SemEstado);
        assert_eq!(estado_de(Some(0), "[1]"), Estado::SemEstado);
    }

    #[test]
    fn link_do_canvas_web() {
        assert_eq!(
            link_canvas(Some("http://127.0.0.1:21220/\n")),
            "http://127.0.0.1:21220/#/argos"
        );
        assert!(link_canvas(None).contains("hades panel service url"));
        assert!(link_canvas(Some("erro")).contains("hades panel service url"));
    }
}
