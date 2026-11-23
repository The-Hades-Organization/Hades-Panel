//! Fila de gates e estado do loop (`panel/gatewatch.ts`, rota `gates`): locks, tickets da fila, gates vivos no
//! `/proc`, escalações, entregas, sumários e o journal da fila. Uma amostra só: o histórico de 1 h é memória do
//! processo e fica com o app (plano F5 §4.4).

pub mod estado;
pub mod procs;
pub mod tempo;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use hades_core::config::{read_config_env, read_env_sh};
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::PanelEnv;
use estado::{
    cut, journal_lines, journal_tail, normalize_cmd, recent_gates, skill_state, step_durations,
};
use procs::{
    DAEMON_RE, HEAVY_RE, Proc, ProcTable, ancestors, boot_epoch, cgroup_usage, descendants, fields,
    gate_scope, load_average, meminfo, pressure, readdir_sorted, scan_procs,
};
use tempo::{iso_epoch, iso_utc_epoch, js_num, js_opt};

/// `MACHINE` (`gatewatch.ts:18`): a fila da máquina, sem slug de repositório.
pub const MACHINE: &str = "_maquina";
const CLK_TCK: f64 = 100.0;
const BYPASS_MIN_RSS: i64 = 150 * 1024 * 1024;
const BYPASS_MIN_CPU: f64 = 20.0;
const STEP_PCT_CAP: f64 = 99.0;
const GATE_CMD_MAX: usize = 4000;
const STEP_CMD_MAX: usize = 160;
const DETAIL_JOURNAL: usize = 60;
const REMOTE_PREFIX: &str = "remote:";
const AGENT_COMMS: [&str; 2] = ["claude", "agy"];
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

static GATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"gate-lock(?:\.sh)?\s+run\s+(\S+)(.*?)\s--\s(.*)$").unwrap());
static SHELL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(bash|bun)\b").unwrap());
static LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--label\s+(\S+)").unwrap());
static SLOT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^gate-.*\.slot-").unwrap());

/// Medição de infraestrutura (docker e daemons do host, `infraUsage` de `gatewatch-proc.ts:305`): fora da matriz
/// (plano F5 §4.4), atrás de trait para o teste.
pub trait Infra {
    /// Bytes somados e os itens, na forma do `InfraItem`.
    fn usage(&self, table: &ProcTable) -> (i64, Vec<Value>);
}

/// Sem docker nem daemons medidos.
pub struct SemInfra;

impl Infra for SemInfra {
    fn usage(&self, _table: &ProcTable) -> (i64, Vec<Value>) {
        (0, Vec::new())
    }
}

/// O que a amostra lê fora de `$D` e do `/proc`: o agora, o cgroup e o uid.
pub struct Sonda<'a> {
    pub now: f64,
    pub cgroup_root: PathBuf,
    pub uid: u32,
    pub infra: &'a dyn Infra,
}

impl Sonda<'_> {
    /// O agora do `PanelEnv` (`HADES_NOW` fixa a fixture), o cgroup do sistema e o uid do processo.
    pub fn do_sistema(env: &PanelEnv) -> Sonda<'static> {
        use std::os::unix::fs::MetadataExt;
        let now = env.now_ms().map_or_else(|_| 0.0, |ms| ms as f64 / 1000.0);
        let uid = std::fs::metadata("/proc/self").map_or(0, |m| m.uid());
        Sonda {
            now,
            cgroup_root: PathBuf::from(CGROUP_ROOT),
            uid,
            infra: &SemInfra,
        }
    }
}

