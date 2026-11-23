use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use hades_core::Env;
use hades_core::config::load_values;

use super::*;

const CATALOGO: &str = "\
HADES_A\t1\tbool\tGeral\tChave A\tnow\tPrimeira.
HADES_B\tx\tenum:x,y\tGeral\tChave B\tnext\tSegunda.
HADES_C\t2\tint:1:9\tGeral\tChave C\trestart\tTerceira.
HADES_D\t3\tint:1:9\tGeral\tChave D\tnow\tQuarta.
HADES_SEGREDO\t\tsecret\tContas\tSegredo\tnow\tToken.
HADES_CAMINHO\t\tpath\tContas\tCaminho\tnow\tPasta.
HADES_SLICE_HIGH\t2G\tsize\tMemória\tPressão\tnow\tAlto.
HADES_SLICE_MAX\t4G\tsize\tMemória\tTeto\tnow\tMáximo.
HADES_DRAWING_ENGINE\tcardinal\tenum:hades,cardinal\tExtensões\tMotor\tnext\tMotor.
";

fn sandbox(nome: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("hades-panel-config-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.tsv"), CATALOGO).unwrap();
    dir
}

fn store_em(dir: &Path) -> ConfigStore {
    ConfigStore {
        dir: dir.to_owned(),
        kit: dir.to_owned(),
    }
}

#[test]
fn item_json_na_ordem_do_ts_e_plugin_so_quando_vem_do_plugin() {
    let dir = sandbox("item");
    std::fs::write(dir.join("config.env"), "HADES_B=y\n").unwrap();
    let items = effective(&store_em(&dir)).unwrap();
    let b = items.iter().find(|e| e.item.key == "HADES_B").unwrap();
    assert_eq!(
        hades_core::jsonfmt::json_compact(&item_json(b)),
        r#"{"key":"HADES_B","default":"x","type":"enum:x,y","group":"Geral","label":"Chave B","apply":"next","desc":"Segunda.","value":"y","changed":true}"#
    );
}

#[test]
fn motor_some_enquanto_cardinal_nao_serve_salvo_se_mudado() {
    let dir = sandbox("motor");
    let env = Env::empty()
        .with("HOME", dir.to_string_lossy())
        .with("HADES_SKILL_DIR", dir.to_string_lossy());
    let items = effective(&store_em(&dir)).unwrap();
    let eng = engine(&env, &items);
    assert_eq!(
        eng.reason.as_ref().unwrap().code,
        "engine.extension-missing"
    );
    assert_eq!(
        (eng.configured, eng.effective, eng.usable, eng.warn),
        ("cardinal", "hades", false, false)
    );
    assert!(
        visible_items(&items, &eng)
            .iter()
            .all(|e| e.item.key != "HADES_DRAWING_ENGINE")
    );

    std::fs::write(dir.join("config.env"), "HADES_DRAWING_ENGINE=hades\n").unwrap();
    let items = effective(&store_em(&dir)).unwrap();
    let eng = engine(&env, &items);
    assert_eq!(eng.configured, "hades");
    assert!(
        visible_items(&items, &eng)
            .iter()
            .any(|e| e.item.key == "HADES_DRAWING_ENGINE")
    );
}

