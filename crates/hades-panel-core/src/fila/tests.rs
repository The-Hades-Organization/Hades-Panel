use super::estado::normalize_cmd;
use super::procs::{Proc, ProcTable, ancestors, descendants, parse_int};
use super::tempo::{iso_epoch, iso_utc_epoch, js_num};
use super::*;
use hades_core::Env;
use std::fs;

fn sandbox(nome: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hades-panel-fila-{nome}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn proc(pid: i64, ppid: i64, comm: &str, cmd: &str) -> Proc {
    Proc {
        pid,
        ppid,
        comm: comm.into(),
        cmd: cmd.into(),
        cpu: 0,
        start: 0,
        rss: 10,
        cwd: String::new(),
    }
}

#[test]
fn parse_int_como_o_js() {
    assert_eq!(parse_int("42abc"), Some(42));
    assert_eq!(parse_int(" -7"), Some(-7));
    assert_eq!(parse_int("x"), None);
}

#[test]
fn instantes_iso() {
    assert_eq!(
        iso_utc_epoch(Some("1970-01-02T00:00:00+03:00")),
        Some(86_400.0)
    );
    assert_eq!(iso_utc_epoch(Some("2026-11")), None);
    assert_eq!(iso_epoch("1970-01-01T00:00:01Z"), Some(1.0));
    assert_eq!(iso_epoch("1970-01-01T03:00:00+03:00"), Some(0.0));
    assert_eq!(iso_epoch("1970-01-01"), Some(0.0));
    assert_eq!(iso_epoch("ontem"), None);
    assert_eq!(js_num(2.0), Value::from(2));
    assert_eq!(js_num(f64::NAN), Value::Null);
}

#[test]
fn normaliza_o_comando_como_o_gate_lock() {
    assert_eq!(
        normalize_cmd("env A=1 B=2 /usr/bin/cargo test -j 1"),
        "cargo test -j 1"
    );
    assert_eq!(
        normalize_cmd("./node_modules/.bin/nx test demo"),
        "nx test demo"
    );
}

#[test]
fn arvore_e_ancestrais() {
    let t = ProcTable::from_procs(vec![
        proc(1, 0, "init", ""),
        proc(10, 1, "bash", ""),
        proc(11, 10, "node", ""),
        proc(12, 11, "jest", ""),
    ]);
    assert_eq!(descendants(10, &t.children), vec![10, 11, 12]);
    assert_eq!(ancestors(12, &t), vec![12, 11, 10]);
}

#[test]
fn ticket_da_fila_ordena_rodando_primeiro() {
    let w = sandbox("fila");
    let locks = w.join("acme__demo").join("locks");
    let q = locks.join("gate-queue");
    fs::create_dir_all(&q).unwrap();
    fs::write(
        q.join("a.t"),
        "label=rs-1\nkind=test\nstate=waiting\nsince=1970-01-01T00:00:05Z\nclone=/x/wt-a\n",
    )
    .unwrap();
    fs::write(
        q.join("b.t"),
        "label=hades-2\nkind=lint\nstate=running\ncost=2\nsince=1970-01-01T00:00:09Z\n",
    )
    .unwrap();
    let rows = gate_queue(Some(&locks));
    assert_eq!(rows[0]["label"], "hades-2");
    assert_eq!(rows[0]["local"], true);
    assert_eq!(rows[1]["clone"], "wt-a");
    assert_eq!(rows[1]["cost"], Value::Null);
    assert_eq!(rows[1]["since"], 5);
}

#[test]
fn sem_locks_a_fila_nao_esta_disponivel() {
    let d = sandbox("sem-locks");
    let env = PanelEnv::from_env(Env::empty().with("HADES_DATA", d.to_string_lossy())).unwrap();
    assert_eq!(fila(&env), json!({ "available": false, "gates": [] }));
}

#[test]
fn gate_rodando_lock_e_escalacao() {
    let w = sandbox("gate");
    let d = w.join("dados");
    let proc_root = w.join("proc");
    let repo = w.join("work").join("acme__demo");
    let locks = repo.join("locks");
    fs::create_dir_all(locks.join("gate-queue")).unwrap();
    fs::create_dir_all(locks.join("pr-12.lock")).unwrap();
    fs::create_dir_all(repo.join("state")).unwrap();
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("env.sh"),
        format!(
            "LOCKS=\"{}\"\nGATE_LOCK=\"{}\"\n",
            locks.display(),
            w.join("gl.sh").display()
        ),
    )
    .unwrap();
    fs::write(
        w.join("gl.sh"),
        "BUDGET_GB=${BUDGET_GB:-24}\nRESERVE_GB=${HADES_RESERVE:-4}\n",
    )
    .unwrap();
    fs::write(
        locks.join("gate-queue").join("7.t"),
        "pid=200\nstate=running\nsince=1970-01-01T00:01:00Z\n",
    )
    .unwrap();
    fs::write(
        locks.join("pr-12.lock").join("owner"),
        "mode=exclusive\nclone=/x/wt-a\nsince=1970-01-01T00:00:00Z\n",
    )
    .unwrap();
    fs::write(
        locks.join("gate-journal.tsv"),
        "t1\tdone\tdur=40s\tcmd=node nx.js test demo\n",
    )
    .unwrap();
    fs::write(
        repo.join("state").join("global.json"),
        r#"{"escalations":[{"id":"e1","pr":12,"why":"conflito","askedAt":"1970-01-01T00:00:00Z"},{"id":"e2","answeredAt":"x"}],"deliveries":{"a":{"status":"ok"},"b":{"status":"ok"},"c":{}}}"#,
    )
    .unwrap();
    fs::create_dir_all(proc_root.join("self")).unwrap();
    fs::write(proc_root.join("stat"), "cpu 0\nbtime 1\n").unwrap();
    fs::write(
        proc_root.join("meminfo"),
        "MemTotal: 100 kB\nMemAvailable: 50 kB\nSwapTotal: 4 kB\nSwapFree: 1 kB\n",
    )
    .unwrap();
    fs::write(proc_root.join("loadavg"), "0.50 1 2 1/2 3\n").unwrap();
    let mk = |pid: i64, ppid: i64, comm: &str, cmd: &str, start: i64| {
        let p = proc_root.join(pid.to_string());
        fs::create_dir_all(&p).unwrap();
        let rest = format!("S {ppid} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {start}");
        fs::write(p.join("stat"), format!("{pid} ({comm}) {rest}")).unwrap();
        fs::write(p.join("cmdline"), cmd.replace(' ', "\0")).unwrap();
        fs::write(p.join("statm"), "100 2 0").unwrap();
        std::os::unix::fs::symlink("/x/wt-a", p.join("cwd")).unwrap();
    };
    mk(
        200,
        1,
        "bash",
        "bash /h/gate-lock.sh run test --label rs-1 -- nx test demo",
        1000,
    );
    mk(201, 200, "node", "node /n/nx/bin/nx.js test demo", 2000);
    let env = PanelEnv::from_env(
        Env::empty()
            .with("HADES_DATA", d.to_string_lossy())
            .with("HADES_PROC_ROOT", proc_root.to_string_lossy()),
    )
    .unwrap();
    let sonda = Sonda {
        now: 100.0,
        cgroup_root: w.join("cg"),
        uid: 1,
        infra: &SemInfra,
    };
    let out = fila_com(&env, &sonda);
    assert_eq!(out["repo"], "acme__demo");
    let g = &out["gates"][0];
    assert_eq!(g["state"], "running");
    assert_eq!(g["label"], "rs-1");
    assert_eq!(g["age"], 89);
    assert_eq!(g["rss"], 16384);
    assert_eq!(g["runningFor"], 40);
    assert_eq!(g["stepMedian"], 40);
    assert_eq!(g["stepAge"], 79);
    assert_eq!(g["stepPct"], 99);
    assert_eq!(
        out["locks"][0],
        json!({"name":"pr-12","mode":"exclusive","clone":"/x/wt-a","age":100,"agentPresent":false})
    );
    assert_eq!(out["loop"]["escalationsOpen"], 1);
    assert_eq!(out["loop"]["deliveries"], json!({"ok":2,"?":1}));
    assert_eq!(out["budget"], json!({"budgetGb":24,"reserveGb":4}));
    assert_eq!(out["host"]["swapUsed"], 3072);
    assert_eq!(out["host"]["load"], json!([0.5, 1, 2]));
    assert_eq!(
        out["remote"],
        json!({"where":"local","autoAfterS":null,"workers":[]})
    );
}
