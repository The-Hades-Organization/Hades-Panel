# Hades-Panel

O painel do Hades como app nativo em Rust, com [gpui](https://www.gpui.rs) e o Hades-UI. Lê os mesmos arquivos que o
painel web e não depende dele estar no ar.

**Estado:** a fase 5 está entregue. O app tem a casca com o trilho em grupos, a barra de estado e a paleta Ctrl K, e
todas as páginas do painel web: Visão geral, Configurações, Onboarding, Documentação, Loop, Orquestração, Revisões,
Fila de gates, Consumo, Herdr, Execuções, Logs, Notificações, Plugins, Extensões, Qualidade, Branch e PRs, e o Argos. As
ferramentas linkadas no binário (Moirai, Thanatos, Caronte, Cerberus e Minos) abrem dentro da janela, cada uma no seu
grupo do trilho.

Fica só no painel web: o chat com o Claude, o acesso remoto pelo túnel, a página de um plugin e o terminal do Herdr.

## Crates

| Crate | O que faz |
|---|---|
| `hades-panel-core` | Leitores do núcleo do painel sobre o `hades-core`, sem gpui. `PanelEnv` diz de onde ler: `HADES_DATA`, `HADES_WORK_ROOT`, `HADES_PROC_ROOT` e `HADES_NOW`. |
| `hades-panel-cli` | Binário `hades-panel`. `hades-panel dump <rota>` imprime o JSON de uma rota, como o `hades panel dump` da skill. |
| `hades-panel-app` | A janela gpui. |

## Como rodar

```bash
cargo test --workspace --exclude hades-panel-app
HADES_DATA=/caminho/dos/dados cargo run -p hades-panel-cli -- dump config
cargo run -p hades-panel-app            # --dark para o tema escuro
```

O app aceita flags de abertura: `--tela <página>` (o id da página, como `queue` ou `usage`), `--ferramenta
<ferramenta>:<tela>` (por exemplo `cerberus:vereditos`), `--detalhe` (abre o primeiro gate da Fila) e `--paleta`.

No Linux, o app precisa das bibliotecas de `scripts/linux-deps.sh`.

## Paridade

`parity/cases.toml` é a matriz que compara `hades-panel dump` com o `hades panel dump` da skill sobre as mesmas
fixtures. O job `parity` do CI roda a matriz contra o sha do Hades-Skill fixado no workflow.

## Licença

MIT.
