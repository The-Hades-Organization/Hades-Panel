//! View-model do painel, sem gpui: navegação (trilho e paleta), histórico da última hora, Visão geral e barra de
//! estado, montados sobre o JSON do `snapshot` e da `fila` do `hades-panel-core`.

use std::collections::VecDeque;

use hades_ui::components::PaletteItem;
use hades_ui::icons::IconName;
use serde_json::Value;

/// Uma página do trilho. `pronta` diz se a página já tem tela neste app; as outras mostram o estado vazio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pagina {
    pub id: &'static str,
    pub rotulo: &'static str,
    pub icone: IconName,
    pub descricao: &'static str,
    pub pronta: bool,
}

const fn pagina(
    id: &'static str,
    rotulo: &'static str,
    icone: IconName,
    descricao: &'static str,
) -> Pagina {
    Pagina {
        id,
        rotulo,
        icone,
        descricao,
        pronta: false,
    }
}

pub const VISAO_GERAL: &str = "overview";

/// Grupos do trilho (`CORE_GROUPS` de `kit/panel/ui/app.tsx:662-668`), com Ferramentas no lugar de "Hades" e as
/// seis ferramentas embutidas (plano F5 painel §6). Rótulos e descrições vêm do `CORE_NAV` (`app.tsx:637-661`).
pub const GRUPOS: &[(&str, &[Pagina])] = &[
    (
        "Ambiente",
        &[
            Pagina {
                pronta: true,
                ..pagina(
                    VISAO_GERAL,
                    "Visão geral",
                    IconName::LayoutDashboard,
                    "Estado, ações principais e tendência da última hora",
                )
            },
            pagina(
                "code",
                "Branch e PRs",
                IconName::GitBranch,
                "O que entra na branch local: base do projeto ou base + PRs",
            ),
        ],
    ),
    (
        "Loop",
        &[
            Pagina {
                pronta: true,
                ..pagina(
                    "loop",
                    "Loop",
                    IconName::Repeat,
                    "Escalações, entregas, ações em 24 h, PRs, agentes e locks",
                )
            },
            pagina(
                "orchestration",
                "Orquestração",
                IconName::Workflow,
                "Agentes do orquestrador, perguntas ao dono, relatórios e PRs de consolidação",
            ),
            pagina(
                "reviews",
                "Revisões",
                IconName::ScanSearch,
                "Revisões sobre PRs escolhidas; histórico das últimas 20",
            ),
            Pagina {
                pronta: true,
                ..pagina(
                    "queue",
                    "Fila de gates",
                    IconName::ListOrdered,
                    "Quem roda, quem espera, memória e journal da fila",
                )
            },
            Pagina {
                pronta: true,
                ..pagina(
                    "usage",
                    "Consumo",
                    IconName::Coins,
                    "Tokens por papel, agente e dia, e a linha de base",
                )
            },
        ],
    ),
    (
        "Operação",
        &[
            pagina(
                "herdr",
                "Herdr",
                IconName::TerminalSquare,
                "Workspaces e agentes do herdr",
            ),
            pagina(
                "jobs",
                "Execuções",
                IconName::SquareTerminal,
                "Ações disparadas pelo painel, com log ao vivo",
            ),
            pagina(
                "logs",
                "Logs",
                IconName::ScrollText,
                "Serviços, builds, processos, containers e documentos do ambiente",
            ),
            pagina(
                "notify",
                "Notificações",
                IconName::Bell,
                "Canais, vigia e as últimas 50 notificações",
            ),
        ],
    ),
    (
        "Ferramentas",
        &[
            pagina(
                "argos",
                "Argos",
                IconName::Grafo,
                "Grafo dos agentes ao vivo",
            ),
            pagina(
                "moirai",
                "Moirai",
                IconName::Hourglass,
                "Orçamento e estimativa das frentes",
            ),
            pagina(
                "cerberus",
                "Cerberus",
                IconName::Guarda,
                "Porta, Limiar e Vigília: guardas, vereditos e exceções",
            ),
            pagina("minos", "Minos", IconName::Scale, "Julgamento das entregas"),
            pagina(
                "thanatos",
                "Thanatos",
                IconName::Skull,
                "Post-mortems de incidentes e bugs em produção",
            ),
            pagina(
                "caronte",
                "Caronte",
                IconName::Caronte,
                "Leve uma orquestração para outra sessão ou máquina",
            ),
        ],
    ),
    (
        "Sistema",
        &[
            pagina(
                "plugins",
                "Plugins",
                IconName::Puzzle,
                "Listar, ligar e desligar plugins",
            ),
            pagina(
                "extensoes",
                "Extensões",
                IconName::Plug,
                "Ferramentas externas: instalada, versão, saúde",
            ),
            pagina(
                "config",
                "Configurações",
                IconName::SlidersHorizontal,
                "Memória, builds, subida, plantão, túnel, painel e o que o projeto declara",
            ),
            pagina(
                "docs",
                "Documentação",
                IconName::BookOpen,
                "SKILL.md, referências e changelog",
            ),
        ],
    ),
];

