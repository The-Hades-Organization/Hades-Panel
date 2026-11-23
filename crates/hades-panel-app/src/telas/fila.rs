//! Tela Fila de gates (`pages/queue.tsx`): quem espera, quem roda e o que acabou; a memória da máquina, a última
//! hora, o journal e o que roda fora da fila.

use std::collections::VecDeque;

use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px, relative,
};
use hades_ui::components::{
    AreaChart, Card, ChartSeries, Dot, EmptyState, Fio, FioState, PageHeader, Segmented,
    StatusBadge,
};
use hades_ui::tokens::size;
use hades_ui::{Palette, Tone, palette};
use serde_json::Value;

use super::loop_::AoAbrir;
use super::ui::{Detalhe, linha_de_estado, texto_meta};
use super::{GB, arr, cauda, decimal_pt, dur, gb, mb, num, plural, sim, texto};

/// `DONE_SHOWN` de `queue.tsx:9`.
const CONCLUIDOS: usize = 4;
/// Linhas do journal na tela (`Mostrar 15` de `queue.tsx:226`).
const JOURNAL: usize = 15;
const JANELA_MS: i64 = 60 * 60 * 1000;

/// `kv` de `queue.tsx:7`: os campos `k=v` a partir do sétimo.
pub fn kv(row: &[Value], chave: &str) -> Option<String> {
    row.iter().skip(6).find_map(|x| {
        let x = x.as_str()?;
        let (k, v) = x.split_once('=')?;
        (k == chave).then(|| v.to_owned())
    })
}

