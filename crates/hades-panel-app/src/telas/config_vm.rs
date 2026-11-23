//! View-model da tela Configurações (`kit/panel/ui/pages/config.tsx`), sem gpui: o catálogo de
//! `hades_panel_core::config::state_json` em grupos, com o controle e a origem de cada valor, e o que mudou.

use hades_ui::components::SettingOrigin;
use serde_json::Value;

fn texto(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// O controle de um item, pelo `type` do catálogo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Controle {
    Switch,
    Select(Vec<String>),
    Numero(i64, i64),
    Texto,
    /// Segredo: o painel não mostra nem edita.
    Segredo,
}

impl Controle {
    pub fn de(tipo: &str) -> Self {
        if let Some(opcoes) = tipo.strip_prefix("enum:") {
            return Self::Select(opcoes.split(',').map(str::to_owned).collect());
        }
        if let Some(faixa) = tipo.strip_prefix("int:") {
            let mut f = faixa.split(':').map(str::parse::<i64>);
            if let (Some(Ok(min)), Some(Ok(max))) = (f.next(), f.next()) {
                return Self::Numero(min, max);
            }
            return Self::Texto;
        }
        match tipo {
            "bool" => Self::Switch,
            "secret" => Self::Segredo,
            _ => Self::Texto,
        }
    }
}

/// O valor de um `bool` gravado (`1`/`0`, como o `validate` do core normaliza).
pub fn ligado(valor: &str) -> bool {
    matches!(valor, "1" | "true" | "on" | "sim")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub chave: String,
    pub rotulo: String,
    pub descricao: String,
    pub aplica: String,
    pub padrao: String,
    pub valor: String,
    pub controle: Controle,
    pub origem: SettingOrigin,
}

impl Item {
    /// `env_raw` diz o valor que o ambiente do processo define para a chave: ele vence o arquivo e deixa o campo só
    /// leitura.
    pub fn de(i: &Value, env_raw: &dyn Fn(&str) -> Option<String>) -> Self {
        let chave = texto(i, "key");
        let changed = i.get("changed").and_then(Value::as_bool).unwrap_or(false);
        let (origem, valor) = match env_raw(&chave) {
            Some(v) => (SettingOrigin::Env, v),
            None if changed => (SettingOrigin::File, texto(i, "value")),
            None => (SettingOrigin::Default, texto(i, "value")),
        };
        Self {
            rotulo: Some(texto(i, "label"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| chave.clone()),
            descricao: texto(i, "desc"),
            aplica: texto(i, "apply"),
            padrao: texto(i, "default"),
            controle: Controle::de(&texto(i, "type")),
            chave,
            valor,
            origem,
        }
    }

    pub fn editavel(&self) -> bool {
        self.origem != SettingOrigin::Env && self.controle != Controle::Segredo
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grupo {
    pub nome: String,
    pub itens: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub caminho: String,
    pub grupos: Vec<Grupo>,
}

impl Config {
    pub fn de(estado: &Value, env_raw: &dyn Fn(&str) -> Option<String>) -> Self {
        let mut grupos: Vec<Grupo> = Vec::new();
        for i in estado
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let nome = Some(texto(i, "group"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Outros".to_owned());
            let item = Item::de(i, env_raw);
            match grupos.iter_mut().find(|g| g.nome == nome) {
                Some(g) => g.itens.push(item),
                None => grupos.push(Grupo {
                    nome,
                    itens: vec![item],
                }),
            }
        }
        Self {
            caminho: texto(estado, "path"),
            grupos,
        }
    }

    pub fn itens(&self) -> impl Iterator<Item = &Item> {
        self.grupos.iter().flat_map(|g| g.itens.iter())
    }

    /// As mudanças pendentes: o valor editado de cada chave editável que difere do gravado.
    pub fn pendentes(&self, editado: &dyn Fn(&Item) -> Option<String>) -> Vec<(String, String)> {
        self.itens()
            .filter(|i| i.editavel())
            .filter_map(|i| {
                editado(i)
                    .filter(|v| *v != i.valor)
                    .map(|v| (i.chave.clone(), v))
            })
            .collect()
    }
}

/// As mudanças como o `KEY=VALUE` que o `plan` do core lê.
pub fn texto_plano(mudancas: &[(String, String)]) -> String {
    mudancas.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

/// A prévia do `plan`, em linhas: o que muda e o que o core recusou.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Previa {
    pub mudancas: Vec<String>,
    pub recusas: Vec<String>,
    pub aplicar: Vec<String>,
}

impl Previa {
    pub fn de(p: &hades_panel_core::config::ImportPlan) -> Self {
        let mut recusas: Vec<String> = p.invalid.iter().map(|(k, e)| format!("{k}: {e}")).collect();
        recusas.extend(p.unknown.iter().map(|k| format!("{k}: chave desconhecida")));
        recusas.extend(
            p.secret
                .iter()
                .map(|k| format!("{k}: segredo, não se grava pelo painel")),
        );
        recusas.extend(p.blocked.iter().map(|k| format!("{k}: só local")));
        Self {
            mudancas: p
                .changes
                .iter()
                .map(|c| {
                    let antes = if c.old.is_empty() {
                        "(vazio)"
                    } else {
                        c.old.as_str()
                    };
                    format!("{} ({}): {antes} → {}", c.label, c.key, c.new)
                })
                .collect(),
            recusas,
            aplicar: p.apply.clone(),
        }
    }

    pub fn pode_gravar(&self) -> bool {
        !self.mudancas.is_empty() && self.recusas.is_empty()
    }
}

#[cfg(test)]
#[path = "config_vm_tests.rs"]
mod tests;