pub fn paginas() -> impl Iterator<Item = &'static Pagina> {
    GRUPOS.iter().flat_map(|(_, p)| p.iter())
}

pub fn por_id(id: &str) -> Option<&'static Pagina> {
    paginas().find(|p| p.id == id)
}

/// O trilho devolve o rótulo do item clicado.
pub fn por_rotulo(rotulo: &str) -> Option<&'static Pagina> {
    paginas().find(|p| p.rotulo == rotulo)
}

/// Prefixo do id dos itens de navegação na paleta.
const IR: &str = "ir:";

/// Itens de navegação da paleta Ctrl K (`commands` de `app.tsx`, grupo "Páginas"), um por página do trilho.
pub fn itens_paleta() -> Vec<PaletteItem> {
    paginas()
        .map(|p| {
            PaletteItem::new(format!("{IR}{}", p.id), "Páginas", p.rotulo, p.icone)
                .desc(p.descricao)
        })
        .collect()
}

/// A página que um item escolhido na paleta abre.
pub fn destino_paleta(id: &str) -> Option<&'static Pagina> {
    id.strip_prefix(IR).and_then(por_id)
}

/// `true` para Ctrl K (ou Cmd K): abre a paleta.
pub fn abre_paleta(tecla: &str, ctrl: bool, plataforma: bool) -> bool {
    (ctrl || plataforma) && tecla.eq_ignore_ascii_case("k")
}

// ------------------------------------------------------------------ histórico da última hora

/// Janela da tendência: a última hora.
pub const JANELA_MS: i64 = 60 * 60 * 1000;

/// Um ponto da amostra periódica.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ponto {
    pub t_ms: i64,
    /// Memória da slice do Hades, em bytes.
    pub memoria: f64,
    /// Gates rodando mais esperando.
    pub fila: f64,
    pub agentes: f64,
}

/// Histórico em memória do processo, como o `history` da amostra do web: só a última hora.
#[derive(Clone, Debug, Default)]
pub struct Historico {
    pontos: VecDeque<Ponto>,
}

impl Historico {
    /// Acrescenta o ponto e descarta os de mais de uma hora antes dele. Ponto mais velho que o último é ignorado.
    pub fn registrar(&mut self, p: Ponto) {
        if self.pontos.back().is_some_and(|u| p.t_ms < u.t_ms) {
            return;
        }
        self.pontos.push_back(p);
        while self
            .pontos
            .front()
            .is_some_and(|f| p.t_ms - f.t_ms > JANELA_MS)
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

    pub fn serie(&self, f: impl Fn(&Ponto) -> f64) -> Vec<f64> {
        self.pontos.iter().map(f).collect()
    }
}

// ------------------------------------------------------------------ Visão geral

/// Tom de um estado, como o `kind` do web.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tom {
    Ok,
    Aviso,
    Ruim,
    Parado,
    Ocupado,
}

/// `OVERALL` de `app.tsx:617-623`: rótulo e tom do estado geral.
pub fn estado_geral(overall: &str) -> (&'static str, Tom) {
    match overall {
        "up" => ("No ar", Tom::Ok),
        "partial" => ("Parcial", Tom::Aviso),
        "paused" => ("Pausado", Tom::Parado),
        "busy" => ("Em operação", Tom::Ocupado),
        _ => ("Parado", Tom::Parado),
    }
}