/// `exitText` de `queue.tsx:12`: 137 e 143 são o teto de memória, não o teste.
pub fn texto_saida(rc: &str) -> String {
    if rc == "137" || rc == "143" {
        format!("estourou a memória ({rc})")
    } else {
        format!("falhou (saída {rc})")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Estado {
    Esperando,
    Rodando,
    Remoto(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gate {
    pub pid: String,
    pub label: String,
    pub meta: String,
    pub estado: Estado,
    pub tempo: String,
    pub cmd: String,
    pub memoria: String,
    pub cpu: String,
    pub passo: Option<String>,
    pub passo_tempo: String,
    pub passo_pct: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Concluido {
    pub label: String,
    pub quando: String,
    pub resumo: String,
    pub ruim: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Evento {
    pub hora: String,
    pub evento: String,
    pub tom: Tone,
    pub gate: String,
    pub onde: String,
    pub reserva: String,
    pub resultado: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segmento {
    pub rotulo: &'static str,
    pub bytes: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilaVm {
    pub disponivel: bool,
    pub principal: String,
    pub resto: Vec<String>,
    pub abaixo_do_minimo: bool,
    pub esperando: Vec<Gate>,
    pub rodando: Vec<Gate>,
    pub remotos: Vec<Gate>,
    pub concluidos: Vec<Concluido>,
    pub memoria_resumo: String,
    pub segmentos: Vec<Segmento>,
    pub mem_total: f64,
    pub reserva: f64,
    pub journal: Vec<Evento>,
    pub fora: Vec<String>,
    pub workers: Vec<(String, bool, String)>,
}

fn host_remoto(state: &str) -> Option<String> {
    state.strip_prefix("remote:").map(str::to_owned)
}

/// `where` de `queue.tsx:20`.
fn onde(g: &Value) -> String {
    let state = texto(&g["state"]);
    if state.starts_with("remote:") {
        state
    } else if texto(&g["where"]) == "remote" && !texto(&g["host"]).is_empty() {
        format!("remote:{}", texto(&g["host"]))
    } else {
        "local".into()
    }
}

/// `gateMeta` de `queue.tsx:32`.
fn meta_gate(g: &Value) -> String {
    let label = texto(&g["label"]);
    [
        Some(texto(&g["kind"])).filter(|s| !s.is_empty()),
        Some(cauda(&texto(&g["clone"]))).filter(|s| !s.is_empty()),
        label.starts_with("hades-").then(|| "Hades".to_owned()),
        sim(&g["machine"]).then(|| "fila da máquina".to_owned()),
        Some(onde(g)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

fn gate(g: &Value) -> Gate {
    let state = texto(&g["state"]);
    let estado = match host_remoto(&state) {
        Some(h) => Estado::Remoto(h),
        None if state == "waiting" => Estado::Esperando,
        None => Estado::Rodando,
    };
    let rodando = estado == Estado::Rodando;
    let mediana = num(&g["stepMedian"]).filter(|m| *m > 0.0);
    Gate {
        pid: texto(&g["pid"]),
        label: texto(&g["label"]),
        meta: meta_gate(g),
        estado,
        tempo: dur(num(if rodando { &g["runningFor"] } else { &g["age"] })),
        cmd: texto(&g["cmd"]),
        memoria: if rodando {
            mb(num(&g["rss"]))
        } else {
            "—".into()
        },
        cpu: if rodando {
            format!("{}%", num(&g["cpu"]).unwrap_or(0.0).round())
        } else {
            "—".into()
        },
        passo: Some(texto(&g["step"])).filter(|s| !s.is_empty()),
        passo_tempo: format!(
            "{}{}",
            dur(num(&g["stepAge"])),
            mediana.map_or(String::new(), |m| format!(" de ~{}", dur(Some(m))))
        ),
        passo_pct: num(&g["stepPct"]).map(|v| v as f32),
    }
}

/// Hora local `HH:MM:SS` de um carimbo do journal (ISO ou já hora).
fn hora(ts: &str) -> String {
    ts.split_once('T')
        .map_or(ts, |(_, h)| h)
        .chars()
        .take(8)
        .collect()
}

/// `doneGates` de `queue.tsx:24`, sem o ponto de corte do fio.
fn concluidos(journal: &[Value]) -> Vec<Concluido> {
    let done: Vec<&[Value]> = journal
        .iter()
        .filter_map(Value::as_array)
        .filter(|r| r.get(1).and_then(Value::as_str) == Some("done"))
        .map(Vec::as_slice)
        .collect();
    done.iter()
        .rev()
        .take(CONCLUIDOS)
        .map(|r| {
            let rc = kv(r, "rc").unwrap_or_default();
            let ruim = rc != "0";
            let host = kv(r, "host");
            let perdido = r
                .iter()
                .any(|f| f.as_str().is_some_and(|s| s.contains("remoto perdido")));
            let resultado = if !ruim {
                "ok".to_owned()
            } else if perdido {
                format!("remoto perdido ({rc})")
            } else {
                texto_saida(&rc)
            };
            let partes = [
                Some(texto(&r[2])),
                Some(host.map_or("local".into(), |h| format!("remote:{h}"))),
                Some(resultado),
                kv(r, "dur")
                    .and_then(|d| d.parse().ok())
                    .map(|d| dur(Some(d))),
            ];
            Concluido {
                label: texto(&r[3]),
                quando: hora(&texto(&r[0])),
                resumo: partes.into_iter().flatten().collect::<Vec<_>>().join(" · "),
                ruim,
            }
        })
        .collect()
}

/// `EV` de `queue.tsx:217`, com o `done` de rc diferente de zero como falha.
fn evento(r: &[Value]) -> Evento {
    let ev = texto(&r[1]);
    let rc = kv(r, "rc");
    let ruim = ev == "done" && rc.as_deref().is_some_and(|c| c != "0");
    let (rotulo, tom) = if ruim {
        ("falhou", Tone::Bad)
    } else {
        match ev.as_str() {
            "enqueue" => ("entrou", Tone::Neutral),
            "admit" => ("admitido", Tone::Neutral),
            "start" => ("começou", Tone::Neutral),
            "done" => ("concluído", Tone::Neutral),
            "timeout" => ("estourou o tempo", Tone::Warn),
            "abort" => ("abortado", Tone::Bad),
            _ => (ev.as_str(), Tone::Neutral),
        }
    };
    let resultado = if ev == "done" {
        let rc = rc.unwrap_or_default();
        format!(
            "{} · {}",
            if ruim { texto_saida(&rc) } else { "ok".into() },
            kv(r, "dur").unwrap_or_default()
        )
    } else {
        kv(r, "waited").map_or(String::new(), |w| format!("esperou {w}"))
    };
    Evento {
        hora: hora(&texto(&r[0])),
        evento: rotulo.to_owned(),
        tom,
        gate: texto(&r[3]),
        onde: format!(
            "{} · {}{}",
            texto(&r[2]),
            cauda(&texto(&r[5])),
            kv(r, "host").map_or(String::new(), |h| format!(" · remote:{h}"))
        ),
        reserva: texto(&r[4]),
        resultado,
    }
}

impl FilaVm {
    /// `Queue()` de `queue.tsx:466`.
    pub fn de(x: &Value) -> Self {
        let gates = arr(&x["gates"]);
        if !sim(&x["available"]) {
            return Self {
                esperando: gates.iter().map(gate).collect(),
                ..Self::default()
            };
        }
        let todos: Vec<Gate> = gates.iter().map(gate).collect();
        let por = |e: fn(&Estado) -> bool| -> Vec<Gate> {
            todos.iter().filter(|g| e(&g.estado)).cloned().collect()
        };
        let rodando = por(|e| *e == Estado::Rodando);
        let esperando = por(|e| *e == Estado::Esperando);
        let remotos = por(|e| matches!(e, Estado::Remoto(_)));
        let mem_gates: f64 = gates
            .iter()
            .filter(|g| {
                let s = texto(&g["state"]);
                s != "waiting" && !s.starts_with("remote:")
            })
            .filter_map(|g| num(&g["rss"]))
            .sum();
        let h = &x["host"];
        let livre = num(&h["memAvailable"]).unwrap_or(0.0);
        let total = num(&h["memTotal"]).unwrap_or(1.0).max(1.0);
        let reserva_gb = num(&x["budget"]["reserveGb"])
            .filter(|v| *v != 0.0)
            .unwrap_or(2.0);
        let abaixo = livre < (reserva_gb + 2.0) * GB;
        let mais_antigo = gates
            .iter()
            .filter(|g| texto(&g["state"]) == "waiting")
            .filter_map(|g| num(&g["age"]))
            .fold(0.0, f64::max);
        let mut resto = vec![if esperando.is_empty() {
            "ninguém na fila".to_owned()
        } else {
            format!(
                "{} esperando, o mais antigo há {}",
                esperando.len(),
                dur(Some(mais_antigo))
            )
        }];
        if !remotos.is_empty() {
            resto.push(format!(
                "{} em worker remoto (fora da conta de memória)",
                remotos.len()
            ));
        }
        resto.push(format!(
            "gates usam {}{}",
            gb(Some(mem_gates)),
            num(&x["budget"]["budgetGb"])
                .filter(|b| *b != 0.0)
                .map_or(String::new(), |b| format!(" de {b} GB de orçamento"))
        ));
        resto.push(format!(
            "livre {} · reserva {reserva_gb} GB{}",
            gb(Some(livre)),
            if abaixo {
                " · abaixo do mínimo para admitir gates"
            } else {
                ""
            }
        ));
        resto.push(format!(
            "carga {} (média de 1 min)",
            decimal_pt(arr(&h["load"]).first().and_then(num).unwrap_or(0.0))
        ));
        let infra = num(&h["infraRss"]).unwrap_or(0.0);
        let sobra = num(&h["leftoverRss"]).unwrap_or(0.0);
        let usado = (total - livre).max(0.0);
        let outros = (usado - mem_gates - infra - sobra).max(0.0);
        let journal = arr(&x["journal"]);
        let swap = num(&h["swapUsed"]).filter(|s| *s > 0.0);
        Self {
            disponivel: true,
            principal: if rodando.is_empty() {
                "nada rodando".into()
            } else {
                format!("{} rodando", rodando.len())
            },
            resto,
            abaixo_do_minimo: abaixo,
            esperando,
            rodando,
            remotos,
            concluidos: concluidos(journal),
            memoria_resumo: format!(
                "livre {} de {}{}",
                gb(Some(livre)),
                gb(Some(total)),
                swap.map_or(String::new(), |s| format!(" · swap {}", gb(Some(s))))
            ),
            segmentos: vec![
                Segmento {
                    rotulo: "Gates",
                    bytes: mem_gates,
                },
                Segmento {
                    rotulo: "Infra (containers, daemons)",
                    bytes: infra,
                },
                Segmento {
                    rotulo: "Sobra de gate",
                    bytes: sobra,
                },
                Segmento {
                    rotulo: "Outros processos",
                    bytes: outros,
                },
            ],
            mem_total: total,
            reserva: reserva_gb * GB,
            journal: journal
                .iter()
                .rev()
                .filter_map(Value::as_array)
                .filter(|r| r.len() >= 6)
                .take(JOURNAL)
                .map(|r| evento(r))
                .collect(),
            fora: arr(&x["bypass"])
                .iter()
                .map(|p| {
                    format!(
                        "pid {} · {} · {}% · {}",
                        texto(&p["pid"]),
                        mb(num(&p["rss"])),
                        num(&p["cpu"]).unwrap_or(0.0).round(),
                        texto(&p["cmd"])
                    )
                })
                .collect(),
            workers: arr(&x["remote"]["workers"])
                .iter()
                .map(|w| {
                    let ok = sim(&w["reachable"]);
                    let resumo = if ok {
                        let st = &w["status"];
                        format!(
                            "memória livre {} de {} GB · fila {}",
                            decimal_pt(num(&st["memFreeGb"]).unwrap_or(0.0)),
                            decimal_pt(num(&st["memTotalGb"]).unwrap_or(0.0)),
                            texto(&st["queue"])
                        )
                    } else {
                        Some(texto(&w["error"]))
                            .filter(|e| !e.is_empty())
                            .unwrap_or_else(|| "sem resposta".into())
                    };
                    (texto(&w["target"]), ok, resumo)
                })
                .collect(),
        }
    }

    /// Todos os gates na ordem da Lista: rodando, remotos, esperando.
    pub fn lista(&self) -> impl Iterator<Item = &Gate> {
        self.rodando
            .iter()
            .chain(&self.remotos)
            .chain(&self.esperando)
    }

    pub fn detalhe(&self, pid: &str) -> Option<Detalhe> {
        let g = self.lista().find(|g| g.pid == pid)?;
        let estado = match &g.estado {
            Estado::Esperando => format!("na fila há {}", g.tempo),
            Estado::Rodando => format!("rodando há {}", g.tempo),
            Estado::Remoto(h) => format!("rodando em {h} há {}", g.tempo),
        };
        Some(
            Detalhe::new(g.label.clone(), g.meta.clone())
                .linha("Estado", estado)
                .linha("Comando", g.cmd.clone())
                .linha("Memória", g.memoria.clone())
                .linha("CPU", g.cpu.clone())
                .linha("Passo atual", g.passo.clone().unwrap_or_default())
                .linha(
                    "Tempo do passo",
                    if g.passo.is_some() {
                        g.passo_tempo.clone()
                    } else {
                        String::new()
                    },
                )
                .linha("pid", g.pid.clone()),
        )
    }
}

/// Um ponto da última hora da fila (`history` do web, acumulado pelo app a cada amostra).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PontoFila {
    pub t_ms: i64,
    pub rodando: f64,
    pub esperando: f64,
    pub gates_gb: f64,
    pub livre_gb: f64,
}

#[derive(Clone, Debug, Default)]
pub struct HistoricoFila {
    pontos: VecDeque<PontoFila>,
}

impl HistoricoFila {
    pub fn registrar(&mut self, t_ms: i64, fila: &Value) {
        if !sim(&fila["available"]) {
            return;
        }
        let vm = FilaVm::de(fila);
        let gates_gb = vm.segmentos.first().map_or(0.0, |s| s.bytes) / GB;
        self.pontos.push_back(PontoFila {
            t_ms,
            rodando: vm.rodando.len() as f64,
            esperando: vm.esperando.len() as f64,
            gates_gb,
            livre_gb: num(&fila["host"]["memAvailable"]).unwrap_or(0.0) / GB,
        });
        while self
            .pontos
            .front()
            .is_some_and(|p| t_ms - p.t_ms > JANELA_MS)
        {
            self.pontos.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.pontos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pontos.is_empty()
    }

    pub fn serie(&self, f: impl Fn(&PontoFila) -> f64) -> Vec<f64> {
        self.pontos.iter().map(f).collect()
    }
}

/// Visão da fila agora: o rio de raias ou a lista.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visao {
    #[default]
    Rio,
    Lista,
}

pub type AoTrocarVisao = std::rc::Rc<dyn Fn(Visao, &mut App)>;

pub struct Props<'a> {
    pub vm: &'a FilaVm,
    pub historico: &'a HistoricoFila,
    pub visao: Visao,
    pub descricao: &'a str,
    pub abrir: AoAbrir,
    pub trocar_visao: AoTrocarVisao,
}

pub fn tela(pr: Props<'_>, cx: &App) -> gpui::AnyElement {
    let p = palette(cx);
    let vm = pr.vm;
    let cabeca = PageHeader::new("Fila de gates").description(pr.descricao.to_owned());
    let base = div()
        .id("tela-fila")
        .flex_1()
        .min_w_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        .child(cabeca);
    if !vm.disponivel {
        let card = Card::new()
            .title("Fila do gate-lock")
            .meta("Amostrador da fila indisponível; mostrando os tickets do gate-lock");
        let card = if vm.esperando.is_empty() {
            card.child(EmptyState::new("Fila vazia"))
        } else {
            card.children(
                vm.esperando
                    .iter()
                    .enumerate()
                    .map(|(i, g)| cartao_gate(&p, g, i, pr.abrir.clone(), vm)),
            )
        };
        return base.child(card).into_any_element();
    }
    let trocar = pr.trocar_visao.clone();
    let seletor = Segmented::new("visao-fila", ["Rio", "Lista"])
        .selected(usize::from(pr.visao == Visao::Lista))
        .on_select(move |i, _, cx| {
            trocar(if i == 1 { Visao::Lista } else { Visao::Rio }, cx);
        });
    let fila_agora = Card::new()
        .title("Fila agora")
        .description("gate = checagem pesada antes da PR: build, lint, testes")
        .action(seletor)
        .child(match pr.visao {
            Visao::Rio => rio(&p, vm, pr.abrir.clone()).into_any_element(),
            Visao::Lista => lista(&p, vm, pr.abrir.clone()).into_any_element(),
        });
    let mut pagina = base.child(linha_estado(&p, vm)).child(fila_agora);
    if !vm.workers.is_empty() {
        pagina = pagina.child(workers(&p, vm));
    }
    pagina = pagina
        .child(
            Card::new()
                .title("Memória da máquina")
                .meta(vm.memoria_resumo.clone())
                .child(medidor(&p, vm)),
        )
        .child(graficos(&p, pr.historico));
    pagina = pagina.child(journal(&p, vm));
    if !vm.fora.is_empty() {
        pagina = pagina.child(
            Card::new()
                .title("Fora da fila")
                .meta(format!(
                    "{} sem gate-lock",
                    plural(vm.fora.len() as u64, "processo", "processos")
                ))
                .description("Furam a conta de memória dos gates.")
                .children(vm.fora.iter().map(|f| {
                    div()
                        .py(px(6.0))
                        .text_size(px(size::META))
                        .text_color(p.bad)
                        .child(f.clone())
                })),
        );
    }
    pagina.into_any_element()
}

fn linha_estado(p: &Palette, vm: &FilaVm) -> impl IntoElement {
    let mut resto = vm.resto.clone();
    if vm.abaixo_do_minimo {
        // o aviso de memória vai no fim da linha, na cor de falha, como no web
        if let Some(i) = resto.iter().position(|r| r.starts_with("livre ")) {
            let aviso = resto.remove(i);
            return div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(linha_de_estado(p, &vm.principal, &resto))
                .child(
                    div()
                        .text_size(px(size::BODY))
                        .text_color(p.bad)
                        .child(aviso),
                )
                .into_any_element();
        }
    }
    linha_de_estado(p, &vm.principal, &resto).into_any_element()
}

fn cartao_gate(
    p: &Palette,
    g: &Gate,
    i: usize,
    abrir: AoAbrir,
    vm: &FilaVm,
) -> gpui::Stateful<gpui::Div> {
    let det = vm.detalhe(&g.pid);
    let fio = match g.estado {
        Estado::Esperando => FioState::Parado,
        _ => FioState::Fiando,
    };
    let mut c = div()
        .id(SharedString::from(format!("gate-{}-{i}", g.pid)))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .cursor_pointer()
        .hover(|s| s.bg(p.hover))
        .on_click(move |_, w, cx| {
            if let Some(d) = det.clone() {
                abrir(d, w, cx);
            }
        })
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(g.label.clone()))
                        .child(texto_meta(p, g.meta.clone()).truncate()),
                )
                .child(texto_meta(p, g.tempo.clone())),
        )
        .child(Fio::new(
            SharedString::from(format!("fio-{}-{i}", g.pid)),
            fio,
        ))
        .child(texto_meta(p, g.cmd.clone()).line_clamp(2));
    if let Estado::Remoto(h) = &g.estado {
        c = c.child(texto_meta(
            p,
            format!("memória e CPU ficam em {h}; não entram na conta local"),
        ));
    }
    if g.estado == Estado::Rodando {
        c = c.child(texto_meta(
            p,
            format!("memória {} · CPU {}", g.memoria, g.cpu),
        ));
    }
    if let Some(passo) = &g.passo {
        c = c.child(texto_meta(
            p,
            format!("passo atual: {passo} · {}", g.passo_tempo),
        ));
    }
    c
}

fn raia(
    p: &Palette,
    titulo: &str,
    gates: &[Gate],
    vazio: &str,
    abrir: AoAbrir,
    vm: &FilaVm,
) -> impl IntoElement {
    let mut s = div()
        .flex_1()
        .min_w(px(220.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .text_size(px(size::LABEL))
                .child(titulo.to_owned())
                .child(texto_meta(p, gates.len().to_string())),
        );
    if gates.is_empty() {
        s = s.child(texto_meta(p, vazio.to_owned()));
    }
    s.children(
        gates
            .iter()
            .enumerate()
            .map(|(i, g)| cartao_gate(p, g, i, abrir.clone(), vm)),
    )
}

fn concluidos_el(p: &Palette, vm: &FilaVm) -> impl IntoElement {
    let mut s = div()
        .flex_1()
        .min_w(px(220.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(div().text_size(px(size::LABEL)).child("Concluídos"));
    if vm.concluidos.is_empty() {
        s = s.child(texto_meta(p, "Nada concluído ainda."));
    }
    s.children(vm.concluidos.iter().enumerate().map(|(i, c)| {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if c.ruim { p.bad } else { p.fg })
                            .child(c.label.clone()),
                    )
                    .child(texto_meta(p, c.quando.clone())),
            )
            .child(Fio::new(
                ("feito", i),
                if c.ruim {
                    FioState::Cortado
                } else {
                    FioState::Medido
                },
            ))
            .child(texto_meta(p, c.resumo.clone()).truncate())
    }))
}

fn rio(p: &Palette, vm: &FilaVm, abrir: AoAbrir) -> impl IntoElement {
    if vm.esperando.is_empty() && vm.rodando.is_empty() && vm.remotos.is_empty() {
        return div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(EmptyState::new("Nada rodando, ninguém na fila"))
            .child(concluidos_el(p, vm))
            .into_any_element();
    }
    let mut r = div()
        .flex()
        .flex_wrap()
        .gap(px(24.0))
        .child(raia(
            p,
            "Esperando",
            &vm.esperando,
            "Ninguém na fila.",
            abrir.clone(),
            vm,
        ))
        .child(raia(
            p,
            "Rodando",
            &vm.rodando,
            "Nenhum gate rodando.",
            abrir.clone(),
            vm,
        ));
    if !vm.remotos.is_empty() {
        r = r.child(raia(p, "Em worker remoto", &vm.remotos, "", abrir, vm));
    }
    r.child(concluidos_el(p, vm)).into_any_element()
}

fn lista(p: &Palette, vm: &FilaVm, abrir: AoAbrir) -> impl IntoElement {
    let gates: Vec<&Gate> = vm.lista().collect();
    if gates.is_empty() {
        return EmptyState::new("Fila vazia").into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .children(gates.into_iter().enumerate().map(|(i, g)| {
            let abrir = abrir.clone();
            let det = vm.detalhe(&g.pid);
            let (rotulo, tom) = match g.estado {
                Estado::Esperando => ("na fila", Tone::Warn),
                _ => ("rodando", Tone::Busy),
            };
            div()
                .id(("linha-gate", i))
                .flex()
                .items_center()
                .gap(px(12.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(p.border)
                .cursor_pointer()
                .hover(|s| s.bg(p.hover))
                .on_click(move |_, w, cx| {
                    if let Some(d) = det.clone() {
                        abrir(d, w, cx);
                    }
                })
                .child(
                    div()
                        .w(px(260.0))
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(g.label.clone()))
                        .child(texto_meta(p, g.meta.clone()).truncate()),
                )
                .child(StatusBadge::new(rotulo, tom))
                .child(texto_meta(p, g.tempo.clone()))
                .child(texto_meta(p, g.memoria.clone()))
                .child(texto_meta(p, g.cpu.clone()))
                .child(
                    texto_meta(p, g.passo.clone().unwrap_or_else(|| "—".into()))
                        .flex_1()
                        .truncate(),
                )
        }))
        .into_any_element()
}

fn workers(p: &Palette, vm: &FilaVm) -> Card {
    let fora = vm.workers.iter().filter(|w| !w.1).count();
    Card::new()
        .title("Workers")
        .meta(format!(
            "{} configurado{}{}",
            vm.workers.len(),
            if vm.workers.len() > 1 { "s" } else { "" },
            if fora > 0 {
                format!(" · {fora} inacessível")
            } else {
                String::new()
            }
        ))
        .children(
            vm.workers
                .iter()
                .enumerate()
                .map(|(i, (alvo, ok, resumo))| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .py(px(6.0))
                        .child(Dot::new(
                            ("worker", i),
                            if *ok { Tone::Ok } else { Tone::Bad },
                        ))
                        .child(div().text_size(px(size::BODY)).child(alvo.clone()))
                        .child(texto_meta(p, resumo.clone()))
                }),
        )
}

fn medidor(p: &Palette, vm: &FilaVm) -> impl IntoElement {
    let cores = [p.info, p.ok, p.warn, p.metal_ink];
    let total = vm.mem_total.max(1.0);
    let barra = div()
        .relative()
        .flex()
        .gap(px(1.0))
        .h(px(12.0))
        .overflow_hidden()
        .bg(p.muted)
        .children(
            vm.segmentos
                .iter()
                .zip(cores)
                .filter(|(s, _)| s.bytes > 0.0)
                .map(|(s, c)| div().h_full().w(relative((s.bytes / total) as f32)).bg(c)),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(relative((vm.reserva / total) as f32))
                .w(px(2.0))
                .bg(p.fg),
        );
    let legenda = div()
        .flex()
        .flex_wrap()
        .gap(px(16.0))
        .children(vm.segmentos.iter().zip(cores).map(|(s, c)| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().w(px(12.0)).h(px(2.0)).bg(c))
                .child(texto_meta(p, format!("{} {}", s.rotulo, mb(Some(s.bytes)))))
        }))
        .child(texto_meta(
            p,
            format!("reserva {} (traço claro na barra)", gb(Some(vm.reserva))),
        ));
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(barra)
        .child(legenda)
}

fn graficos(p: &Palette, h: &HistoricoFila) -> impl IntoElement {
    let rotulos =
        || -> Vec<SharedString> { (0..h.len()).map(|_| SharedString::default()).collect() };
    let card =
        |titulo: &str, meta: &str| Card::new().title(titulo.to_owned()).meta(meta.to_owned());
    if h.len() < 2 {
        return card(
            "Última hora",
            "a tendência aparece a partir da segunda amostra",
        )
        .into_any_element();
    }
    div()
        .flex()
        .flex_wrap()
        .gap(px(24.0))
        .child(
            div().flex_1().min_w(px(320.0)).child(
                card("Gates simultâneos", "última hora").child(
                    AreaChart::new("grafico-fila", rotulos())
                        .series(ChartSeries::new("Rodando", h.serie(|p| p.rodando), p.info))
                        .series(ChartSeries::new(
                            "Esperando",
                            h.serie(|p| p.esperando),
                            p.warn,
                        ))
                        .y_fmt(|v| format!("{}", v.round()))
                        .height(200.0),
                ),
            ),
        )
        .child(
            div().flex_1().min_w(px(320.0)).child(
                card("Memória da fila", "GB").child(
                    AreaChart::new("grafico-memoria", rotulos())
                        .series(ChartSeries::new(
                            "Memória dos gates",
                            h.serie(|p| p.gates_gb),
                            p.info,
                        ))
                        .series(
                            ChartSeries::new("Livre na máquina", h.serie(|p| p.livre_gb), p.ok)
                                .no_fill(),
                        )
                        .y_fmt(|v| format!("{v:.0}"))
                        .height(200.0),
                ),
            ),
        )
        .into_any_element()
}

fn journal(p: &Palette, vm: &FilaVm) -> Card {
    let card = Card::new()
        .title("Histórico da fila")
        .meta("mais recente primeiro");
    if vm.journal.is_empty() {
        return card.child(EmptyState::new("Journal vazio"));
    }
    card.children(vm.journal.iter().map(|e| {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(p.border)
            .child(texto_meta(p, e.hora.clone()).w(px(64.0)))
            .child(
                div()
                    .w(px(130.0))
                    .child(StatusBadge::new(e.evento.clone(), e.tom)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(size::BODY))
                            .child(e.gate.clone()),
                    )
                    .child(texto_meta(p, e.onde.clone()).truncate()),
            )
            .child(texto_meta(p, e.reserva.clone()).w(px(40.0)))
            .child(
                texto_meta(p, e.resultado.clone())
                    .w(px(200.0))
                    .truncate()
                    .text_color(if e.tom == Tone::Bad {
                        p.bad
                    } else {
                        p.muted_fg
                    }),
            )
    }))
}

#[cfg(test)]
#[path = "fila_tests.rs"]
mod tests;
