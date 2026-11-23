//! Leitura do `/proc` e do cgroup para a fila de gates (`panel/gatewatch-proc.ts`, `panel/system.ts`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use std::sync::LazyLock;

const PAGE_BYTES: i64 = 4096;
const LOAD_SAMPLES: usize = 3;

/// `HEAVY_RE` (`gatewatch-proc.ts:27`).
pub static HEAVY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(/nx/bin/nx\.js|\bnx\s+(run|test|lint|build|affected|run-many)\b|jest|webpack|eslint|/tsc\b|ngc)")
        .unwrap()
});
/// `DAEMON_RE` (`gatewatch-proc.ts:28`).
pub static DAEMON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"nx/src/daemon|daemon/server|plugin-worker|language-server|languageserver|tsserver|--stdio|lsp").unwrap()
});
/// `SCOPE_RE` (`gatewatch-proc.ts:19`).
static SCOPE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"run-p\d+-i\d+\.scope$").unwrap());
static SCOPE_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^run-p.*\.scope$").unwrap());

/// `Proc` (`gatewatch-proc.ts:30-41`): `cpu` é utime + stime em ticks, `start` em ticks desde o boot.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: i64,
    pub ppid: i64,
    pub comm: String,
    pub cmd: String,
    pub cpu: i64,
    pub start: i64,
    pub rss: i64,
    pub cwd: String,
}

/// `ProcTable` (`gatewatch-proc.ts:43-46`): a ordem de `order` é a do `readdir` do `/proc` (a do `Map` do TS).
#[derive(Clone, Debug, Default)]
pub struct ProcTable {
    pub order: Vec<i64>,
    pub procs: HashMap<i64, Proc>,
    pub children: HashMap<i64, Vec<i64>>,
}

