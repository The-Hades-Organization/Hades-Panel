//! Tela Configurações (`kit/panel/ui/pages/config.tsx`): o catálogo em grupos, com a origem de cada valor
//! (ambiente, arquivo ou padrão). Grava pelo `config` do core, que segura o `flock` do `config.env`, depois de
//! mostrar a prévia do `plan` num `Sheet`.

use std::collections::HashMap;

use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use hades_core::Env;
use hades_panel_core::config;
use hades_ui::components::{
    Button, EmptyState, Page, PageHeader, Select, SelectOption, SettingRow, SettingsBar,
    SettingsGroup, Sheet,
};
use hades_ui::palette;
use hades_ui::tokens::size;

use super::config_vm::{Config, Controle, Item, Previa, ligado, texto_plano};
use super::panel_env;
use crate::vm::Tarefa;

const DESCRICAO: &str = "Memória, builds, subida, plantão, túnel, painel e o que o projeto declara";

/// A prévia aberta: o plano lido do core e as mudanças que ele cobre.
struct Gravacao {
    mudancas: Vec<(String, String)>,
    previa: Option<Result<Previa, String>>,
    gravando: bool,
}

pub struct ConfigView {
    env: Env,
    dados: Option<Result<Config, String>>,
    tarefa: Tarefa,
    /// Campos de texto e número, criados na render (precisam da janela) a cada leitura nova.
    campos: HashMap<String, Entity<InputState>>,
    /// Switch e Select editados e ainda não gravados.
    escolhas: HashMap<String, String>,
    gravacao: Option<Gravacao>,
    salvo: bool,
    erro: Option<String>,
}

impl ConfigView {
    pub fn new(env: Env, cx: &mut Context<Self>) -> Self {
        let mut v = Self {
            env,
            dados: None,
            tarefa: Tarefa::default(),
            campos: HashMap::new(),
            escolhas: HashMap::new(),
            gravacao: None,
            salvo: false,
            erro: None,
        };
        v.recarregar(cx);
        v
    }

