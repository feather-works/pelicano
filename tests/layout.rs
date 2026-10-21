//! Layout, eventos e captura sem janela.
use pelicano::*;
use pelicano::column;

fn montar<M>(el: &Element<'_, M>, size: Size, r: &mut Renderer, st: &States) -> Node {
    r.begin(size, 1.0, Color::TRANSPARENT);
    layout(el, size, r, st, &Theme::dark())
}

#[test]
fn fill_divide_a_sobra_pela_proporcao() {
    let el: Element<()> = row![Space::new(100.0, 10.0), Space::new(Length::Fill(1), 10.0), Space::new(Length::Fill(3), 10.0)].spacing(10).padding(5.0).width(Length::FILL).into();
    let n = montar(&el, Size::new(525.0, 100.0), &mut Renderer::new(), &States::default());
    let w: Vec<f32> = n.children.iter().map(|c| c.rect.width).collect();
    // 525 - 10 de padding - 20 de espaço - 100 fixo = 395 para dividir 1:3.
    assert_eq!(w, vec![100.0, 98.0, 296.0]);
    assert_eq!(n.children[1].rect.x, 5.0 + 100.0 + 10.0);
}

#[test]
fn coluna_sem_largura_herda_fill_do_filho() {
    // Abas do monitor: [botão + traço FILL] ×3. Antes a primeira tomava tudo e as outras ficavam com 0.
    let aba = || column![Space::new(60.0, 30.0), container(Space::new(Length::FILL, 3.0)).width(Length::FILL)];
    let el: Element<()> = row![aba(), aba(), aba()].spacing(10).into();
    let n = montar(&el, Size::new(320.0, 100.0), &mut Renderer::new(), &States::default());
    let w: Vec<f32> = n.children.iter().map(|c| c.rect.width).collect();
    assert_eq!(w, vec![100.0, 100.0, 100.0]);
}

#[test]
fn margem_fica_fora_do_padding() {
    let el: Element<()> = container(Space::new(50.0, 20.0)).padding(4.0).margin([10u16, 20]).into();
    let n = montar(&el, Size::new(500.0, 500.0), &mut Renderer::new(), &States::default());
    assert_eq!(n.rect.size(), Size::new(50.0 + 8.0 + 40.0, 20.0 + 8.0 + 20.0));
    assert_eq!((n.children[0].rect.x, n.children[0].rect.y), (24.0, 14.0));
}

#[test]
fn botao_publica_no_clique_e_campo_edita() {
    #[derive(Clone, Debug, PartialEq)]
    enum M {
        Ok,
        Texto(String),
    }
    let el: Element<M> = column![button("ok").on_press(M::Ok), text_input("", "ab").on_input(M::Texto)].into();
    let (mut r, mut st) = (Renderer::new(), States::default());
    let n = montar(&el, Size::new(300.0, 200.0), &mut r, &st);
    let mut msgs = Vec::new();
    let mut clicar = |p: Point, evs: &[Event], msgs: &mut Vec<M>, st: &mut States| {
        for ev in evs {
            let mut cx = EventCx { renderer: &mut r, states: st, cursor: Some(p), messages: msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut MemoryClipboard::default() };
            dispatch(&el, &n, ev, &mut cx);
        }
    };
    let b = n.children[0].rect;
    clicar(Point::new(b.x + 2.0, b.y + 2.0), &[Event::MousePressed(MouseButton::Left), Event::MouseReleased(MouseButton::Left)], &mut msgs, &mut st);
    assert_eq!(msgs, vec![M::Ok]);
    let i = n.children[1].rect;
    let tecla = |t: &str| Event::Key { key: Key::Character(t.into()), text: Some(t.into()), modifiers: Modifiers::empty(), pressed: true };
    let fim = Event::Key { key: Key::Named(NamedKey::End), text: None, modifiers: Modifiers::empty(), pressed: true };
    msgs.clear();
    clicar(Point::new(i.x + 1.0, i.y + 4.0), &[Event::MousePressed(MouseButton::Left), fim, tecla("ç")], &mut msgs, &mut st);
    assert_eq!(msgs, vec![M::Texto("abç".into())]);
}

#[test]
fn captura_gera_png() {
    let el: Element<()> = container(text("olá")).padding(8.0).style(|t| Style::glass(8.0).text_color(t.text)).into();
    let png = capture(&el, Size::new(120.0, 60.0), 2.0, &Theme::dark(), &mut Renderer::new(), None);
    assert_eq!(&png[1..4], b"PNG");
    assert_eq!(Color::hex("#ff000080"), Some(Color::rgb(1.0, 0.0, 0.0).alpha(128.0 / 255.0)));
}

#[test]
fn palavra_que_nao_cabe_nao_vira_coluna_de_letras() {
    let mut r = Renderer::new();
    r.begin(Size::new(500.0, 500.0), 1.0, Color::TRANSPARENT);
    let palavra = r.measure("Substituir", 11.0, Font::DEFAULT, None);
    // Numa caixa de 20, a palavra fica inteira numa linha e com a largura dela.
    let el: Element<()> = container(text("Substituir").size(11)).width(20.0).into();
    let n = montar(&el, Size::new(500.0, 500.0), &mut r, &States::default());
    assert_eq!(n.children[0].rect.size(), palavra);
    // Rótulo de botão não quebra nem entre palavras.
    let el: Element<()> = container(button(text("Selecionar tudo").size(11)).padding(0.0)).width(40.0).into();
    let n = montar(&el, Size::new(500.0, 500.0), &mut r, &States::default());
    assert_eq!(n.children[0].rect.height, palavra.height);
}

#[test]
fn reticencias_cortam_o_que_nao_cabe() {
    let el: Element<()> = container(text("Verificar ortografia").ellipsis()).width(40.0).into();
    let mut r = Renderer::new();
    let n = montar(&el, Size::new(500.0, 500.0), &mut r, &States::default());
    assert_eq!(n.children[0].rect.width, 40.0);
    draw(&el, &n, &mut r, &States::default(), &Theme::dark(), None, false);
}

#[test]
fn max_height_limita_contener_e_rolagem() {
    let alto = || Space::new(50.0, 900.0);
    let el: Element<()> = container(scrollable(alto()).height(Length::Shrink)).max_height(300.0).into();
    let n = montar(&el, Size::new(500.0, 800.0), &mut Renderer::new(), &States::default());
    assert_eq!(n.rect.height, 300.0);
    let el: Element<()> = scrollable(alto()).height(Length::Shrink).max_height(200.0).into();
    assert_eq!(montar(&el, Size::new(500.0, 800.0), &mut Renderer::new(), &States::default()).rect.height, 200.0);
    // Baixo, cresce com o conteúdo.
    let el: Element<()> = scrollable(Space::new(50.0, 80.0)).height(Length::Shrink).max_height(200.0).into();
    assert_eq!(montar(&el, Size::new(500.0, 800.0), &mut Renderer::new(), &States::default()).rect.height, 80.0);
}

#[test]
fn barra_com_espaco_fica_ao_lado_do_conteudo() {
    let el: Element<()> = scrollable(Space::new(Length::FILL, 900.0)).width(182.0).height(300.0).scrollbar_width(6.0).scrollbar_spacing(4.0).into();
    let n = montar(&el, Size::new(500.0, 800.0), &mut Renderer::new(), &States::default());
    assert_eq!(n.children[0].rect.width, 182.0 - 10.0);
}