/// Uma leitura da régua da Visão geral.
#[derive(Clone, Debug, PartialEq)]
pub struct Leitura {
    pub rotulo: &'static str,
    pub valor: String,
    pub detalhe: String,
    pub tom: Option<Tom>,
}

/// O que a Visão geral mostra, já em texto.
#[derive(Clone, Debug, PartialEq)]
pub struct VisaoGeral {
    pub estado: &'static str,
    pub tom: Tom,
    /// Linha de fatos sob o estado (`facts` de `overview.tsx:76-83`).
    pub fatos: String,
    /// Agentes que pedem resposta (`d.herdr.blocked`, `app.tsx:1847`).
    pub pedem: u64,
    pub leituras: Vec<Leitura>,
}

const GB: f64 = 1024.0 * 1024.0 * 1024.0;
/// `MEM_WARN` e `MEM_BAD` de `overview.tsx:10`.
const MEM_AVISO: f64 = 65.0;
const MEM_RUIM: f64 = 85.0;

/// `decimal` de `overview.tsx:25`: uma casa, vírgula.
pub fn decimal(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn plural(n: usize, um: &str, varios: &str) -> String {
    if n == 1 {
        format!("{n} {um}")
    } else {
        format!("{n} {varios}")
    }
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Gates rodando e esperando na fila (`gates` da rota `gates`, ou o `gates` do snapshot quando a fila não está).
pub fn contagem_fila(snapshot: &Value, fila: &Value) -> (usize, usize) {
    let gates = if truthy(&fila["available"]) {
        arr(&fila["gates"])
    } else {
        arr(&snapshot["gates"])
    };
    let rodando = gates.iter().filter(|g| g["state"] == "running").count();
    (rodando, gates.len() - rodando)
}

impl VisaoGeral {
    /// Porta `Estado` e `Leituras` (`overview.tsx:69-166`) sobre o snapshot e a fila, com a contagem de agentes e
    /// da fila no lugar de containers e logs, que o painel gpui ainda não lê.
    pub fn de(snapshot: &Value, fila: &Value) -> Self {
        let (estado, tom) = estado_geral(snapshot["overall"].as_str().unwrap_or("stopped"));
        let servicos = arr(&snapshot["services"]);
        let ligados: Vec<&Value> = servicos.iter().filter(|s| truthy(&s["enabled"])).collect();
        let prontos = ligados.iter().filter(|s| truthy(&s["ready"])).count();
        let (rodando, esperando) = contagem_fila(snapshot, fila);

        let pausado = truthy(&snapshot["paused"]) && snapshot["overall"] != "busy";
        let fatos = [
            pausado.then(|| "pausado".to_owned()),
            (snapshot["overall"] == "busy").then(|| "ciclo em andamento".to_owned()),
            Some(if ligados.is_empty() {
                "nenhum serviço ligado".to_owned()
            } else {
                format!("{prontos} de {} serviços prontos", ligados.len())
            }),
            Some(if rodando > 0 {
                format!("{rodando} rodando")
            } else {
                "nada rodando".to_owned()
            }),
            (esperando > 0).then(|| format!("{esperando} na fila")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");

        let desligados = servicos.len() - ligados.len();
        let servicos_lido = Leitura {
            rotulo: "Serviços prontos",
            valor: format!("{prontos} / {}", ligados.len()),
            detalhe: plural(desligados, "desligado", "desligados"),
            tom: (prontos < ligados.len()).then_some(Tom::Aviso),
        };

        let usado = snapshot["slice"]["used"].as_f64();
        let teto = snapshot["slice"]["max"].as_f64().filter(|m| *m > 0.0);
        let memoria = match (usado, teto) {
            (Some(u), Some(m)) => {
                let pct = u / m * 100.0;
                Leitura {
                    rotulo: "Memória do Hades",
                    valor: format!("{} GB", decimal(u / GB)),
                    detalhe: format!("{}% do teto de {} GB", pct.round(), decimal(m / GB)),
                    tom: if pct > MEM_RUIM {
                        Some(Tom::Ruim)
                    } else if pct > MEM_AVISO {
                        Some(Tom::Aviso)
                    } else {
                        None
                    },
                }
            }
            _ => Leitura {
                rotulo: "Memória do Hades",
                valor: usado.map_or_else(|| "—".to_owned(), |u| format!("{} GB", decimal(u / GB))),
                detalhe: format!(
                    "nenhum serviço rodando · teto {}",
                    snapshot["config"]["HADES_SLICE_MAX"]
                        .as_str()
                        .unwrap_or("—")
                ),
                tom: None,
            },
        };

        let fila_lido = Leitura {
            rotulo: "Fila de gates",
            valor: (rodando + esperando).to_string(),
            detalhe: format!(
                "{} · {}",
                plural(rodando, "rodando", "rodando"),
                plural(esperando, "esperando", "esperando")
            ),
            tom: (esperando > 0).then_some(Tom::Aviso),
        };

        let agentes = arr(&fila["agents"]).len();
        let agentes_lido = Leitura {
            rotulo: "Agentes ativos",
            valor: agentes.to_string(),
            detalhe: snapshot["herdr"]["working"].as_u64().map_or_else(
                || "herdr fora do ar".to_owned(),
                |w| format!("{w} trabalhando no herdr"),
            ),
            tom: None,
        };

        Self {
            estado,
            tom,
            fatos,
            pedem: snapshot["herdr"]["blocked"].as_u64().unwrap_or(0),
            leituras: vec![agentes_lido, servicos_lido, memoria, fila_lido],
        }
    }
}

/// O ponto da amostra periódica tirado do snapshot e da fila.
pub fn ponto(t_ms: i64, snapshot: &Value, fila: &Value) -> Ponto {
    let (rodando, esperando) = contagem_fila(snapshot, fila);
    Ponto {
        t_ms,
        memoria: snapshot["slice"]["used"].as_f64().unwrap_or(0.0),
        fila: (rodando + esperando) as f64,
        agentes: arr(&fila["agents"]).len() as f64,
    }
}

/// Texto da barra de estado (plano §6): herdr e painel web à esquerda, versões do Hades e do `hades-core` à
/// direita.
pub fn barra_estado(snapshot: &Value) -> (String, String) {
    let herdr = match snapshot["herdr"]["working"].as_u64() {
        Some(n) => format!("herdr: {}", plural(n as usize, "agente", "agentes")),
        None => "herdr fora do ar".to_owned(),
    };
    let web = if snapshot["extras"]["panel"]["state"] == "active" {
        "painel web no ar"
    } else {
        "painel web parado"
    };
    let projeto = snapshot["instance"]["project"]
        .as_str()
        .filter(|s| !s.is_empty());
    let esquerda = match projeto {
        Some(p) => format!("{p} · {herdr} · {web}"),
        None => format!("{herdr} · {web}"),
    };
    let hades = snapshot["versionCheck"]["local"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map_or_else(|| "hades —".to_owned(), |v| format!("hades {v}"));
    (esquerda, format!("{hades} · hades-core {HADES_CORE}"))
}

/// Tag do `hades-core` no workspace.
pub const HADES_CORE: &str = "v0.2.7";

/// Controle de uma tarefa de fundo, como a do `cerberus-app`: no máximo uma em voo e resultado de geração velha
/// descartado.
#[derive(Debug, Default)]
pub struct Tarefa {
    geracao: u64,
    em_voo: bool,
}

impl Tarefa {
    /// Geração da nova execução, ou `None` se a anterior ainda não terminou.
    pub fn iniciar(&mut self) -> Option<u64> {
        if self.em_voo {
            return None;
        }
        self.em_voo = true;
        Some(self.geracao)
    }

    /// Descarta o que estiver em voo e libera uma nova execução.
    pub fn invalidar(&mut self) {
        self.geracao = self.geracao.wrapping_add(1);
        self.em_voo = false;
    }

    /// `true` se o resultado da geração `geracao` ainda vale; só então libera a próxima execução.
    pub fn concluir(&mut self, geracao: u64) -> bool {
        if geracao != self.geracao {
            return false;
        }
        self.em_voo = false;
        true
    }

    pub fn carregando(&self) -> bool {
        self.em_voo
    }
}

#[cfg(test)]
mod tests;