/// `LOCKS` do `env.sh` de `$D` (`panel/env.ts:55`): ausente ou vazio é `None`.
pub fn locks_dir(env: &PanelEnv) -> Option<PathBuf> {
    read_env_sh(env.data())
        .get("LOCKS")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn parent2(p: &Path) -> PathBuf {
    p.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// `gateQueue` (`panel/sampler.ts:227-247`): os tickets `*.t` da fila do projeto e da fila da máquina.
pub fn gate_queue(locks: Option<&Path>) -> Vec<Value> {
    let Some(locks) = locks else {
        return Vec::new();
    };
    let dirs = [
        locks.join("gate-queue"),
        parent2(locks)
            .join(MACHINE)
            .join("locks")
            .join("gate-queue"),
    ];
    let mut files: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|d| {
            readdir_sorted(d)
                .into_iter()
                .filter(|f| f.ends_with(".t"))
                .map(move |f| d.join(f))
        })
        .collect();
    files.sort();
    let prefix = format!("{}/", locks.display());
    let mut rows: Vec<(bool, f64, Value)> = files
        .iter()
        .map(|path| {
            let f = fields(path);
            let get = |k: &str| f.get(k).cloned();
            let label = get("label");
            let since = iso_epoch(&get("since").unwrap_or_default());
            let state = get("state").unwrap_or_else(|| "-".into());
            let clone = get("clone").unwrap_or_default();
            let base = Path::new(&clone).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let row = json!({
                "label": label.clone().unwrap_or_else(|| "-".into()), "kind": get("kind").unwrap_or_else(|| "-".into()),
                "state": state, "cost": get("cost"), "since": js_opt(since), "clone": base,
                "local": label.unwrap_or_default().starts_with("hades-"),
                "machine": !path.to_string_lossy().starts_with(&prefix),
            });
            (state != "running", since.unwrap_or(0.0), row)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    rows.into_iter().map(|r| r.2).collect()
}

/// Dono de um slot (v1) ou ticket (v2) da fila (`Owner`, `gatewatch.ts:44-47`).
#[derive(Clone, Debug)]
struct Owner {
    f: HashMap<String, String>,
    slot: String,
    v1: bool,
}

impl Owner {
    fn get(&self, k: &str) -> Option<&str> {
        self.f.get(k).map(String::as_str)
    }

    /// `admitted` (`gatewatch.ts:375`).
    fn admitted(&self) -> bool {
        self.v1 || self.get("state") == Some("running")
    }
}

/// `remoteHostOf` (`gatewatch.ts:136`).
fn remote_host_of(state: Option<&str>) -> Option<String> {
    state
        .and_then(|s| s.strip_prefix(REMOTE_PREFIX))
        .filter(|h| !h.is_empty())
        .map(str::to_owned)
}

fn digits(s: Option<&str>) -> Option<i64> {
    s.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse().ok())
}

/// `belongs` (`gatewatch.ts:281-286`).
fn belongs(cwd: &str, repo: &str, owner: Option<&Owner>, named: &[String]) -> bool {
    if repo == MACHINE {
        return owner.is_some();
    }
    if named.len() <= 1 {
        return true;
    }
    owner.is_some() || cwd.contains(repo.rsplit("__").next().unwrap_or(repo))
}

/// `currentStep` (`gatewatch.ts:274-279`): o passo pesado mais raso sob o gate, em largura.
fn current_step(pid: i64, table: &ProcTable) -> Option<(String, &Proc)> {
    let mut queue: std::collections::VecDeque<i64> =
        table.children.get(&pid).cloned().unwrap_or_default().into();
    while let Some(cur) = queue.pop_front() {
        if let Some(p) = table.get(cur)
            && HEAVY_RE.is_match(&p.cmd)
            && !DAEMON_RE.is_match(&p.cmd)
        {
            return Some((cut(&p.cmd, STEP_CMD_MAX), p));
        }
        queue.extend(table.children.get(&cur).into_iter().flatten());
    }
    None
}

fn is_agent(p: &Proc) -> bool {
    if AGENT_COMMS.contains(&p.comm.as_str()) {
        return true;
    }
    let first = p.cmd.split(' ').next().unwrap_or_default();
    AGENT_COMMS
        .iter()
        .any(|n| p.cmd == *n || first.ends_with(&format!("/{n}")))
}

/// Uma amostra da fila (`sampleOnce`, `gatewatch.ts:288-476`), com a CPU zerada por ser a primeira.
struct Amostra {
    t: f64,
    host: Map<String, Value>,
    agents: Vec<Map<String, Value>>,
    repos: Vec<Repo>,
    bypass: Vec<Value>,
    point: Value,
}

struct Repo {
    name: String,
    gates: Vec<Map<String, Value>>,
    locks: Vec<Value>,
    state: Value,
    recent: Vec<(f64, Value)>,
    journal: Vec<Vec<String>>,
}

fn amostrar(env: &PanelEnv, root: &Path, sonda: &Sonda) -> Amostra {
    let at = sonda.now;
    let proc_root = env.proc_root();
    let boot = match boot_epoch(proc_root) {
        0 => at.floor(),
        b => b as f64,
    };
    let table = scan_procs(proc_root);
    let mut scopes_seen: HashSet<String> = HashSet::new();
    let tree = |pid: i64| -> (i64, f64) {
        let rss = descendants(pid, &table.children)
            .iter()
            .map(|d| table.get(*d).map_or(0, |p| p.rss))
            .sum();
        (rss, 0.0)
    };
    let gate_usage = |pid: i64, seen: &mut HashSet<String>| -> (i64, f64) {
        if let Some(cg) = gate_scope(pid, &table, proc_root, &sonda.cgroup_root)
            && let (Some(mem), _) = cgroup_usage(&cg)
        {
            seen.insert(cg.to_string_lossy().into_owned());
            return (mem, 0.0);
        }
        tree(pid)
    };
    let age_of = |p: &Proc| at - (boot + p.start as f64 / CLK_TCK);

    let mem = meminfo(proc_root);
    let psi = pressure(proc_root);
    let load = load_average(proc_root);

    let mut agents: Vec<Map<String, Value>> = Vec::new();
    for p in table.iter().filter(|p| is_agent(p)) {
        if table
            .get(p.ppid)
            .is_some_and(|pp| AGENT_COMMS.contains(&pp.comm.as_str()))
        {
            continue;
        }
        let (rss, cpu) = tree(p.pid);
        let mut a = Map::new();
        a.insert("pid".into(), p.pid.into());
        a.insert("cwd".into(), p.cwd.clone().into());
        a.insert("age".into(), js_num(age_of(p)));
        a.insert("rss".into(), rss.into());
        a.insert("cpu".into(), js_num(cpu));
        agents.push(a);
    }

    let mut repos: Vec<Repo> = Vec::new();
    let mut wrappers: HashSet<i64> = HashSet::new();
    let (mut gate_rss, mut running, mut waiting, mut remote) = (0_i64, 0_i64, 0_i64, 0_i64);
    let all = list_repos(root);
    let named: Vec<String> = all.iter().filter(|r| *r != MACHINE).cloned().collect();
    for repo_name in &all {
        let repo_dir = root.join(repo_name);
        let locks_dir = repo_dir.join("locks");
        let lines = journal_lines(&repo_dir);
        let medians = step_durations(&lines);
        let mut owners: HashMap<i64, Owner> = HashMap::new();
        for s in readdir_sorted(&locks_dir)
            .into_iter()
            .filter(|n| SLOT_RE.is_match(n))
        {
            let f = fields(&locks_dir.join(&s).join("owner"));
            if let Some(pid) = digits(f.get("pid").map(String::as_str)) {
                owners.insert(
                    pid,
                    Owner {
                        f,
                        slot: s,
                        v1: true,
                    },
                );
            }
        }
        let queue_dir = locks_dir.join("gate-queue");
        for t in readdir_sorted(&queue_dir)
            .into_iter()
            .filter(|n| n.ends_with(".t"))
        {
            let f = fields(&queue_dir.join(&t));
            if let Some(pid) = digits(f.get("pid").map(String::as_str)) {
                owners.insert(
                    pid,
                    Owner {
                        f,
                        slot: t,
                        v1: false,
                    },
                );
            }
        }

        let mut gates: Vec<(bool, f64, Map<String, Value>)> = Vec::new();
        for p in table.iter() {
            let Some(m) = GATE_RE.captures(&p.cmd) else {
                continue;
            };
            if !SHELL_RE.is_match(p.cmd.split(' ').next().unwrap_or_default()) {
                continue;
            }
            if table.get(p.ppid).is_some_and(|pp| pp.cmd == p.cmd) {
                continue;
            }
            let own = owners.get(&p.pid);
            if !belongs(&p.cwd, repo_name, own, &named) {
                continue;
            }
            wrappers.insert(p.pid);
            let remote_host = remote_host_of(own.and_then(|o| o.get("state")));
            let state = if let Some(h) = &remote_host {
                format!("{REMOTE_PREFIX}{h}")
            } else if own.is_some_and(Owner::admitted) {
                "running".to_owned()
            } else if ancestors(p.ppid, &table)
                .iter()
                .any(|a| owners.get(a).is_some_and(Owner::admitted))
            {
                "lease".to_owned()
            } else {
                "waiting".to_owned()
            };
            let is_running = state == "running";
            let (rss, cpu) = if is_running {
                gate_usage(p.pid, &mut scopes_seen)
            } else {
                (0, 0.0)
            };
            let since = iso_utc_epoch(own.and_then(|o| o.get("since")));
            let (mut step, mut step_age, mut step_median, mut step_pct) =
                (Value::Null, Value::Null, Value::Null, Value::Null);
            if is_running && let Some((cmd, proc)) = current_step(p.pid, &table) {
                let age = age_of(proc);
                let median = medians.get(&normalize_cmd(&cmd)).copied();
                step_age = js_num(age);
                step_median = median.map_or(Value::Null, Value::from);
                if let Some(md) = median.filter(|m| *m != 0) {
                    step_pct = js_num(STEP_PCT_CAP.min((age / md as f64 * 100.0).round()));
                }
                step = cmd.into();
            }
            let label = LABEL_RE
                .captures(m.get(2).map_or("", |x| x.as_str()))
                .map_or("-".to_owned(), |c| c[1].to_owned());
            let age = age_of(p);
            let where_ =
                if remote_host.is_some() || own.and_then(|o| o.get("where")) == Some("remote") {
                    "remote"
                } else {
                    "local"
                };
            let host = remote_host
                .clone()
                .or_else(|| own.and_then(|o| o.get("remote")).map(str::to_owned));
            let mut g = Map::new();
            for (k, v) in [
                ("pid", Value::from(p.pid)),
                ("kind", m[1].into()),
                ("label", label.into()),
                ("state", state.clone().into()),
                ("clone", p.cwd.clone().into()),
                ("cmd", cut(&m[3], GATE_CMD_MAX).into()),
                ("age", js_num(age)),
                ("rss", rss.into()),
                ("cpu", js_num(cpu)),
                ("runningFor", js_opt(since.map(|s| at - s))),
                ("slot", own.map_or(Value::Null, |o| o.slot.clone().into())),
                ("step", step),
                ("stepAge", step_age),
                ("stepMedian", step_median),
                ("stepPct", step_pct),
                ("where", where_.into()),
                ("host", host.map_or(Value::Null, Value::from)),
            ] {
                g.insert(k.into(), v);
            }
            if is_running {
                gate_rss += rss;
                running += 1;
            } else if remote_host.is_some() {
                remote += 1;
            } else if state == "waiting" {
                waiting += 1;
            }
            gates.push((!is_running, age, g));
        }
        gates.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));

        let mut locks: Vec<Value> = Vec::new();
        for l in readdir_sorted(&locks_dir)
            .into_iter()
            .filter(|n| n.ends_with(".lock"))
        {
            let o = fields(&locks_dir.join(&l).join("owner"));
            let since = iso_utc_epoch(o.get("since").map(String::as_str));
            let clone = o.get("clone").cloned().unwrap_or_default();
            let present = !clone.is_empty()
                && agents.iter().any(|a| {
                    let cwd = a["cwd"].as_str().unwrap_or_default();
                    cwd == clone || cwd.starts_with(&format!("{clone}/"))
                });
            locks.push(json!({
                "name": &l[..l.len() - ".lock".len()], "mode": o.get("mode").cloned().unwrap_or_else(|| "-".into()),
                "clone": clone, "age": js_opt(since.map(|s| at - s)), "agentPresent": present,
            }));
        }
        repos.push(Repo {
            name: repo_name.clone(),
            gates: gates.into_iter().map(|g| g.2).collect(),
            locks,
            state: skill_state(&repo_dir, at),
            recent: recent_gates(&repo_dir, at),
            journal: journal_tail(&lines),
        });
    }

    let mut bypass: Vec<(i64, Value)> = Vec::new();
    for p in table.iter() {
        if !["node", "MainThread", "npm", "npx"].contains(&p.comm.as_str())
            && !p.cmd.starts_with("node")
        {
            continue;
        }
        if !HEAVY_RE.is_match(&p.cmd) || DAEMON_RE.is_match(&p.cmd) {
            continue;
        }
        let chain: Vec<i64> = ancestors(p.pid, &table).into_iter().skip(1).collect();
        let cmd_of = |a: &i64| table.get(*a).map_or("", |x| x.cmd.as_str());
        if chain
            .iter()
            .any(|a| wrappers.contains(a) || GATE_RE.is_match(cmd_of(a)))
        {
            continue;
        }
        if chain
            .iter()
            .any(|a| HEAVY_RE.is_match(cmd_of(a)) && !DAEMON_RE.is_match(cmd_of(a)))
        {
            continue;
        }
        let (rss, cpu) = gate_usage(p.pid, &mut scopes_seen);
        if rss < BYPASS_MIN_RSS && cpu < BYPASS_MIN_CPU {
            continue;
        }
        bypass.push((
            rss,
            json!({ "pid": p.pid, "cwd": p.cwd, "cmd": cut(&p.cmd, GATE_CMD_MAX), "age": js_num(age_of(p)), "rss": rss, "cpu": js_num(cpu) }),
        ));
    }
    bypass.sort_by_key(|b| std::cmp::Reverse(b.0));

    for a in &mut agents {
        let cwd = a["cwd"].as_str().unwrap_or_default().to_owned();
        let gates: Vec<Value> = repos
            .iter()
            .flat_map(|r| r.gates.iter())
            .filter(|g| {
                let clone = g["clone"].as_str().unwrap_or_default();
                clone == cwd || clone.starts_with(&format!("{cwd}/"))
            })
            .map(|g| format!("{}:{}/{}", s(&g["state"]), s(&g["kind"]), s(&g["label"])).into())
            .collect();
        let locks: Vec<Value> = repos
            .iter()
            .flat_map(|r| r.locks.iter())
            .filter(|l| {
                let clone = l["clone"].as_str().unwrap_or_default();
                !clone.is_empty() && clone == cwd
            })
            .map(|l| l["name"].clone())
            .collect();
        a.insert("gates".into(), gates.into());
        a.insert("locks".into(), locks.into());
    }
    agents.sort_by(|a, b| s(&a["cwd"]).cmp(s(&b["cwd"])));

    let leftover_rss: i64 = procs::all_gate_scopes(&sonda.cgroup_root, sonda.uid)
        .into_iter()
        .filter(|(k, _)| !scopes_seen.contains(k))
        .map(|(_, rss)| rss)
        .sum();
    let (infra_rss, _) = sonda.infra.usage(&table);
    let m = |k: &str| mem.get(k).copied().unwrap_or(0);
    let swap_used = m("SwapTotal") - m("SwapFree");
    let mut host = Map::new();
    for (k, v) in [
        ("memTotal", Value::from(m("MemTotal"))),
        ("memAvailable", Value::from(m("MemAvailable"))),
        ("leftoverRss", Value::from(leftover_rss)),
        ("infraRss", Value::from(infra_rss)),
        ("swapUsed", Value::from(swap_used)),
        ("psiSome", js_num(psi.get("some").copied().unwrap_or(0.0))),
        (
            "load",
            load.iter().map(|x| js_opt(*x)).collect::<Vec<_>>().into(),
        ),
    ] {
        host.insert(k.into(), v);
    }
    let point = json!({
        "t": js_num(at), "running": running, "waiting": waiting, "remote": remote, "gateRss": gate_rss,
        "avail": m("MemAvailable"),
    });
    Amostra {
        t: at,
        host,
        agents,
        repos,
        bypass: bypass.into_iter().map(|b| b.1).collect(),
        point,
    }
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

/// `repos` (`gatewatch.ts:286`): pastas de `root` com `__` no nome, mais a da máquina.
fn list_repos(root: &Path) -> Vec<String> {
    readdir_sorted(root)
        .into_iter()
        .filter(|n| root.join(n).is_dir() && (n.contains("__") || n == MACHINE))
        .collect()
}

/// `budget` (`gatewatch.ts:489-496`): orçamento e reserva lidos do gate-lock.sh.
fn budget(gate_lock: Option<&str>) -> Value {
    let src = gate_lock
        .and_then(|p| procs::read_text(Path::new(p)))
        .unwrap_or_default();
    let num = |name: &str| -> Value {
        let a = Regex::new(&format!(r"(?m)^{name}=\$\{{{name}:-(\d+)\}}")).unwrap();
        let b = Regex::new(&format!(r"{name}=\$\{{[A-Z_]+:-(\d+)\}}")).unwrap();
        a.captures(&src)
            .or_else(|| b.captures(&src))
            .and_then(|c| c[1].parse::<i64>().ok())
            .map_or(Value::Null, Value::from)
    };
    json!({ "budgetGb": num("BUDGET_GB"), "reserveGb": num("RESERVE_GB") })
}

/// `setting` (`gatewatch.ts:154`): ambiente, senão `config.env` de `$D`.
fn setting(env: &PanelEnv, key: &str) -> Option<String> {
    env.env()
        .get_raw(key)
        .map(str::to_owned)
        .or_else(|| read_config_env(env.data()).get(key).map(str::to_owned))
}

/// `view` do `createWorkerWatch` (`gatewatch-proc.ts:474-477`) antes da primeira consulta: o dump não espera a rede.
fn workers(spec: Option<&str>) -> Vec<Value> {
    let mut seen: Vec<&str> = Vec::new();
    for raw in spec
        .unwrap_or_default()
        .split(|c: char| c.is_whitespace() || c == ';')
    {
        let target = raw.split(':').next().unwrap_or_default();
        if !target.is_empty() && !seen.contains(&target) {
            seen.push(target);
        }
    }
    seen.into_iter()
        .map(|t| json!({ "target": t, "reachable": false, "stale": false, "checkedAt": null, "okAt": null, "error": "ainda não consultado", "status": null }))
        .collect()
}

/// `detail` (`gatewatch.ts:498-527`) da rota `gates` (`panel/dump.ts:51-55`).
pub fn fila(env: &PanelEnv) -> Value {
    fila_com(env, &Sonda::do_sistema(env))
}

pub fn fila_com(env: &PanelEnv, sonda: &Sonda) -> Value {
    let Some(locks) = locks_dir(env) else {
        return json!({ "available": false, "gates": gate_queue(None) });
    };
    let root = parent2(&locks);
    let a = amostrar(env, &root, sonda);
    let locks_text = locks.to_string_lossy();
    let named: Vec<&Repo> = a.repos.iter().filter(|r| r.name != MACHINE).collect();
    let repo = named
        .iter()
        .find(|r| locks_text.contains(&r.name))
        .or_else(|| named.first())
        .copied();
    let mach = a.repos.iter().find(|r| r.name == MACHINE);

    let mut order: Vec<i64> = Vec::new();
    let mut gates: HashMap<i64, Map<String, Value>> = HashMap::new();
    for g in repo.map(|r| r.gates.as_slice()).unwrap_or_default() {
        let pid = g["pid"].as_i64().unwrap_or_default();
        if gates.insert(pid, g.clone()).is_none() {
            order.push(pid);
        }
    }
    for g in mach.map(|r| r.gates.as_slice()).unwrap_or_default() {
        let pid = g["pid"].as_i64().unwrap_or_default();
        if !gates.contains_key(&pid) || !g["slot"].is_null() {
            let mut g = g.clone();
            g.insert("machine".into(), true.into());
            if gates.insert(pid, g).is_none() {
                order.push(pid);
            }
        }
    }
    let mut merged: Vec<Map<String, Value>> = order
        .iter()
        .map(|p| gates.remove(p).unwrap_or_default())
        .collect();
    merged.sort_by(|a, b| {
        let r = |g: &Map<String, Value>| g["state"] != "running";
        let age = |g: &Map<String, Value>| g["age"].as_f64().unwrap_or(0.0);
        r(a).cmp(&r(b)).then(age(b).total_cmp(&age(a)))
    });

    let both = [repo, mach];
    let mut journal: Vec<Vec<String>> = both
        .iter()
        .flatten()
        .flat_map(|r| r.journal.clone())
        .collect();
    journal.sort_by(|x, y| x.first().cmp(&y.first()));
    let from = journal.len().saturating_sub(DETAIL_JOURNAL);
    let mut recent: Vec<(f64, Value)> = both
        .iter()
        .flatten()
        .flat_map(|r| r.recent.clone())
        .collect();
    recent.sort_by(|x, y| y.0.total_cmp(&x.0));
    let locks_out: Vec<Value> = both
        .iter()
        .flatten()
        .flat_map(|r| r.locks.clone())
        .collect();
    let repo_name = repo.map_or_else(
        || {
            if mach.is_some() {
                Value::from(MACHINE)
            } else {
                Value::Null
            }
        },
        |r| r.name.clone().into(),
    );
    let gate_lock = read_env_sh(env.data()).get("GATE_LOCK").map(str::to_owned);

    json!({
        "available": true,
        "t": js_num(a.t),
        "repo": repo_name,
        "machine": mach.is_some(),
        "gates": merged,
        "locks": locks_out,
        "recent": recent.into_iter().map(|r| r.1).collect::<Vec<_>>(),
        "journal": journal[from..].to_vec(),
        "bypass": a.bypass,
        "loop": repo.map_or_else(|| json!({}), |r| r.state.clone()),
        "agents": a.agents,
        "host": a.host,
        "history": [a.point],
        "budget": budget(gate_lock.as_deref()),
        "remote": {
            "where": setting(env, "HADES_GATE_WHERE").unwrap_or_else(|| "local".into()),
            "autoAfterS": setting(env, "HADES_GATE_AUTO_AFTER_S"),
            "workers": workers(setting(env, "HADES_GATE_WORKERS").as_deref()),
        },
    })
}

#[cfg(test)]
mod tests;
