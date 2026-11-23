//! App gpui do painel do Hades: casca com o trilho, a Visão geral, Loop, Fila de gates e Consumo, a barra de estado e
//! a paleta Ctrl K. As outras páginas entram nos PRs seguintes e, até lá, mostram o estado vazio.

pub mod acoes;
pub mod argos;
pub mod embutidas;
pub mod embutidas_vm;
pub mod service;
pub mod telas;
pub mod vm;

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use hades_core::Env;
use hades_ui::components::{
    Button, CommandPalette, Confirm, Dot, EmptyState, Kbd, PageHeader, Rail, RailBrand, RailGroup,
    RailItem, Sparkline, StatusBar, Toast,
};
use hades_ui::icons::{Icon, IconName};
use hades_ui::tokens::size;
use hades_ui::{Palette, Tone, palette};

use crate::acoes::{Acao, Pedido, Resultado};
use crate::embutidas::{Aberta, Registro};
use crate::embutidas_vm::{Destino, GrupoFerramenta};
use crate::service::Dados;
use crate::vm::{Historico, Leitura, Pagina, Tarefa, Tom, VisaoGeral};

pub const NOME: &str = "Hades";

/// Intervalo da amostra periódica: snapshot e fila relidos no executor de fundo.
const PERIODO_MS: u64 = 15_000;

pub struct PanelView {
    env: Env,
    pagina: &'static Pagina,
    recolhida: bool,
    paleta: bool,
    foco: FocusHandle,
    dados: Option<Dados>,
    aviso: Option<String>,
    historico: Historico,
    tarefa: Tarefa,
    detalhe: Option<telas::Detalhe>,
    visao_fila: telas::fila::Visao,
    historico_fila: telas::fila::HistoricoFila,
    tela: Option<telas::Tela>,
    registros: Vec<Registro>,
    grupos_ferramentas: Vec<GrupoFerramenta>,
    /// Views de ferramenta já abertas: seguem vivas ao trocar de página.
    abertas: Vec<(&'static str, Rc<dyn Aberta>)>,
    /// Ferramenta e tela no corpo, no lugar da página.
    ferramenta: Option<(&'static str, &'static str)>,
    acoes: Vec<Acao>,
    confirmar: Option<(Pedido, String)>,
    aviso_acao: Option<(Tone, String, Option<String>)>,
    argos: Option<(argos::Estado, String)>,
}

fn tone(t: Tom) -> Tone {
    match t {
        Tom::Ok => Tone::Ok,
        Tom::Aviso => Tone::Warn,
        Tom::Ruim => Tone::Bad,
        Tom::Parado => Tone::Neutral,
        Tom::Ocupado => Tone::Busy,
    }
}

fn cor(p: &Palette, t: Option<Tom>) -> gpui::Hsla {
    match t {
        Some(Tom::Aviso) => p.warn,
        Some(Tom::Ruim) => p.bad,
        _ => p.fg,
    }
}

impl PanelView {
    pub fn new(env: Env, foco: FocusHandle, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            env,
            pagina: vm::por_id(vm::VISAO_GERAL).expect("Visão geral no trilho"),
            recolhida: false,
            paleta: false,
            foco,
            dados: None,
            aviso: None,
            historico: Historico::default(),
            tarefa: Tarefa::default(),
            detalhe: None,
            visao_fila: telas::fila::Visao::default(),
            historico_fila: telas::fila::HistoricoFila::default(),
            tela: None,
            registros: embutidas::registros(),
            grupos_ferramentas: Vec::new(),
            abertas: Vec::new(),
            ferramenta: None,
            acoes: Vec::new(),
            confirmar: None,
            aviso_acao: None,
            argos: None,
        };
        let ligadas: Vec<_> = v.registros.iter().map(|r| r.ligada).collect();
        let ocupados: Vec<&str> = vm::paginas().map(|p| p.rotulo).collect();
        v.grupos_ferramentas = embutidas_vm::grupos(&ligadas, &ocupados);
        v.carregar_acoes(cx);
        v.recarregar(cx);
        v
    }

