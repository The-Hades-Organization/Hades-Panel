use hades_panel_app::Abertura;
use hades_ui::ThemeMode;

fn valor_de(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn tem(flag: &str) -> bool {
    std::env::args().any(|a| a == flag)
}

/// `--tela <id da página>`, `--ferramenta <ferramenta>:<tela>`, `--paleta` e `--detalhe` (primeiro gate da Fila).
fn main() {
    let modo = if tem("--dark") {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    let abertura = Abertura {
        pagina: valor_de("--tela"),
        ferramenta: valor_de("--ferramenta")
            .and_then(|f| f.split_once(':').map(|(a, b)| (a.to_owned(), b.to_owned()))),
        paleta: tem("--paleta"),
        detalhe: tem("--detalhe"),
    };
    gpui_platform::application().run(move |cx| {
        hades_ui::init(cx, modo);
        hades_panel_app::abrir_janela_com(cx, abertura);
    });
}