    fn recarregar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.tarefa.iniciar() else {
            return;
        };
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let raw = env.clone();
                    let e = panel_env(env)?;
                    let estado = config::state_json(&e).map_err(|e| e.to_string())?;
                    Ok(Config::de(&estado, &|k| raw.get_raw(k).map(str::to_owned)))
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.tarefa.concluir(g) {
                    v.dados = Some(r);
                    v.campos.clear();
                    v.escolhas.clear();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn garantir_campos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Ok(c)) = &self.dados else {
            return;
        };
        let faltam: Vec<(String, String)> = c
            .itens()
            .filter(|i| matches!(i.controle, Controle::Texto | Controle::Numero(..)))
            .filter(|i| !self.campos.contains_key(&i.chave))
            .map(|i| (i.chave.clone(), i.valor.clone()))
            .collect();
        for (chave, valor) in faltam {
            let campo = cx.new(|cx| InputState::new(window, cx).default_value(valor));
            cx.subscribe(&campo, |v: &mut Self, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    v.salvo = false;
                    cx.notify();
                }
            })
            .detach();
            self.campos.insert(chave, campo);
        }
    }

    fn editado(&self, i: &Item, cx: &gpui::App) -> Option<String> {
        match &i.controle {
            Controle::Texto | Controle::Numero(..) => self
                .campos
                .get(&i.chave)
                .map(|c| c.read(cx).value().to_string()),
            _ => self.escolhas.get(&i.chave).cloned(),
        }
    }

    fn pendentes(&self, cx: &gpui::App) -> Vec<(String, String)> {
        match &self.dados {
            Some(Ok(c)) => c.pendentes(&|i| self.editado(i, cx)),
            _ => Vec::new(),
        }
    }

    fn escolher(&mut self, chave: String, valor: String, cx: &mut Context<Self>) {
        self.escolhas.insert(chave, valor);
        self.salvo = false;
        cx.notify();
    }

    fn descartar(&mut self, cx: &mut Context<Self>) {
        self.campos.clear();
        self.escolhas.clear();
        cx.notify();
    }

    fn restaurar(&mut self, i: &Item, window: &mut Window, cx: &mut Context<Self>) {
        match self.campos.get(&i.chave) {
            Some(c) => c.update(cx, |s, cx| s.set_value(i.padrao.clone(), window, cx)),
            None => {
                self.escolhas.insert(i.chave.clone(), i.padrao.clone());
            }
        }
        cx.notify();
    }

    /// Lê a prévia do `plan` no fundo; nada é gravado ainda.
    fn previa(&mut self, cx: &mut Context<Self>) {
        let mudancas = self.pendentes(cx);
        if mudancas.is_empty() {
            return;
        }
        let texto = texto_plano(&mudancas);
        self.gravacao = Some(Gravacao {
            mudancas,
            previa: None,
            gravando: false,
        });
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let e = panel_env(env)?;
                    config::plan(&config::store(&e), &texto, &[])
                        .map(|p| Previa::de(&p))
                        .map_err(|e| e.to_string())
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if let Some(g) = v.gravacao.as_mut() {
                    g.previa = Some(r);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn gravar(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.gravacao.as_mut() else {
            return;
        };
        g.gravando = true;
        let mudancas = g.mudancas.clone();
        let env = self.env.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let e = panel_env(env)?;
                    let pares: Vec<(&str, &str)> = mudancas
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    config::save_validated(&config::store(&e), &pares, &[])
                        .map_err(|e| e.to_string())
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.gravacao = None;
                match r {
                    Ok(_) => {
                        v.salvo = true;
                        v.erro = None;
                        v.recarregar(cx);
                    }
                    Err(e) => v.erro = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn linha(&self, i: &Item, n: usize, cx: &mut Context<Self>) -> SettingRow {
        let rotulo = i.rotulo.clone();
        let atual = self.editado(i, cx).unwrap_or_else(|| i.valor.clone());
        let ent = cx.entity();
        let chave = i.chave.clone();
        let row = match &i.controle {
            Controle::Switch => SettingRow::switch(("cfg", n), rotulo, ligado(&atual)).on_change(
                move |on, _, cx| {
                    let v = if on { "1" } else { "0" }.to_owned();
                    let k = chave.clone();
                    ent.update(cx, |s, cx| s.escolher(k, v, cx));
                },
            ),
            Controle::Select(opcoes) => {
                let sel = Select::new(
                    ("cfg-sel", n),
                    opcoes
                        .iter()
                        .map(|o| SelectOption::new(o.clone(), o.clone())),
                )
                .value(atual.clone())
                .disabled(!i.editavel())
                .on_change(move |v, _, cx| {
                    let (k, v) = (chave.clone(), v.to_string());
                    ent.update(cx, |s, cx| s.escolher(k, v, cx));
                });
                SettingRow::select(("cfg", n), rotulo, sel)
            }
            Controle::Numero(min, max) => match self.campos.get(&i.chave) {
                Some(c) => SettingRow::number(("cfg", n), rotulo, c, *min, *max),
                None => SettingRow::switch(("cfg", n), rotulo, false).disabled(true),
            },
            Controle::Texto => match self.campos.get(&i.chave) {
                Some(c) => SettingRow::text(("cfg", n), rotulo, c),
                None => SettingRow::switch(("cfg", n), rotulo, false).disabled(true),
            },
            Controle::Segredo => {
                SettingRow::switch(("cfg", n), rotulo, !i.valor.is_empty()).disabled(true)
            }
        };
        let item = i.clone();
        let ent = cx.entity();
        let mut desc = i.descricao.clone();
        if !i.aplica.is_empty() {
            desc = if desc.is_empty() {
                format!("Aplica: {}", i.aplica)
            } else {
                format!("{desc} · aplica: {}", i.aplica)
            };
        }
        row.key(i.chave.clone())
            .description(desc)
            .default_value(i.padrao.clone())
            .origin(i.origem)
            .modified(atual != i.valor)
            .disabled(!i.editavel())
            .on_reset(move |_, window, cx| {
                let item = item.clone();
                ent.update(cx, |s, cx| s.restaurar(&item, window, cx));
            })
    }

    fn sheet(&self, g: &Gravacao, cx: &mut Context<Self>) -> Sheet {
        let p = palette(cx);
        let fechar = cx.entity();
        let gravar = cx.entity();
        let pode = matches!(&g.previa, Some(Ok(pv)) if pv.pode_gravar());
        let mut botao = Button::new("cfg-gravar", "Gravar")
            .primary()
            .loading(g.gravando);
        if pode {
            botao = botao.on_click(move |_, _, cx| gravar.update(cx, |v, cx| v.gravar(cx)));
        } else {
            botao = botao.disabled("Nada a gravar ou há valores recusados");
        }
        let linhas = |titulo: &str, l: &[String], cor| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(size::META))
                        .text_color(p.muted_fg)
                        .child(SharedString::from(titulo.to_owned())),
                )
                .children(
                    l.iter()
                        .map(|x| div().text_color(cor).child(SharedString::from(x.clone()))),
                )
        };
        let corpo: AnyElement = match &g.previa {
            None => EmptyState::new("Conferindo os valores…").into_any_element(),
            Some(Err(e)) => EmptyState::new("Não foi possível conferir")
                .hint(e.clone())
                .into_any_element(),
            Some(Ok(pv)) => {
                let mut d = div().flex().flex_col().gap(px(16.0));
                if !pv.mudancas.is_empty() {
                    d = d.child(linhas("Muda", &pv.mudancas, p.fg));
                }
                if !pv.recusas.is_empty() {
                    d = d.child(linhas("Recusado", &pv.recusas, p.bad));
                }
                if !pv.aplicar.is_empty() {
                    d = d.child(linhas("Para aplicar", &pv.aplicar, p.muted_fg));
                }
                d.into_any_element()
            }
        };
        Sheet::new("cfg-previa", "Gravar configurações")
            .width(560.0)
            .footer(botao)
            .on_close(move |_, cx| {
                fechar.update(cx, |v, cx| {
                    v.gravacao = None;
                    cx.notify();
                })
            })
            .child(div().px(px(24.0)).py(px(20.0)).child(corpo))
    }
}