    /// Lê snapshot e fila no executor de fundo; resultado de uma geração velha é descartado.
    fn recarregar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.tarefa.iniciar() else {
            return;
        };
        let env = self.env.clone();
        let com_consumo = self.pagina.id == "usage";
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { service::carregar(env, com_consumo) })
                .await;
            let _ = this.update(cx, |v, cx| {
                if !v.tarefa.concluir(g) {
                    return;
                }
                match r {
                    Ok(d) => {
                        v.historico
                            .registrar(vm::ponto(d.t_ms, &d.snapshot, &d.fila));
                        v.historico_fila.registrar(d.t_ms, &d.fila);
                        v.dados = Some(d);
                        v.aviso = None;
                    }
                    Err(e) => v.aviso = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Amostra periódica enquanto a janela existir.
    pub fn iniciar(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(PERIODO_MS))
                    .await;
                if this.update(cx, |v, cx| v.recarregar(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Catálogo das ações no executor de fundo; skill sem `panel action` deixa a paleta sem ações.
    fn carregar_acoes(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async { acoes::listar() })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.acoes = r.unwrap_or_default();
                cx.notify();
            });
        })
        .detach();
    }

    /// Ação escolhida na paleta: confirma antes quando o catálogo pede.
    fn pedir_acao(&mut self, p: Pedido, cx: &mut Context<Self>) {
        let Some(a) = self.acoes.iter().find(|a| a.id == p.id) else {
            self.aviso_acao = Some((Tone::Warn, acoes::INDISPONIVEL.into(), None));
            cx.notify();
            return;
        };
        if a.pede_confirmacao(p.op.as_deref()) {
            let rotulo = match &p.op {
                Some(op) => format!("{}: {op}", a.rotulo),
                None => a.rotulo.clone(),
            };
            self.confirmar = Some((p, rotulo));
            cx.notify();
            return;
        }
        self.lancar(p, cx);
    }

    /// Lança a ação no executor de fundo; o job entra na fila e a amostra é relida.
    fn lancar(&mut self, p: Pedido, cx: &mut Context<Self>) {
        self.confirmar = None;
        self.aviso_acao = Some((Tone::Busy, format!("Lançando {}…", p.id), None));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { acoes::lancar(&p) })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.aviso_acao = Some(match r {
                    Resultado::Lancada {
                        job,
                        titulo,
                        exclusiva,
                    } => (
                        Tone::Ok,
                        titulo,
                        Some(if exclusiva {
                            format!("Job {job} na fila, exclusivo no ambiente.")
                        } else {
                            format!("Job {job} na fila.")
                        }),
                    ),
                    Resultado::Recusada { erro, codigo } => {
                        (Tone::Bad, erro, Some(codigo).filter(|c| !c.is_empty()))
                    }
                    Resultado::Indisponivel => (Tone::Warn, acoes::INDISPONIVEL.into(), None),
                    Resultado::Falhou(e) => (Tone::Bad, e, None),
                });
                v.recarregar(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Lê o estado do Argos e o link do canvas web no executor de fundo.
    fn consultar_argos(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async {
                    let e = argos::consultar();
                    let link = match e {
                        argos::Estado::NaoInstalado => {
                            argos::link_canvas(argos::url_painel().as_deref())
                        }
                        _ => String::new(),
                    };
                    (e, link)
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.argos = Some(r);
                cx.notify();
            });
        })
        .detach();
    }

    /// Abre a tela `tela` da ferramenta `id`, criando a view dela na primeira vez.
    fn ir_ferramenta(
        &mut self,
        id: &str,
        tela: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Rc<dyn Aberta>> {
        let item = embutidas_vm::item(&self.grupos_ferramentas, id, tela)?;
        let (fid, tid) = (item.ferramenta, item.tela);
        let aberta = match self.abertas.iter().find(|(f, _)| *f == fid) {
            Some((_, a)) => {
                a.ir(tid, cx);
                a.clone()
            }
            None => {
                let r = self.registros.iter().find(|r| r.ligada.info.id == fid)?;
                let cwd = std::env::current_dir().unwrap_or_default();
                let a = (r.abrir)(self.env.clone(), cwd, tid, window, cx);
                self.abertas.push((fid, a.clone()));
                a
            }
        };
        self.ferramenta = Some((fid, tid));
        self.tela = None;
        self.detalhe = None;
        cx.notify();
        Some(aberta)
    }

    /// Roda o comando `comando` da ferramenta `id`, levando o corpo para ela (na primeira tela, se ainda não aberta).
    fn rodar(&mut self, id: &str, comando: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.ferramenta.map(|f| f.0) == Some(id)
            && let Some((_, a)) = self.abertas.iter().find(|(f, _)| *f == id)
        {
            let a = a.clone();
            a.executar(comando, window, cx);
            return;
        }
        let primeira = self
            .grupos_ferramentas
            .iter()
            .flat_map(|g| g.itens.iter())
            .find(|i| i.ferramenta == id)
            .map(|i| i.tela);
        if let Some(a) = primeira.and_then(|t| self.ir_ferramenta(id, t, window, cx)) {
            a.executar(comando, window, cx);
        }
    }

    fn ir(&mut self, pagina: &'static Pagina, cx: &mut Context<Self>) {
        let releitura = pagina.id == "usage" && self.pagina.id != pagina.id;
        self.ferramenta = None;
        if pagina.id == vm::ARGOS {
            self.consultar_argos(cx);
        }
        self.pagina = pagina;
        self.detalhe = None;
        if releitura {
            self.recarregar(cx);
        }
        self.tela = telas::Tela::abrir(pagina.id, &self.env, cx);
        cx.notify();
    }

    fn rail(&self, cx: &mut Context<Self>) -> Rail {
        let p = palette(cx);
        let recolhimento = cx.entity();
        let selecao = cx.entity();
        let mut grupos: Vec<RailGroup> = Vec::new();
        for (titulo, paginas) in vm::GRUPOS {
            grupos.push(RailGroup::new(
                *titulo,
                paginas
                    .iter()
                    .map(|pg| RailItem::new(pg.rotulo).icon(pg.icone)),
            ));
            if *titulo == vm::FERRAMENTAS {
                grupos.extend(self.grupos_ferramentas.iter().map(|g| {
                    RailGroup::new(
                        g.titulo,
                        g.itens
                            .iter()
                            .map(|i| RailItem::new(i.rotulo.clone()).icon(i.icone)),
                    )
                }));
            }
        }
        let selecionado = self
            .ferramenta
            .and_then(|(f, t)| embutidas_vm::item(&self.grupos_ferramentas, f, t))
            .map_or_else(
                || SharedString::from(self.pagina.rotulo),
                |i| SharedString::from(i.rotulo.clone()),
            );
        let projeto = self
            .dados
            .as_ref()
            .and_then(|d| d.snapshot["instance"]["project"].as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("—")
            .to_owned();
        let rodape = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .text_size(px(size::META))
            .text_color(p.muted_fg)
            .child(projeto)
            .child(Kbd::new("Ctrl K"));
        Rail::new(grupos)
            .brand(RailBrand::hades())
            .theme_toggle()
            .footer(rodape)
            .collapsed(self.recolhida)
            .on_collapse(move |recolher, _, cx| {
                recolhimento.update(cx, |v, cx| {
                    v.recolhida = recolher;
                    cx.notify();
                });
            })
            .on_select(move |rotulo, window, cx| {
                selecao.update(cx, |v, cx| {
                    if let Some(pg) = vm::por_rotulo(rotulo) {
                        v.ir(pg, cx);
                    } else if let Some(i) = embutidas_vm::por_rotulo(&v.grupos_ferramentas, rotulo)
                    {
                        let (f, t) = (i.ferramenta, i.tela);
                        v.ir_ferramenta(f, t, window, cx);
                    }
                });
            })
            .selected(selecionado)
    }

    fn corpo(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if let Some((f, _)) = self.ferramenta
            && let Some((_, a)) = self.abertas.iter().find(|(id, _)| *id == f)
        {
            return a.elemento();
        }
        if self.pagina.id == vm::ARGOS {
            return self.tela_argos(cx);
        }
        if !self.pagina.pronta {
            return EmptyState::new(self.pagina.rotulo)
                .hint("Esta página chega numa próxima versão do painel.")
                .fill()
                .into_any_element();
        }
        if let Some(t) = &self.tela {
            return t.elemento();
        }
        match (&self.dados, &self.aviso) {
            (Some(d), _) => self.tela(d, cx),
            (None, Some(e)) => EmptyState::new("Sem dados do Hades")
                .hint(e.clone())
                .fill()
                .into_any_element(),
            (None, None) => EmptyState::new("Lendo o estado do Hades…")
                .fill()
                .into_any_element(),
        }
    }

    fn tela(&self, d: &Dados, cx: &mut Context<Self>) -> gpui::AnyElement {
        let alvo = cx.entity();
        let abrir: telas::loop_::AoAbrir = Rc::new(move |d, _, cx| {
            alvo.update(cx, |v, cx| {
                v.detalhe = Some(d);
                cx.notify();
            });
        });
        let descricao = self.pagina.descricao;
        match self.pagina.id {
            "loop" => {
                let vm = telas::loop_::LoopVm::de(&d.fila, d.t_ms as f64 / 1000.0);
                telas::loop_::tela(&vm, descricao, abrir, cx)
            }
            "queue" => {
                let vm = telas::fila::FilaVm::de(&d.fila);
                let alvo = cx.entity();
                telas::fila::tela(
                    telas::fila::Props {
                        vm: &vm,
                        historico: &self.historico_fila,
                        visao: self.visao_fila,
                        descricao,
                        abrir,
                        trocar_visao: Rc::new(move |visao, cx| {
                            alvo.update(cx, |v, cx| {
                                v.visao_fila = visao;
                                cx.notify();
                            });
                        }),
                    },
                    cx,
                )
            }
            "usage" => {
                let e = telas::consumo::Estado::de(d.consumo.as_ref(), &d.limites);
                telas::consumo::tela(&e, descricao, abrir, cx)
            }
            _ => self.visao_geral(d, cx).into_any_element(),
        }
    }

    fn visao_geral(&self, d: &Dados, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let v = VisaoGeral::de(&d.snapshot, &d.fila);
        let mut pagina = div()
            .id("visao-geral")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .p(px(24.0))
            .child(PageHeader::new("Visão geral").description(self.pagina.descricao));
        if v.pedem > 0 {
            pagina = pagina.child(pedem_voce(&p, v.pedem));
        }
        pagina
            .child(estado(&p, &v))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .children(v.leituras.iter().map(|l| leitura(&p, l))),
            )
            .child(tendencia(&p, &self.historico))
    }

    fn tela_argos(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = palette(cx);
        let cabeca = PageHeader::new("Argos").description(self.pagina.descricao);
        let corpo: gpui::AnyElement = match &self.argos {
            None => EmptyState::new("Procurando o Argos…").into_any_element(),
            Some((argos::Estado::NaoInstalado, link)) => EmptyState::new("Argos não instalado")
                .hint(format!(
                    "Instale o {} para abrir o grafo aqui. Enquanto isso, o canvas está no painel web: {link}",
                    argos::BINARIO
                ))
                .into_any_element(),
            Some((e, _)) => {
                let alvo = cx.entity();
                let mut caixa = div().flex().flex_col().gap(px(8.0)).child(
                    Button::new("argos-abrir", "Abrir o Argos")
                        .primary()
                        .on_click(move |_, _, cx| {
                            let r = argos::abrir_janela();
                            alvo.update(cx, |v, cx| {
                                if let Err(e) = r {
                                    v.aviso_acao = Some((Tone::Bad, e, None));
                                    cx.notify();
                                }
                            });
                        }),
                );
                let linhas: Vec<(String, String)> = match e {
                    argos::Estado::Campos(c) => c.clone(),
                    argos::Estado::Falhou(m) => vec![("erro".into(), m.clone())],
                    _ => vec![(
                        "estado".into(),
                        "esta versão do Argos não tem --json".into(),
                    )],
                };
                for (k, val) in linhas {
                    caixa = caixa.child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .text_size(px(size::BODY))
                            .child(div().w(px(160.0)).text_color(p.muted_fg).child(k))
                            .child(val),
                    );
                }
                caixa.into_any_element()
            }
        };
        div()
            .id("argos")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .p(px(24.0))
            .child(cabeca)
            .child(corpo)
            .into_any_element()
    }

    fn paleta(&self, cx: &mut Context<Self>) -> CommandPalette {
        let fechar = cx.entity();
        let ir = cx.entity();
        let ligadas: Vec<_> = self.registros.iter().map(|r| r.ligada).collect();
        let mut itens = vm::itens_paleta();
        itens.extend(embutidas_vm::itens_paleta(&ligadas));
        itens.extend(acoes::itens_paleta(&self.acoes));
        CommandPalette::new("paleta", itens)
            .placeholder("Ir para uma página, tela ou ação…")
            .on_close(move |window, cx| {
                fechar.update(cx, |v, cx| {
                    v.paleta = false;
                    window.focus(&v.foco, cx);
                    cx.notify();
                });
            })
            .on_pick(move |id, window, cx| {
                ir.update(cx, |v, cx| {
                    if let Some(pg) = vm::destino_paleta(id) {
                        v.ir(pg, cx);
                    } else if let Some(d) = embutidas_vm::destino(id) {
                        match d {
                            Destino::Tela { ferramenta, tela } => {
                                v.ir_ferramenta(ferramenta, tela, window, cx);
                            }
                            Destino::Comando {
                                ferramenta,
                                comando,
                            } => v.rodar(ferramenta, comando, window, cx),
                        }
                    } else if let Some(p) = acoes::pedido(id) {
                        v.pedir_acao(p, cx);
                    }
                });
            })
    }
}

