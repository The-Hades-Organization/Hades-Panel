use std::path::{Path, PathBuf};

use hades_core::Env;
use hades_core::output::exit;

use super::*;

fn sandbox(nome: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hades-panel-core-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn sem_hades_data_e_falha_com_rc_1() {
    for env in [Env::empty(), Env::empty().with("HADES_DATA", "")] {
        let e = PanelEnv::from_env(env).unwrap_err();
        assert_eq!(e.code, exit::FAIL);
        assert_eq!(e.message, SEM_DADOS);
    }
}

#[test]
fn variaveis_explicitas_valem() {
    let p = PanelEnv::from_env(
        Env::empty()
            .with("HOME", "/sandbox/home")
            .with("HADES_DATA", "/sandbox/dados")
            .with("HADES_WORK_ROOT", "/sandbox/work")
            .with("HADES_PROC_ROOT", "/sandbox/proc"),
    )
    .unwrap();
    assert_eq!(p.data(), Path::new("/sandbox/dados"));
    assert_eq!(p.work_root(), Path::new("/sandbox/work"));
    assert_eq!(p.proc_root(), Path::new("/sandbox/proc"));
    assert_eq!(
        p.proc_path(["123", "status"]),
        Path::new("/sandbox/proc/123/status")
    );
}

#[test]
fn padroes_sem_variaveis() {
    let home = sandbox("padroes");
    let p = PanelEnv::from_env(
        Env::empty()
            .with("HOME", home.to_string_lossy())
            .with("HADES_DATA", "/sandbox/dados")
            .with("HADES_PROC_ROOT", ""),
    )
    .unwrap();
    assert_eq!(p.work_root(), home.join(".claude/hades/work"));
    assert_eq!(p.proc_root(), Path::new("/proc"));
}

#[test]
fn work_root_pela_primeira_linha_do_arquivo() {
    let home = sandbox("arquivo");
    let hades = home.join(".claude/hades");
    std::fs::create_dir_all(&hades).unwrap();
    std::fs::write(hades.join("work-root"), "  /sandbox/outro  \nignorada\n").unwrap();
    let env = Env::empty()
        .with("HOME", home.to_string_lossy())
        .with("HADES_DATA", "/sandbox/dados");
    assert_eq!(
        PanelEnv::from_env(env.clone()).unwrap().work_root(),
        Path::new("/sandbox/outro")
    );
    std::fs::write(hades.join("work-root"), "\n").unwrap();
    assert_eq!(
        PanelEnv::from_env(env).unwrap().work_root(),
        hades.join("work")
    );
}

#[test]
fn relogio_pelo_hades_now() {
    let base = Env::empty().with("HADES_DATA", "/sandbox/dados");
    let p = PanelEnv::from_env(base.clone().with("HADES_NOW", "2026-01-02T03:04:05Z")).unwrap();
    assert_eq!(p.now_ms(), Ok(1_767_323_045_000));
    let p = PanelEnv::from_env(base.with("HADES_NOW", "ontem")).unwrap();
    assert!(p.now_ms().is_err());
}