#[test]
fn herdadas_do_settings() {
    let dir = sandbox("herdadas");
    let arquivo = dir.join("settings.json");
    let env = Env::empty().with("HADES_CLAUDE_SETTINGS", arquivo.to_string_lossy());
    assert_eq!(
        inherited_json(&env)["reason"],
        "arquivo ausente ou ilegível"
    );
    std::fs::write(&arquivo, "[1]").unwrap();
    assert_eq!(
        inherited_json(&env)["reason"],
        "o conteúdo não é um objeto JSON"
    );
    std::fs::write(&arquivo, r#"{"model":" opus ","effortLevel":3}"#).unwrap();
    let v = inherited_json(&env);
    assert_eq!(
        (v["ok"].as_bool(), v["model"].as_str(), v["effort"].as_str()),
        (Some(true), Some("opus"), Some(""))
    );
    assert_eq!(v["heirs"].as_array().unwrap().len(), 3);
}

#[test]
fn export_pula_segredo_e_identidade_e_conta_os_pulados() {
    let dir = sandbox("export");
    std::fs::write(
        dir.join("config.env"),
        "HADES_B=y\nHADES_SEGREDO=abc\nHADES_CAMINHO=/x\n",
    )
    .unwrap();
    let env = Env::empty()
        .with("HOME", dir.to_string_lossy())
        .with("TZ", "UTC");
    let now = 1_700_000_000_000;
    let texto = export_text(&env, &store_em(&dir), ExportOptions::default(), now).unwrap();
    assert_eq!(
        texto,
        "# Configurações do Hades — skill desconhecida — exportado em 2023-11-14 22:13\n# Valores do dono (só o que \
         difere do padrão). Importar: config.sh import <arquivo> [--dry-run]\n# 2 chave(s) de segredo ou desta máquina \
         ficaram de fora (--include-identity inclui contas e caminhos).\nHADES_B=y\n"
    );
    let opts = ExportOptions {
        all: true,
        identity: true,
    };
    let texto = export_text(&env, &store_em(&dir), opts, now).unwrap();
    assert!(texto.contains("(tudo; com contas e caminhos desta máquina)"));
    assert!(texto.contains("# 1 chave(s)"));
    assert!(texto.contains("HADES_A=1\n") && texto.contains("HADES_CAMINHO=/x\n"));
}

#[test]
fn plano_separa_mudanca_igual_invalida_desconhecida_e_segredo() {
    let dir = sandbox("plano");
    let texto = "# comentário\nHADES_B=x\nHADES_C=5\nHADES_C=7\nHADES_D=99\nHADES_ZZ=1\nHADES_SEGREDO=s\nlixo\n";
    let p = plan(&store_em(&dir), texto, &["HADES_A"]).unwrap();
    assert_eq!(p.same, ["HADES_B"]);
    assert_eq!(p.changes.len(), 1);
    assert_eq!(
        (
            p.changes[0].key.as_str(),
            p.changes[0].old.as_str(),
            p.changes[0].new.as_str()
        ),
        ("HADES_C", "2", "7")
    );
    assert_eq!(p.unknown, ["HADES_ZZ"]);
    assert_eq!(p.secret, ["HADES_SEGREDO"]);
    assert_eq!(p.invalid[0].0, "linha 8");
    assert_eq!(p.invalid[1].0, "HADES_D");
    assert_eq!(p.apply, ["restart"]);
    let p = plan(&store_em(&dir), "HADES_A=0\n", &["HADES_A"]).unwrap();
    assert_eq!(p.blocked, ["HADES_A"]);
}

#[test]
fn plano_recusa_pressao_acima_do_teto() {
    let dir = sandbox("fatia");
    let p = plan(
        &store_em(&dir),
        "HADES_SLICE_HIGH=8G\nHADES_SLICE_MAX=6G\nHADES_B=y\n",
        &[],
    )
    .unwrap();
    assert_eq!(
        p.invalid,
        [("HADES_SLICE_HIGH".to_owned(), SLICE_ERROR.to_owned())]
    );
    assert_eq!(
        p.changes.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
        ["HADES_B"]
    );
    assert_eq!(p.to_json()["invalid"][0]["key"], "HADES_SLICE_HIGH");
}

#[test]
fn gravacao_valida_antes_de_gravar() {
    let dir = sandbox("valida");
    let store = store_em(&dir);
    assert!(save_validated(&store, &[("HADES_C", "abc")], &[]).is_err());
    assert!(
        save_validated(&store, &[("HADES_ZZ", "1")], &[])
            .unwrap_err()
            .0
            .contains("chave desconhecida")
    );
    assert!(!dir.join("config.env").exists());
    assert_eq!(
        save_validated(&store, &[("HADES_C", " 7 ")], &[]).unwrap(),
        ["restart"]
    );
    assert_eq!(load_values(&store).get("HADES_C"), Some("7"));
}

#[test]
fn gravacoes_concorrentes_nao_perdem_chave() {
    let dir = sandbox("concorrente");
    let store = store_em(&dir);
    let chaves = [
        ("HADES_A", "0"),
        ("HADES_B", "y"),
        ("HADES_C", "5"),
        ("HADES_D", "8"),
    ];
    let largada = Arc::new(Barrier::new(chaves.len()));
    let fios: Vec<_> = chaves
        .iter()
        .map(|&(k, v)| {
            let store = store.clone();
            let largada = Arc::clone(&largada);
            thread::spawn(move || {
                largada.wait();
                for _ in 0..20 {
                    save_validated(&store, &[(k, v)], &[]).unwrap();
                }
            })
        })
        .collect();
    for f in fios {
        f.join().unwrap();
    }
    let gravado = load_values(&store);
    for (k, v) in chaves {
        assert_eq!(gravado.get(k), Some(v), "{k} perdida");
    }
}

#[test]
fn gravacao_espera_o_lock_do_config_env() {
    let dir = sandbox("lock");
    let store = store_em(&dir);
    let guarda = hades_core::lock::lock(&dir.join("config.env.lock")).unwrap();
    let fio = {
        let store = store.clone();
        thread::spawn(move || save_validated(&store, &[("HADES_B", "y")], &[]).unwrap())
    };
    thread::sleep(Duration::from_millis(150));
    assert!(!dir.join("config.env").exists(), "gravou sem o lock");
    drop(guarda);
    fio.join().unwrap();
    assert_eq!(load_values(&store).get("HADES_B"), Some("y"));
}