fn pedem_voce(p: &Palette, n: u64) -> impl IntoElement {
    let texto = if n == 1 {
        "Um agente pede resposta.".to_owned()
    } else {
        format!("{n} agentes pedem resposta.")
    };
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(20.0))
        .py(px(16.0))
        .border_1()
        .border_color(p.warn)
        .bg(p.card)
        .child(Icon::new(IconName::Hand).color(p.warn))
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(size::SECTION))
                        .child(format!("Pedem você {n}")),
                )
                .child(
                    div()
                        .text_size(px(size::BODY))
                        .text_color(p.muted_fg)
                        .child(texto),
                ),
        )
}

fn estado(p: &Palette, v: &VisaoGeral) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .text_size(px(size::DISPLAY))
                .child(Dot::new("estado", tone(v.tom)).pulse(v.tom == Tom::Ocupado))
                .child(format!("{}.", v.estado)),
        )
        .child(
            div()
                .text_size(px(size::BODY))
                .text_color(p.muted_fg)
                .child(v.fatos.clone()),
        )
}

fn leitura(p: &Palette, l: &Leitura) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .min_w(px(200.0))
        .flex_1()
        .p(px(16.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .child(
            div()
                .text_size(px(size::META))
                .text_color(p.muted_fg)
                .child(l.rotulo),
        )
        .child(
            div()
                .text_size(px(size::DISPLAY))
                .text_color(cor(p, l.tom))
                .child(l.valor.clone()),
        )
        .child(
            div()
                .text_size(px(size::META))
                .text_color(p.muted_fg)
                .child(l.detalhe.clone()),
        )
}

fn tendencia(p: &Palette, h: &Historico) -> impl IntoElement {
    let caixa = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .child(div().text_size(px(size::SECTION)).child("Última hora"));
    if h.len() < 2 {
        return caixa.child(
            div()
                .text_size(px(size::BODY))
                .text_color(p.muted_fg)
                .child(
                    "Coletando a tendência: um ponto a cada 15 s enquanto o painel está aberto.",
                ),
        );
    }
    let linha = |rotulo: &'static str, valores: Vec<f64>, cor: gpui::Hsla| {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(140.0))
                    .text_size(px(size::META))
                    .text_color(p.muted_fg)
                    .child(rotulo),
            )
            .child(Sparkline::new(valores).color(cor).size(320.0, 28.0))
    };
    caixa
        .child(linha("Memória do Hades", h.serie(|x| x.memoria), p.vivo))
        .child(linha("Fila de gates", h.serie(|x| x.fila), p.info))
        .child(linha("Agentes ativos", h.serie(|x| x.agentes), p.ok))
}

