use super::*;

#[test]
fn instancia_sem_id_e_com_id_saneado() {
    let a = Instancia::com_id("");
    assert_eq!(
        (a.hu.as_str(), a.slice.as_str(), a.tunnel_id.as_str()),
        ("hds", "hades", "")
    );
    let b = Instancia::com_id("Meu-Box");
    assert_eq!(b.id, "meu_box");
    assert_eq!(b.hu, "hds_meu_box");
    assert_eq!(b.project, "hades_meu_box");
    assert_eq!(b.tunnel_id, "-meu-box");
}

#[test]
fn unit_view_inativa_e_ativa() {
    let vazia = unit_view(None, 100.0);
    assert_eq!(
        vazia,
        json!({"state":"inactive","sub":"dead","result":null,"restarts":0,"mem":null,"cpu":null,"age":null})
    );
    let u: Unit = [
        ("ActiveState", "active"),
        ("SubState", "running"),
        ("Result", "success"),
        ("NRestarts", "2"),
        ("MemoryCurrent", "4096"),
        ("ActiveEnterTimestampMonotonic", "40000000"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    let v = unit_view(Some(&u), 100.0);
    assert_eq!(v["restarts"], 2);
    assert_eq!(v["mem"], 4096);
    assert_eq!(v["age"], 60);
    assert_eq!(v["cpu"], Value::Null);
}

#[test]
fn overall_na_ordem_do_ts() {
    assert_eq!(overall(true, true, 0, 0, 0, 0, 0), "busy");
    assert_eq!(overall(false, true, 0, 0, 2, 1, 3), "paused");
    assert_eq!(overall(false, false, 0, 0, 2, 0, 3), "stopped");
    assert_eq!(overall(false, false, 2, 2, 2, 2, 3), "up");
    assert_eq!(overall(false, false, 2, 1, 2, 3, 3), "partial");
}

#[test]
fn repo_da_url_do_origin() {
    assert_eq!(
        repo_of("git@github.com:acme/demo.git").as_deref(),
        Some("acme/demo")
    );
    assert_eq!(
        repo_of("https://github.com/acme/demo/").as_deref(),
        Some("acme/demo")
    );
    assert_eq!(repo_of(""), None);
}

#[test]
fn catalogo_pula_comentario_e_linha_curta() {
    let dir = std::env::temp_dir().join(format!("amostra-cat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("services.tsv"),
        "# c\nms-a\t3001\tdev\tr\t-\tdb\tgw\ncurta\t1\nms-b\tx\tdev\t-\t-\t-\t-\n",
    )
    .unwrap();
    let cat = service_catalog(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(cat.len(), 2);
    assert_eq!(
        (cat[0].port, cat[0].redis, cat[0].db),
        (Some(3001), true, true)
    );
    assert_eq!((cat[1].port, cat[1].redis, cat[1].db), (None, false, false));
}