impl Render for ConfigView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.garantir_campos(window, cx);
        let mut header = PageHeader::new("Configurações").description(DESCRICAO);
        let corpo: Vec<AnyElement> = match self.dados.clone() {
            None => vec![EmptyState::new("Lendo as configurações…").into_any_element()],
            Some(Err(e)) => vec![
                EmptyState::new("Sem dados do Hades")
                    .hint(e)
                    .into_any_element(),
            ],
            Some(Ok(c)) => {
                header = header.meta(c.caminho.clone());
                let mut n = 0;
                let mut out: Vec<AnyElement> = Vec::new();
                if let Some(e) = &self.erro {
                    let p = palette(cx);
                    out.push(
                        div()
                            .text_color(p.bad)
                            .child(SharedString::from(format!("Não gravou: {e}")))
                            .into_any_element(),
                    );
                }
                for g in &c.grupos {
                    let mut grupo = SettingsGroup::new(g.nome.clone());
                    for i in &g.itens {
                        grupo = grupo.row(self.linha(i, n, cx));
                        n += 1;
                    }
                    out.push(grupo.into_any_element());
                }
                out
            }
        };
        let pendentes = self.pendentes(cx).len();
        let salvar = cx.entity();
        let descartar = cx.entity();
        let barra = SettingsBar::new(pendentes)
            .saving(self.gravacao.as_ref().is_some_and(|g| g.gravando))
            .saved(self.salvo && pendentes == 0)
            .on_save(move |_, _, cx| salvar.update(cx, |v, cx| v.previa(cx)))
            .on_discard(move |_, _, cx| descartar.update(cx, |v, cx| v.descartar(cx)));
        let sheet = self.gravacao.as_ref().map(|g| self.sheet(g, cx));
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(Page::new("config").header(header).children(corpo)),
            )
            .child(barra)
            .children(sheet)
    }
}
