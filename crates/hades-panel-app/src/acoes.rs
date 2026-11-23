//! Ações do painel por subprocesso do CLI (plano F5 painel, decisão 2): `hades panel action --list --json` dá o
//! catálogo e `hades panel action <id> [op=<op>] --json` lança o job. O parse da saída não depende de gpui.

use std::process::Command;

use hades_ui::IconName;
use hades_ui::components::PaletteItem;
use serde_json::Value;

/// Quando a ação pede confirmação antes de lançar (`confirm` de `ActionInfo` em `src/panel/action-cli.ts:15-22`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirma {
    Nunca,
    Sempre,
    /// Só estes `op`.
    Ops(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acao {
    pub id: String,
    pub rotulo: String,
    /// `op` aceitos; vazio quando a ação não leva op.
    pub ops: Vec<String>,
    pub confirma: Confirma,
}

impl Acao {
    pub fn pede_confirmacao(&self, op: Option<&str>) -> bool {
        match &self.confirma {
            Confirma::Nunca => false,
            Confirma::Sempre => true,
            Confirma::Ops(ops) => op.is_some_and(|op| ops.iter().any(|o| o == op)),
        }
    }
}

/// Uma ação escolhida: id e, se a ação leva, o `op`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pedido {
    pub id: String,
    pub op: Option<String>,
}

impl Pedido {
    /// Argumentos `k=v` do CLI.
    pub fn args(&self) -> Vec<String> {
        self.op.iter().map(|op| format!("op={op}")).collect()
    }
}

/// O que o CLI respondeu ao lançar (`panelAction` de `src/panel/action-cli.ts:98-103` e `commands/panel.ts:53-66`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resultado {
    /// rc 0: `{job, title, exclusive}`.
    Lancada {
        job: String,
        titulo: String,
        exclusiva: bool,
    },
    /// rc 1: `{error, code}`.
    Recusada { erro: String, codigo: String },
    /// rc 2 (ação desconhecida) ou CLI sem `panel action --json`.
    Indisponivel,
    /// O CLI não rodou.
    Falhou(String),
}

/// Mensagem de quando a skill instalada não conhece a ação.
pub const INDISPONIVEL: &str = "Ação indisponível nesta versão da skill: atualize o Hades.";

/// O catálogo de `hades panel action --list --json`; saída que não é a lista (skill antiga imprime o uso) vira erro.
pub fn lista_de(saida: &str) -> Result<Vec<Acao>, String> {
    let v: Value = serde_json::from_str(saida.trim()).map_err(|_| INDISPONIVEL.to_owned())?;
    let itens = v.as_array().ok_or_else(|| INDISPONIVEL.to_owned())?;
    Ok(itens.iter().filter_map(acao).collect())
}

fn acao(v: &Value) -> Option<Acao> {
    let texto = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let lista = |x: Option<&Value>| -> Vec<String> {
        x.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let id = texto("id")?;
    let confirma = match v.get("confirm") {
        Some(Value::Bool(true)) => Confirma::Sempre,
        Some(Value::Array(_)) => Confirma::Ops(lista(v.get("confirm"))),
        _ => Confirma::Nunca,
    };
    Some(Acao {
        rotulo: texto("label").unwrap_or_else(|| id.clone()),
        ops: lista(v.get("ops")),
        id,
        confirma,
    })
}

/// A resposta de `hades panel action <id> --json` pelo código de saída.
pub fn resultado(rc: Option<i32>, saida: &str) -> Resultado {
    let v: Option<Value> = serde_json::from_str(saida.trim()).ok();
    let campo = |k: &str| {
        v.as_ref()
            .and_then(|v| v.get(k))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match (rc, v.is_some()) {
        (Some(0), true) if campo("job").is_some() => Resultado::Lancada {
            job: campo("job").unwrap_or_default(),
            titulo: campo("title").unwrap_or_default(),
            exclusiva: v
                .as_ref()
                .and_then(|v| v.get("exclusive"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        (Some(1), true) => Resultado::Recusada {
            erro: campo("error").unwrap_or_default(),
            codigo: campo("code").unwrap_or_default(),
        },
        (Some(0 | 2), _) => Resultado::Indisponivel,
        (Some(rc), _) => Resultado::Falhou(format!("hades panel action saiu com {rc}")),
        (None, _) => Resultado::Falhou("hades panel action encerrado por sinal".into()),
    }
}

const ACAO: &str = "acao:";

/// Um item da paleta por ação, e um por `op` quando a ação leva op.
pub fn itens_paleta(acoes: &[Acao]) -> Vec<PaletteItem> {
    let mut out = Vec::new();
    for a in acoes {
        if a.ops.is_empty() {
            out.push(PaletteItem::new(
                format!("{ACAO}{}", a.id),
                "Ações",
                a.rotulo.clone(),
                IconName::Play,
            ));
        }
        for op in &a.ops {
            out.push(PaletteItem::new(
                format!("{ACAO}{}?{op}", a.id),
                "Ações",
                format!("{}: {op}", a.rotulo),
                IconName::Play,
            ));
        }
    }
    out
}

/// O pedido de um id da paleta montado por [`itens_paleta`].
pub fn pedido(id: &str) -> Option<Pedido> {
    let r = id.strip_prefix(ACAO)?;
    Some(match r.split_once('?') {
        Some((id, op)) => Pedido {
            id: id.to_owned(),
            op: Some(op.to_owned()),
        },
        None => Pedido {
            id: r.to_owned(),
            op: None,
        },
    })
}

fn hades() -> Command {
    let mut c = Command::new("hades");
    c.args(["panel", "action"]);
    c
}

/// Roda `hades panel action --list --json`. Bloqueia: chame no executor de fundo.
pub fn listar() -> Result<Vec<Acao>, String> {
    let out = hades()
        .args(["--list", "--json"])
        .output()
        .map_err(|e| format!("hades não encontrado no PATH: {e}"))?;
    lista_de(&String::from_utf8_lossy(&out.stdout))
}

/// Roda `hades panel action <id> [op=<op>] --json`. Bloqueia: chame no executor de fundo.
pub fn lancar(p: &Pedido) -> Resultado {
    match hades().arg(&p.id).args(p.args()).arg("--json").output() {
        Ok(out) => resultado(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        Err(e) => Resultado::Falhou(format!("hades não encontrado no PATH: {e}")),
    }
}

#[cfg(test)]
#[path = "acoes_tests.rs"]
mod tests;