impl ProcTable {
    pub fn get(&self, pid: i64) -> Option<&Proc> {
        self.procs.get(&pid)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Proc> {
        self.order.iter().filter_map(|pid| self.procs.get(pid))
    }

    pub fn from_procs(list: Vec<Proc>) -> Self {
        let mut table = Self::default();
        for p in list {
            if !table.procs.contains_key(&p.pid) {
                table.order.push(p.pid);
            }
            table.procs.insert(p.pid, p);
        }
        for pid in table.order.clone() {
            let ppid = table.procs[&pid].ppid;
            table.children.entry(ppid).or_default().push(pid);
        }
        table
    }
}

/// `Number.parseInt(text, 10)`: sinal opcional e os dígitos do começo; sem dígito é `None` (NaN).
pub fn parse_int(text: &str) -> Option<i64> {
    let t = text.trim_start();
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: i64 = digits.parse().ok()?;
    Some(if neg { -n } else { n })
}

pub fn read_text(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `readLines` (`panel/system.ts`): arquivo ausente é lista vazia.
pub fn read_lines(path: &Path) -> Vec<String> {
    read_text(path)
        .map(|t| t.split('\n').map(str::to_owned).collect())
        .unwrap_or_default()
}

/// `readdirSync` do Node: a ordem do libuv é a do `alphasort`, por bytes no locale C.
pub fn readdir_sorted(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// `readProc` (`gatewatch-proc.ts:58-84`).
pub fn read_proc(proc_root: &Path, pid: i64) -> Option<Proc> {
    let base = proc_root.join(pid.to_string());
    let stat = read_text(&base.join("stat")).filter(|s| !s.is_empty())?;
    let rp = stat.rfind(')')?;
    let open = stat.find('(')?;
    let tail = stat.get(rp + 2..).unwrap_or_default();
    let f: Vec<&str> = tail.split(' ').collect();
    let field = |i: usize| f.get(i).copied().and_then(parse_int);
    let ppid = field(1)?;
    let cpu = field(11)? + field(12)?;
    let start = field(19)?;
    let cmd = fs::read(base.join("cmdline"))
        .map(|b| {
            String::from_utf8_lossy(&b)
                .replace('\0', " ")
                .trim()
                .to_owned()
        })
        .unwrap_or_default();
    let resident = read_text(&base.join("statm"))
        .filter(|s| !s.is_empty())
        .map(|s| s.split(' ').nth(1).and_then(parse_int).unwrap_or(0))
        .unwrap_or(0);
    let cwd = fs::read_link(base.join("cwd"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(Proc {
        pid,
        ppid,
        comm: stat.get(open + 1..rp).unwrap_or_default().to_owned(),
        cmd,
        cpu,
        start,
        rss: resident * PAGE_BYTES,
        cwd,
    })
}

/// `scanProcs` (`gatewatch-proc.ts:152-162`), só Linux.
pub fn scan_procs(proc_root: &Path) -> ProcTable {
    let list = readdir_sorted(proc_root)
        .into_iter()
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|n| n.parse().ok().and_then(|pid| read_proc(proc_root, pid)))
        .collect();
    ProcTable::from_procs(list)
}

/// `descendants` (`gatewatch-proc.ts:164-172`): o próprio pid e a árvore abaixo, em pilha.
pub fn descendants(pid: i64, children: &HashMap<i64, Vec<i64>>) -> Vec<i64> {
    let mut out = Vec::new();
    let mut stack = vec![pid];
    while let Some(cur) = stack.pop() {
        out.push(cur);
        stack.extend(children.get(&cur).into_iter().flatten().copied());
    }
    out
}

/// `ancestors` (`gatewatch-proc.ts:174-182`): do `start` para cima, parando no pid 1 ou num ciclo.
pub fn ancestors(start: i64, table: &ProcTable) -> Vec<i64> {
    let mut seen = Vec::new();
    let mut pid = start;
    while let Some(p) = table.get(pid) {
        if seen.contains(&pid) || pid <= 1 {
            break;
        }
        seen.push(pid);
        pid = p.ppid;
    }
    seen
}

/// `gateScope` (`gatewatch-proc.ts:185-198`): cgroup do scope `run-*.scope` em que o gate roda.
pub fn gate_scope(
    pid: i64,
    table: &ProcTable,
    proc_root: &Path,
    cgroup_root: &Path,
) -> Option<PathBuf> {
    for d in descendants(pid, &table.children) {
        for line in read_lines(&proc_root.join(d.to_string()).join("cgroup")) {
            let path = line.rsplit(':').next().unwrap_or_default();
            if SCOPE_RE.is_match(path) {
                let cg = cgroup_root.join(path.trim_start_matches('/'));
                return cg.is_dir().then_some(cg);
            }
        }
    }
    None
}

/// `digitsOf` (`gatewatch-proc.ts:208`).
fn digits_of(text: Option<String>) -> Option<i64> {
    let t = text?;
    let t = t.trim();
    (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
        .then(|| t.parse().ok())
        .flatten()
}

/// `cgroupUsage` (`gatewatch-proc.ts:237-247`): bytes em uso e segundos de CPU acumulados.
pub fn cgroup_usage(cg: &Path) -> (Option<i64>, Option<f64>) {
    let mem = digits_of(read_text(&cg.join("memory.current")));
    let cpu = read_lines(&cg.join("cpu.stat"))
        .iter()
        .find(|l| l.starts_with("usage_usec"))
        .map(|l| l.split_whitespace().nth(1).and_then(parse_int).unwrap_or(0) as f64 / 1e6);
    (mem, cpu)
}

/// `allGateScopes` (`gatewatch-proc.ts:211-234`): todo `run-p*.scope` vivo sob a sessão do usuário.
pub fn all_gate_scopes(cgroup_root: &Path, uid: u32) -> Vec<(String, i64)> {
    let base = cgroup_root
        .join("user.slice")
        .join(format!("user-{uid}.slice"))
        .join(format!("user@{uid}.service"));
    let mut out = Vec::new();
    if base.is_dir() {
        walk_scopes(&base, &mut out);
    }
    out
}

fn walk_scopes(dir: &Path, out: &mut Vec<(String, i64)>) {
    for name in readdir_sorted(dir) {
        let path = dir.join(&name);
        if !path.is_dir() || path.is_symlink() {
            continue;
        }
        if SCOPE_NAME_RE.is_match(&name)
            && let Some(mem) = digits_of(read_text(&path.join("memory.current")))
        {
            out.push((path.to_string_lossy().into_owned(), mem));
        }
        walk_scopes(&path, out);
    }
}

fn has_proc(proc_root: &Path) -> bool {
    proc_root.join("self").exists()
}

/// `meminfo` (`panel/system.ts:113-124`): bytes por chave; sem `/proc` (o macOS) fica vazio.
pub fn meminfo(proc_root: &Path) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    if !has_proc(proc_root) {
        return out;
    }
    for line in read_lines(&proc_root.join("meminfo")) {
        let Some(colon) = line.find(':') else {
            continue;
        };
        let value = line[colon + 1..].trim();
        let n = if value.is_empty() {
            0
        } else {
            value
                .split_whitespace()
                .next()
                .and_then(parse_int)
                .unwrap_or(0)
                * 1024
        };
        out.insert(line[..colon].to_owned(), n);
    }
    out
}

/// `pressure` (`gatewatch-proc.ts:249-257`): o `avg10` de cada linha de `/proc/pressure/memory`.
pub fn pressure(proc_root: &Path) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for line in read_lines(&proc_root.join("pressure").join("memory")) {
        let mut it = line.split_whitespace();
        let Some(kind) = it.next() else { continue };
        let avg = it
            .map(|x| x.split('=').collect::<Vec<_>>())
            .find(|pair| pair.first() == Some(&"avg10"))
            .map(|pair| parse_float(pair.get(1).copied().unwrap_or("0")).unwrap_or(f64::NAN));
        out.insert(kind.to_owned(), avg.unwrap_or(0.0));
    }
    out
}

/// `Number.parseFloat`: o prefixo numérico.
pub fn parse_float(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let end = t
        .char_indices()
        .take_while(|(i, c)| {
            c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+'))
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    t[..end].parse().ok()
}

/// `loadAverage` (`panel/system.ts:103-106`): `Number` de cada campo; o que não é número vira `None` (o `null` do
/// `JSON.stringify` de `NaN`).
pub fn load_average(proc_root: &Path) -> Vec<Option<f64>> {
    if !has_proc(proc_root) {
        return Vec::new();
    }
    let text = read_text(&proc_root.join("loadavg")).unwrap_or_else(|| "0 0 0".to_owned());
    text.split(|c: char| c.is_whitespace())
        .take(LOAD_SAMPLES)
        .map(|s| {
            if s.is_empty() {
                Some(0.0)
            } else {
                s.parse().ok()
            }
        })
        .collect()
}

/// `bootEpoch` (`panel/system.ts:58-61`): `btime` do `/proc/stat`, 0 se não há.
pub fn boot_epoch(proc_root: &Path) -> i64 {
    if !has_proc(proc_root) {
        return 0;
    }
    read_lines(&proc_root.join("stat"))
        .iter()
        .find(|l| l.starts_with("btime"))
        .and_then(|l| l.split_whitespace().nth(1).and_then(parse_int))
        .unwrap_or(0)
}

/// `fields` (`panel/system.ts:33-40`): `chave=valor` por linha, aparados.
pub fn fields(path: &Path) -> HashMap<String, String> {
    read_lines(path)
        .into_iter()
        .filter_map(|line| {
            let eq = line.find('=')?;
            Some((
                line[..eq].trim().to_owned(),
                line[eq + 1..].trim().to_owned(),
            ))
        })
        .collect()
}
