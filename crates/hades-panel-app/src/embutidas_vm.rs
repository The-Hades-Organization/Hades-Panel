//! View-model das ferramentas embutidas, sem gpui: os grupos do trilho com as telas de cada ferramenta ligada, os
//! itens da paleta (ir para uma tela, rodar um comando) e o caminho de volta do rótulo ou do id escolhido.

use hades_ui::components::PaletteItem;
use hades_ui::{FerramentaInfo, IconName, TelaInfo};

/// Uma ferramenta linkada no binário: o contrato dela e as telas que o painel mostra (sem Configurações, que vêm do
/// painel, plano F5 painel §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ligada {
    pub info: FerramentaInfo,
    pub telas: &'static [TelaInfo],
}

/// Um item do trilho que abre a tela `tela` da ferramenta `ferramenta`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemTela {
    pub ferramenta: &'static str,
    pub tela: &'static str,
    /// Único no trilho: o trilho devolve o rótulo do item clicado.
    pub rotulo: String,
    pub icone: IconName,
}

/// Um grupo do trilho por ferramenta, com o título dela.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrupoFerramenta {
    pub titulo: &'static str,
    pub itens: Vec<ItemTela>,
}

/// O que um item escolhido na paleta pede.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino<'a> {
    Tela {
        ferramenta: &'a str,
        tela: &'a str,
    },
    Comando {
        ferramenta: &'a str,
        comando: &'a str,
    },
}

const IR_TELA: &str = "tela:";
const RODAR: &str = "cmd:";

/// Grupos do trilho, na ordem de `ligadas`. Rótulo que repete um de `ocupados` (as páginas do painel) ou de uma
/// ferramenta anterior ganha o nome da ferramenta: "Histórico · Moirai".
pub fn grupos(ligadas: &[Ligada], ocupados: &[&str]) -> Vec<GrupoFerramenta> {
    let mut usados: Vec<String> = ocupados.iter().map(|s| (*s).to_owned()).collect();
    ligadas
        .iter()
        .map(|l| {
            let itens = l
                .telas
                .iter()
                .map(|t| {
                    let rotulo = if usados.iter().any(|u| u == t.rotulo) {
                        format!("{} · {}", t.rotulo, l.info.titulo)
                    } else {
                        t.rotulo.to_owned()
                    };
                    usados.push(rotulo.clone());
                    ItemTela {
                        ferramenta: l.info.id,
                        tela: t.id,
                        rotulo,
                        icone: t.icone,
                    }
                })
                .collect();
            GrupoFerramenta {
                titulo: l.info.titulo,
                itens,
            }
        })
        .collect()
}

/// O item do trilho de rótulo `rotulo`, se for de uma ferramenta.
pub fn por_rotulo<'a>(grupos: &'a [GrupoFerramenta], rotulo: &str) -> Option<&'a ItemTela> {
    grupos
        .iter()
        .flat_map(|g| g.itens.iter())
        .find(|i| i.rotulo == rotulo)
}

/// O item do trilho da tela `tela` da ferramenta `ferramenta`.
pub fn item<'a>(
    grupos: &'a [GrupoFerramenta],
    ferramenta: &str,
    tela: &str,
) -> Option<&'a ItemTela> {
    grupos
        .iter()
        .flat_map(|g| g.itens.iter())
        .find(|i| i.ferramenta == ferramenta && i.tela == tela)
}

/// Itens da paleta Ctrl K: as telas de cada ferramenta (grupo com o título dela) e os comandos dela (grupo
/// "<Ferramenta> · <grupo do comando>"), com descrição e atalho.
pub fn itens_paleta(ligadas: &[Ligada]) -> Vec<PaletteItem> {
    let mut out = Vec::new();
    for l in ligadas {
        let i = &l.info;
        for t in l.telas {
            out.push(
                PaletteItem::new(
                    format!("{IR_TELA}{}/{}", i.id, t.id),
                    i.titulo,
                    format!("{}: {}", i.titulo, t.rotulo),
                    t.icone,
                )
                .desc(format!("Abrir a tela {} do {}", t.rotulo, i.titulo)),
            );
        }
        for c in i.comandos {
            let mut item = PaletteItem::new(
                format!("{RODAR}{}/{}", i.id, c.id),
                format!("{} · {}", i.titulo, c.grupo),
                c.rotulo,
                c.icone,
            );
            if let Some(d) = c.descricao {
                item = item.desc(d);
            }
            if let Some(a) = c.atalho {
                item = item.shortcut(a);
            }
            out.push(item);
        }
    }
    out
}

/// O destino de um id da paleta montado por [`itens_paleta`]; `None` para os outros ids.
pub fn destino(id: &str) -> Option<Destino<'_>> {
    if let Some(r) = id.strip_prefix(IR_TELA) {
        let (ferramenta, tela) = r.split_once('/')?;
        return Some(Destino::Tela { ferramenta, tela });
    }
    let (ferramenta, comando) = id.strip_prefix(RODAR)?.split_once('/')?;
    Some(Destino::Comando {
        ferramenta,
        comando,
    })
}

#[cfg(test)]
#[path = "embutidas_vm_tests.rs"]
mod tests;
