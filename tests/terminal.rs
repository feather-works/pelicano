//! O terminal desenhado: cores, negrito e o cursor.
#![cfg(feature = "terminal")]
use pelicano::column;
use pelicano::*;

#[test]
fn desenha_cores_e_texto() {
    let script = "printf 'normal \\033[1mnegrito\\033[0m \\033[32mverde\\033[0m \\033[41m fundo \\033[0m \\033[38;2;255;128;0mlaranja\\033[0m\\nsegunda linha: ação é ü\\n'; sleep 5";
    let t = Terminal::new(TerminalSettings { program: Some("/bin/sh".into()), args: vec!["-c".into(), script.into()], ..Default::default() }).expect("pty");
    let el: Element<()> = terminal(&t).size(14.0).into();
    let mut r = Renderer::new();
    let tela = Size::new(640.0, 120.0);
    // O primeiro desenho acerta o tamanho do PTY; depois espera a saída.
    capture(&el, tela, 1.0, &Theme::dark(), &mut r, None);
    let fim = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !t.screen_text().contains("ü") && std::time::Instant::now() < fim {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(t.screen_text().contains("segunda linha: ação é ü"), "{:?}", t.screen_text());
    let png = capture(&el, tela, 1.0, &Theme::dark(), &mut r, None);
    if let Ok(d) = std::env::var("PELICANO_PNG") {
        std::fs::write(format!("{d}/terminal.png"), png).unwrap();
    }
}

#[test]
fn dentro_da_rolagem_tem_tamanho_finito() {
    let t = Terminal::new(TerminalSettings { program: Some("/bin/sh".into()), args: vec!["-c".into(), "sleep 5".into()], ..Default::default() }).expect("pty");
    let el: Element<()> = scrollable(column![text("x"), terminal(&t)]).into();
    let mut r = Renderer::new();
    let tela = Size::new(640.0, 120.0);
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &States::default(), &Theme::dark());
    assert!(n.children[0].rect.height.is_finite());
    // O desenho redimensiona o PTY: com altura infinita pedia usize::MAX linhas.
    capture(&el, tela, 1.0, &Theme::dark(), &mut r, None);
}
