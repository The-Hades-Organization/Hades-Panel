# Hades-Panel

O painel do Hades como app nativo em Rust, com [gpui](https://www.gpui.rs) e o Hades-UI. Lê os mesmos arquivos que o
painel web e não depende dele estar no ar.

**Estado:** em construção. Por enquanto há o esqueleto: a janela abre com o trilho e a marca, e o `dump` ainda não
conhece nenhuma rota.

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

No Linux, o app precisa das bibliotecas de `scripts/linux-deps.sh`.

## Paridade

`parity/cases.toml` é a matriz que compara `hades-panel dump` com o `hades panel dump` da skill sobre as mesmas
fixtures. O job `parity` do CI roda a matriz contra o sha do Hades-Skill fixado no workflow.

## Licença

MIT.