impl Render for PanelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let rail = self.rail(cx);
        let corpo = self.corpo(cx);
        let (esquerda, direita) = self.dados.as_ref().map_or_else(
            || vm::barra_estado(&serde_json::Value::Null),
            |d| vm::barra_estado(&d.snapshot),
        );
        let mut raiz = div()
            .id("painel")
            .track_focus(&self.foco)
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, _, cx| {
                let m = &ev.keystroke.modifiers;
                if vm::abre_paleta(&ev.keystroke.key, m.control, m.platform) {
                    v.paleta = true;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.fg)
            .child(div().flex_1().flex().min_h_0().child(rail).child(corpo))
            .child(
                StatusBar::new(esquerda, direita)
                    .version(concat!("hades-panel ", env!("CARGO_PKG_VERSION"))),
            );
        if self.paleta {
            raiz = raiz.child(self.paleta(cx));
        }
        if let Some((tom, titulo, detalhe)) = &self.aviso_acao {
            let fechar = cx.entity();
            let mut t = Toast::new("aviso-acao", *tom, titulo.clone()).on_close(move |_, _, cx| {
                fechar.update(cx, |v, cx| {
                    v.aviso_acao = None;
                    cx.notify();
                });
            });
            if let Some(d) = detalhe {
                t = t.description(d.clone());
            }
            raiz = raiz.child(div().absolute().bottom(px(40.0)).right(px(16.0)).child(t));
        }
        if let Some((pedido, rotulo)) = &self.confirmar {
            let ok = cx.entity();
            let nao = cx.entity();
            let pedido = pedido.clone();
            raiz = raiz.child(
                Confirm::new(format!("{rotulo}?"))
                    .detail("A ação roda no ambiente local como um job do painel.")
                    .confirm_label("Lançar")
                    .on_confirm(move |_, _, cx| {
                        let p = pedido.clone();
                        ok.update(cx, |v, cx| v.lancar(p, cx));
                    })
                    .on_cancel(move |_, _, cx| {
                        nao.update(cx, |v, cx| {
                            v.confirmar = None;
                            cx.notify();
                        });
                    }),
            );
        }
        if let Some(d) = &self.detalhe {
            let fechar = cx.entity();
            raiz = raiz.child(telas::detalhe_sheet(d, cx, move |_, cx| {
                fechar.update(cx, |v, cx| {
                    v.detalhe = None;
                    cx.notify();
                });
            }));
        }
        raiz
    }
}

pub fn abrir_janela(cx: &mut App) {
    let aberta = cx.open_window(gpui::WindowOptions::default(), |window, cx| {
        window.set_window_title(NOME);
        let view: Entity<PanelView> = cx.new(|cx| {
            PanelView::iniciar(cx);
            PanelView::new(Env::from_process(), cx.focus_handle(), cx)
        });
        let foco = view.read(cx).foco.clone();
        window.focus(&foco, cx);
        view
    });
    if aberta.is_err() {
        cx.quit();
    }
}
