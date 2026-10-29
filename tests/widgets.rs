//! Cada widget recebendo eventos de verdade, e os pixels do recorte e do vidro.
use pelicano::column;
use pelicano::*;

const TELA: Size = Size::new(400.0, 300.0);

/// Monta a árvore de novo a cada evento, como a janela faz.
struct H {
    r: Renderer,
    st: States,
    area: MemoryClipboard,
}

impl H {
    fn new() -> Self {
        H { r: Renderer::new(), st: States::default(), area: MemoryClipboard::default() }
    }
    fn node<M>(&mut self, el: &Element<'_, M>) -> Node {
        self.r.begin(TELA, 1.0, Color::TRANSPARENT);
        layout(el, TELA, &mut self.r, &self.st, &Theme::dark())
    }
    fn ev<M>(&mut self, el: &Element<'_, M>, p: Point, ev: Event) -> (Vec<M>, Captured) {
        let n = self.node(el);
        let mut msgs = Vec::new();
        let mut cx = EventCx { renderer: &mut self.r, states: &mut self.st, cursor: Some(p), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut self.area };
        let c = dispatch(el, &n, &ev, &mut cx);
        (msgs, c)
    }
    fn click<M>(&mut self, el: &Element<'_, M>, p: Point) -> Vec<M> {
        let mut m = self.ev(el, p, Event::MousePressed(MouseButton::Left)).0;
        m.extend(self.ev(el, p, Event::MouseReleased(MouseButton::Left)).0);
        m
    }
    fn tecla<M>(&mut self, el: &Element<'_, M>, k: Key, texto: Option<&str>) -> Vec<M> {
        self.ev(el, Point::ORIGIN, Event::Key { key: k, text: texto.map(Into::into), modifiers: Modifiers::empty(), pressed: true }).0
    }
    fn desenhar<M>(&mut self, el: &Element<'_, M>, cursor: Option<Point>) -> CursorIcon {
        let n = self.node(el);
        draw(el, &n, &mut self.r, &self.st, &Theme::dark(), cursor, false)
    }
}

fn centro(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

fn nomeada(n: NamedKey) -> Key {
    Key::Named(n)
}

#[derive(Debug, Clone, PartialEq)]
enum M {
    Clique(usize),
    Escolha(&'static str),
    Valor(f32),
    Marcado(bool),
    Texto(String),
    Enviar,
    Duplo,
}

// ---------- Rolagem ----------

fn lista<'a>() -> Element<'a, M> {
    let itens = (0..20).map(|i| button(text(format!("{i}"))).on_press(M::Clique(i)).height(50.0).width(Length::FILL).into());
    scrollable(column(itens)).height(200.0).width(300.0).into()
}

fn deslocamento(h: &mut H, el: &Element<M>) -> f32 {
    let n = h.node(el);
    n.rect.y - n.children[0].rect.y
}

#[test]
fn rolagem_pela_roda_respeita_os_limites() {
    let (mut h, el) = (H::new(), lista());
    let dentro = Point::new(100.0, 100.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: -120.0, modifiers: Modifiers::empty() });
    assert_eq!(deslocamento(&mut h, &el), 120.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: -100_000.0, modifiers: Modifiers::empty() });
    // 20 itens de 50 = 1000, menos os 200 visíveis.
    assert_eq!(deslocamento(&mut h, &el), 800.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: 100_000.0, modifiers: Modifiers::empty() });
    assert_eq!(deslocamento(&mut h, &el), 0.0);
    // Roda com o mouse fora não rola.
    h.ev(&el, Point::new(350.0, 100.0), Event::Wheel { dx: 0.0, dy: -120.0, modifiers: Modifiers::empty() });
    assert_eq!(deslocamento(&mut h, &el), 0.0);
}

#[test]
fn rolagem_pelo_polegar_e_clique_no_item_certo() {
    let (mut h, el) = (H::new(), lista());
    // O polegar fica colado na direita, no topo: arrasta até embaixo.
    let polegar = Point::new(300.0 - 5.0, 10.0);
    let (_, c) = h.ev(&el, polegar, Event::MousePressed(MouseButton::Left));
    assert_eq!(c, Captured::Yes);
    h.ev(&el, Point::new(295.0, 400.0), Event::CursorMoved(Point::new(295.0, 400.0)));
    h.ev(&el, Point::new(295.0, 400.0), Event::MouseReleased(MouseButton::Left));
    assert_eq!(deslocamento(&mut h, &el), 800.0);
    // Rolado até o fim, o topo da área mostra o item 16.
    assert_eq!(h.click(&el, Point::new(100.0, 10.0)), vec![M::Clique(16)]);
}

#[test]
fn clique_no_conteudo_escondido_nao_vale() {
    let (mut h, el) = (H::new(), column![lista(), Space::new(10.0, 900.0)].into());
    // O item 19 está em y = 950, fora da janela de 200 da rolagem.
    assert_eq!(h.click(&el, Point::new(100.0, 960.0)), vec![]);
}

// ---------- Dropdown ----------

const OPCOES: [&str; 3] = ["a", "b", "c"];

#[test]
fn dropdown_com_estilo_e_padding() {
    let mut h = H::new();
    let normal: Element<M> = pick_list(&OPCOES[..], None, M::Escolha).width(120.0).into();
    let alto = h.node(&normal).rect.height;
    let el: Element<M> = pick_list(&OPCOES[..], None, M::Escolha).width(120.0).padding([2, 6]).style(|_, _| Style::default().background(Color::rgb8(255, 0, 0))).into();
    let campo = h.node(&el).rect;
    assert_eq!(campo.height, alto - 10.0, "padding 2 em cima e embaixo no lugar de 7");
    let mut r = Renderer::new();
    let st = States::default();
    let n = layout(&el, Size::new(200.0, 60.0), &mut r, &st, &Theme::dark());
    r.begin(Size::new(200.0, 60.0), 1.0, Color::BLACK);
    draw(&el, &n, &mut r, &st, &Theme::dark(), None, false);
    let px = r.pixmap.pixel(60, (campo.height / 2.0) as u32).expect("dentro");
    assert_eq!((px.red(), px.green(), px.blue()), (255, 0, 0), "o fundo vem do estilo");
}

#[test]
fn dropdown_abre_escolhe_e_fecha() {
    let mut h = H::new();
    let el: Element<M> = pick_list(&OPCOES[..], None, M::Escolha).width(120.0).into();
    let campo = h.node(&el).rect;
    let item = |i: f32| Point::new(campo.x + 20.0, campo.y + campo.height + 4.0 + 4.0 + 30.0 * i + 15.0);
    // Fechado, o lugar do menu não responde.
    assert_eq!(h.click(&el, item(1.0)), vec![]);
    assert_eq!(h.click(&el, centro(campo)), vec![]);
    assert_eq!(h.click(&el, item(1.0)), vec![M::Escolha("b")]);
    // Escolher fecha: o mesmo clique de novo não faz nada.
    assert_eq!(h.click(&el, item(2.0)), vec![]);
    // Esc fecha sem escolher.
    h.click(&el, centro(campo));
    h.tecla(&el, nomeada(NamedKey::Escape), None);
    assert_eq!(h.click(&el, item(0.0)), vec![]);
    // Clique fora fecha sem escolher.
    h.click(&el, centro(campo));
    assert_eq!(h.click(&el, Point::new(390.0, 290.0)), vec![]);
    assert_eq!(h.click(&el, item(0.0)), vec![]);
}

#[test]
fn menu_aberto_por_cima_de_quem_vem_antes_ganha_o_clique() {
    // Sem espaço embaixo o menu abre para cima, sobre um botão que vem antes na árvore (as abas).
    let mut h = H::new();
    // Dentro de uma rolagem, como numa página: ela também não pode engolir o clique no menu.
    let el: Element<M> = scrollable(column![Space::new(1.0, 170.0), button("aba").on_press(M::Clique(0)).width(200.0).height(40.0), pick_list(&OPCOES[..], None, M::Escolha).width(120.0)]).into();
    let campo = h.node(&el).children[0].children[2].rect;
    h.click(&el, centro(campo));
    h.desenhar(&el, None);
    let sob_o_menu = Point::new(20.0, 195.0);
    let m = h.click(&el, sob_o_menu);
    assert!(matches!(m.as_slice(), [M::Escolha(_)]), "o menu de cima ganha, não o botão: {m:?}");
    // Fechado, o botão volta a receber.
    h.desenhar(&el, None);
    assert_eq!(h.click(&el, sob_o_menu), vec![M::Clique(0)]);
}

const MUITAS: [&str; 20] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19"];

/// O centro do item `i` (à vista) do menu aberto embaixo de `campo`.
fn item_do_menu(campo: Rect, i: f32) -> Point {
    Point::new(campo.x + 20.0, campo.y + campo.height + 4.0 + 4.0 + 30.0 * i + 15.0)
}

#[test]
fn dropdown_com_altura_maxima_rola() {
    let mut h = H::new();
    let el: Element<M> = pick_list(&MUITAS[..], None, M::Escolha).width(120.0).menu_height(98.0).into();
    let campo = h.node(&el).rect;
    h.click(&el, centro(campo));
    // 98 de altura: três itens à vista; a roda desce dois.
    h.ev(&el, item_do_menu(campo, 1.0), Event::Wheel { dx: 0.0, dy: -60.0, modifiers: Modifiers::empty() });
    assert_eq!(h.click(&el, item_do_menu(campo, 0.0)), vec![M::Escolha("2")]);
    // Abaixo da altura máxima o menu já acabou.
    h.click(&el, centro(campo));
    assert_eq!(h.click(&el, item_do_menu(campo, 4.0)), vec![]);
}

// ---------- Busca com lista ----------

const FRUTAS: [&str; 4] = ["Maçã", "Banana", "Mamão", "Uva"];

fn digitar(h: &mut H, el: &Element<M>, s: &str) {
    for c in s.chars() {
        h.tecla(el, Key::Character(c.to_string().into()), Some(&c.to_string()));
    }
}

#[test]
fn combo_box_filtra_ao_digitar_e_escolhe_com_setas() {
    let mut h = H::new();
    let el: Element<M> = combo_box(&FRUTAS[..], "Fruta", None, M::Escolha).width(160.0).into();
    let campo = h.node(&el).rect;
    h.click(&el, centro(campo));
    // "ma" sobra Maçã e Mamão; a seta desce para a segunda.
    digitar(&mut h, &el, "MA");
    h.tecla(&el, nomeada(NamedKey::ArrowDown), None);
    assert_eq!(h.tecla(&el, nomeada(NamedKey::Enter), None), vec![M::Escolha("Mamão")]);
    // Sem acento e em minúsculas também acha.
    h.click(&el, centro(campo));
    digitar(&mut h, &el, "maca");
    assert_eq!(h.tecla(&el, nomeada(NamedKey::Enter), None), vec![M::Escolha("Maçã")]);
    // Esc fecha sem escolher.
    h.click(&el, centro(campo));
    h.tecla(&el, nomeada(NamedKey::Escape), None);
    assert_eq!(h.click(&el, item_do_menu(campo, 0.0)), vec![]);
}

#[test]
fn combo_box_clique_escolhe_da_lista_filtrada() {
    let mut h = H::new();
    let el: Element<M> = combo_box(&FRUTAS[..], "Fruta", Some(&FRUTAS[3]), M::Escolha).width(160.0).into();
    let campo = h.node(&el).rect;
    // Fechada, o lugar da lista não responde.
    assert_eq!(h.click(&el, item_do_menu(campo, 1.0)), vec![]);
    h.click(&el, centro(campo));
    assert_eq!(h.click(&el, item_do_menu(campo, 1.0)), vec![M::Escolha("Banana")]);
    h.click(&el, centro(campo));
    digitar(&mut h, &el, "u");
    assert_eq!(h.click(&el, item_do_menu(campo, 0.0)), vec![M::Escolha("Uva")]);
}

#[test]
fn combo_box_rola_a_lista_com_a_roda_e_as_setas() {
    let mut h = H::new();
    let el: Element<M> = combo_box(&MUITAS[..], "", None, M::Escolha).width(120.0).menu_height(98.0).into();
    let campo = h.node(&el).rect;
    h.click(&el, centro(campo));
    h.ev(&el, item_do_menu(campo, 1.0), Event::Wheel { dx: 0.0, dy: -90.0, modifiers: Modifiers::empty() });
    assert_eq!(h.click(&el, item_do_menu(campo, 0.0)), vec![M::Escolha("3")]);
    // Descendo com as setas além da borda, a lista acompanha: o item 5 fica embaixo.
    h.click(&el, centro(campo));
    for _ in 0..5 {
        h.tecla(&el, nomeada(NamedKey::ArrowDown), None);
    }
    assert_eq!(h.click(&el, item_do_menu(campo, 2.0)), vec![M::Escolha("5")]);
}

#[test]
fn combo_box_sem_resultado_aceita_as_setas() {
    let mut h = H::new();
    let el: Element<M> = combo_box(&FRUTAS[..], "Fruta", None, M::Escolha).width(160.0).into();
    let campo = h.node(&el).rect;
    h.click(&el, centro(campo));
    digitar(&mut h, &el, "zzz");
    assert_eq!(h.tecla(&el, nomeada(NamedKey::ArrowDown), None), vec![]);
    h.tecla(&el, nomeada(NamedKey::ArrowUp), None);
    h.desenhar(&el, None);
    assert_eq!(h.tecla(&el, nomeada(NamedKey::Enter), None), vec![]);
}

// ---------- Slider ----------

#[test]
fn slider_clique_arrasto_e_passo() {
    let mut h = H::new();
    // 216 de largura: 8 de folga de cada lado, trilho de 200.
    let el: Element<M> = slider(0.0..=100.0, 0.0, M::Valor).step(10.0).width(216.0).into();
    let y = h.node(&el).rect.y + 10.0;
    assert_eq!(h.ev(&el, Point::new(108.0, y), Event::MousePressed(MouseButton::Left)).0, vec![M::Valor(50.0)]);
    // 37/200 = 18,5 -> passo de 10 arredonda para 20.
    assert_eq!(h.ev(&el, Point::new(45.0, y), Event::CursorMoved(Point::new(45.0, y))).0, vec![M::Valor(20.0)]);
    // Passou do fim: fica no máximo.
    assert_eq!(h.ev(&el, Point::new(900.0, y), Event::CursorMoved(Point::new(900.0, y))).0, vec![M::Valor(100.0)]);
    h.ev(&el, Point::new(900.0, y), Event::MouseReleased(MouseButton::Left));
    // Solto, mover o mouse não muda nada.
    assert_eq!(h.ev(&el, Point::new(45.0, y), Event::CursorMoved(Point::new(45.0, y))).0, vec![]);
}

// ---------- Checkbox e toggler ----------

#[test]
fn checkbox_e_toggler_invertem_e_desabilitado_ignora() {
    let mut h = H::new();
    let el: Element<M> = column![checkbox("aviso", false).on_toggle(M::Marcado), toggler("escuro", true).on_toggle(M::Marcado), checkbox::<M>("travado", false)].spacing(10).into();
    let n = h.node(&el);
    let [a, b, c] = [0, 1, 2].map(|i| centro(n.children[i].rect));
    assert_eq!(h.click(&el, a), vec![M::Marcado(true)]);
    assert_eq!(h.click(&el, b), vec![M::Marcado(false)]);
    assert_eq!(h.click(&el, c), vec![]);
}

// ---------- Campo de texto ----------

/// O ciclo do app: cada `Texto` vira o novo valor do campo, que é montado de novo.
struct Campo {
    h: H,
    valor: String,
}

impl Campo {
    fn el(valor: &str) -> Element<'_, M> {
        column![text_input("", valor).on_input(M::Texto).on_submit(M::Enviar).width(200.0), Space::new(10.0, 100.0)].into()
    }
    fn aplicar(&mut self, msgs: Vec<M>) -> Vec<M> {
        for m in &msgs {
            if let M::Texto(t) = m {
                self.valor = t.clone();
            }
        }
        msgs
    }
    fn tecla(&mut self, k: Key, t: Option<&str>) -> Vec<M> {
        let el = Campo::el(&self.valor);
        let m = self.h.tecla(&el, k, t);
        drop(el);
        self.aplicar(m)
    }
    fn ctrl(&mut self, letra: &str) -> Vec<M> {
        let el = Campo::el(&self.valor);
        let m = self.h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character(letra.into()), text: Some(letra.into()), modifiers: Modifiers::CONTROL, pressed: true }).0;
        drop(el);
        self.aplicar(m)
    }
    fn digitar(&mut self, t: &str) -> Vec<M> {
        self.tecla(Key::Character(t.into()), Some(t))
    }
    fn click(&mut self, p: Point) {
        let el = Campo::el(&self.valor);
        let m = self.h.click(&el, p);
        drop(el);
        self.aplicar(m);
    }
}

#[test]
fn campo_edita_navega_envia_e_perde_o_foco() {
    let mut c = Campo { h: H::new(), valor: "abc".into() };
    let campo = c.h.node(&Campo::el("abc")).children[0].rect;
    // Sem foco, digitar não faz nada.
    assert_eq!(c.digitar("x"), vec![]);
    // Clique depois do texto põe o cursor no fim.
    c.click(Point::new(campo.x + 150.0, campo.y + 5.0));
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![M::Texto("ab".into())]);
    c.tecla(nomeada(NamedKey::ArrowLeft), None);
    c.tecla(nomeada(NamedKey::ArrowLeft), None);
    assert_eq!(c.digitar("X"), vec![M::Texto("Xab".into())]);
    // O cursor ficou depois do X; Delete apaga a letra seguinte (o "a").
    assert_eq!(c.tecla(nomeada(NamedKey::Delete), None), vec![M::Texto("Xb".into())]);
    c.tecla(nomeada(NamedKey::End), None);
    assert_eq!(c.digitar("ç"), vec![M::Texto("Xbç".into())]);
    // Backspace num caractere de 2 bytes apaga ele inteiro.
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![M::Texto("Xb".into())]);
    c.tecla(nomeada(NamedKey::Home), None);
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![], "Backspace no início não apaga");
    assert_eq!(c.tecla(nomeada(NamedKey::Enter), None), vec![M::Enviar]);
    // Ctrl+letra não é texto.
    let el = Campo::el(&c.valor);
    let ctrl = c.h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character("a".into()), text: Some("a".into()), modifiers: Modifiers::CONTROL, pressed: true }).0;
    drop(el);
    assert_eq!(ctrl, vec![]);
    // Clique fora tira o foco.
    c.click(Point::new(5.0, campo.y + campo.height + 50.0));
    assert_eq!(c.digitar("x"), vec![]);
    assert_eq!(c.valor, "Xb");
}

#[test]
fn campo_seleciona_com_shift_ctrl_a_e_arrasto() {
    let mut c = Campo { h: H::new(), valor: "abcd".into() };
    let campo = c.h.node(&Campo::el("abcd")).children[0].rect;
    c.click(Point::new(campo.x + 150.0, campo.y + 5.0));
    let shift = |c: &mut Campo, k: NamedKey| {
        let el = Campo::el(&c.valor);
        let m = c.h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Named(k), text: None, modifiers: Modifiers::SHIFT, pressed: true }).0;
        drop(el);
        c.aplicar(m)
    };
    // Shift+← duas vezes seleciona "cd"; digitar troca o trecho.
    shift(&mut c, NamedKey::ArrowLeft);
    shift(&mut c, NamedKey::ArrowLeft);
    assert_eq!(c.ctrl("c"), vec![]);
    assert_eq!(c.h.area.0.as_deref(), Some("cd"));
    assert_eq!(c.digitar("X"), vec![M::Texto("abX".into())]);
    // Ctrl+A e Backspace esvaziam.
    c.ctrl("a");
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![M::Texto(String::new())]);
    // Arrastar do começo ao fim seleciona tudo.
    c.valor = "abcd".into();
    let el = Campo::el("abcd");
    c.h.ev(&el, Point::new(campo.x + 1.0, campo.y + 5.0), Event::MousePressed(MouseButton::Left));
    c.h.ev(&el, Point::new(campo.x + 150.0, campo.y + 5.0), Event::CursorMoved(Point::new(campo.x + 150.0, campo.y + 5.0)));
    c.h.ev(&el, Point::new(campo.x + 150.0, campo.y + 5.0), Event::MouseReleased(MouseButton::Left));
    drop(el);
    assert_eq!(c.ctrl("x"), vec![M::Texto(String::new())]);
    assert_eq!(c.h.area.0.as_deref(), Some("abcd"));
}

#[test]
fn campo_copia_cola_e_recorta() {
    let mut c = Campo { h: H::new(), valor: "abc".into() };
    let campo = c.h.node(&Campo::el("abc")).children[0].rect;
    c.click(Point::new(campo.x + 150.0, campo.y + 5.0));
    // Copiar não muda o campo; colar entra no cursor.
    assert_eq!(c.ctrl("c"), vec![]);
    assert_eq!(c.h.area.0.as_deref(), Some("abc"));
    c.tecla(nomeada(NamedKey::Home), None);
    assert_eq!(c.ctrl("v"), vec![M::Texto("abcabc".into())]);
    // O cursor ficou depois do colado.
    assert_eq!(c.digitar("-"), vec![M::Texto("abc-abc".into())]);
    // Quebra de linha colada vira espaço; Ctrl+V maiúsculo (Caps Lock) também cola.
    c.h.area.0 = Some("x\ny".into());
    c.tecla(nomeada(NamedKey::End), None);
    assert_eq!(c.ctrl("V"), vec![M::Texto("abc-abcx y".into())]);
    // Recortar leva tudo e esvazia.
    assert_eq!(c.ctrl("x"), vec![M::Texto(String::new())]);
    assert_eq!(c.h.area.0.as_deref(), Some("abc-abcx y"));
}

#[test]
fn senha_nao_vai_para_a_area_de_transferencia() {
    let mut h = H::new();
    let el: Element<M> = text_input("", "segredo").secure(true).on_input(M::Texto).width(200.0).into();
    let campo = h.node(&el).rect;
    h.click(&el, Point::new(campo.x + 10.0, campo.y + 5.0));
    for k in ["c", "x"] {
        h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character(k.into()), text: Some(k.into()), modifiers: Modifiers::CONTROL, pressed: true });
    }
    assert_eq!(h.area.0, None);
}

#[test]
fn campo_senha_mapeia_o_clique_para_o_valor() {
    let mut h = H::new();
    let el: Element<M> = text_input("", "çãb").secure(true).on_input(M::Texto).width(200.0).into();
    let campo = h.node(&el).rect;
    // Clique no começo: o cursor fica antes do "ç" (2 bytes), e não no meio dele.
    h.click(&el, Point::new(campo.x + 10.0, campo.y + 5.0));
    h.tecla(&el, nomeada(NamedKey::Home), None);
    h.tecla(&el, nomeada(NamedKey::ArrowRight), None);
    assert_eq!(h.tecla(&el, Key::Character("-".into()), Some("-")), vec![M::Texto("ç-ãb".into())]);
}

#[test]
fn senha_trocada_pelo_app_nao_corta_caractere() {
    let mut h = H::new();
    let campo = |v| -> Element<M> { text_input("", v).secure(true).on_input(M::Texto).width(200.0).into() };
    let r = h.node(&campo("ab")).rect;
    h.click(&campo("ab"), Point::new(r.x + 10.0, r.y + 5.0));
    h.tecla(&campo("ab"), nomeada(NamedKey::Home), None);
    h.tecla(&campo("ab"), nomeada(NamedKey::ArrowRight), None);
    // O cursor ficou no byte 1, que no meio do "é" não é fronteira.
    h.desenhar(&campo("é"), None);
}

#[test]
fn quem_enche_dentro_da_rolagem_fica_finito() {
    let mut h = H::new();
    let c = Content::with_text("um\ndois");
    let el: Element<M> = scrollable(column![text("x"), text_editor(&c)]).into();
    let n = h.node(&el);
    assert!(n.children[0].rect.height.is_finite(), "{:?}", n.rect);
    h.desenhar(&el, None);
    let el: Element<M> = scrollable(column![text("x"), responsive(|s| Space::new(s.width, s.height).into())]).into();
    let n = h.node(&el);
    assert!(n.children[0].rect.height.is_finite(), "{:?}", n.rect);
    h.desenhar(&el, None);
}

// ---------- Mouse, camadas e map ----------

#[test]
fn duplo_clique_e_clique_simples() {
    let mut h = H::new();
    let el: Element<M> = mouse_area(Space::new(100.0, 100.0)).on_press(M::Clique(0)).on_double_click(M::Duplo).into();
    let p = Point::new(50.0, 50.0);
    assert_eq!(h.click(&el, p), vec![M::Clique(0)]);
    assert_eq!(h.click(&el, p), vec![M::Duplo]);
    // Depois do duplo, o próximo volta a ser simples.
    assert_eq!(h.click(&el, p), vec![M::Clique(0)]);
}

#[test]
fn duplo_clique_com_on_enter_e_o_mouse_andando_entre_os_cliques() {
    let mut h = H::new();
    let el: Element<M> = mouse_area(Space::new(100.0, 100.0)).on_enter(M::Enviar).on_double_click(M::Duplo).into();
    let p = Point::new(50.0, 50.0);
    assert_eq!(h.click(&el, p), vec![]);
    // O estado de "em cima" não pode apagar a hora do último clique.
    h.ev(&el, Point::new(51.0, 50.0), Event::CursorMoved(Point::new(51.0, 50.0)));
    assert_eq!(h.click(&el, p), vec![M::Duplo]);
}

#[test]
fn camada_opaca_bloqueia_clique_e_hover_de_baixo() {
    let mut h = H::new();
    let base = || button(text("fundo")).on_press(M::Clique(1)).width(Length::FILL).height(Length::FILL);
    let p = Point::new(50.0, 50.0);
    let livre: Element<M> = stack![base()].into();
    assert_eq!(h.click(&livre, p), vec![M::Clique(1)]);
    assert_eq!(h.desenhar(&livre, Some(p)), CursorIcon::Pointer);
    let coberto: Element<M> = stack![base(), opaque(Space::new(100.0, 100.0))].into();
    assert_eq!(h.click(&coberto, p), vec![]);
    assert_eq!(h.desenhar(&coberto, Some(p)), CursorIcon::Default, "o botão coberto não acende");
    // Fora da camada opaca, o fundo volta a responder.
    assert_eq!(h.click(&coberto, Point::new(300.0, 200.0)), vec![M::Clique(1)]);
}

#[test]
fn map_converte_as_mensagens() {
    let mut h = H::new();
    let el: Element<String> = Element::from(button("ok").on_press(7usize)).map(|n| format!("n={n}"));
    assert_eq!(h.click(&el, Point::new(5.0, 5.0)), vec!["n=7".to_string()]);
}

#[test]
fn botao_desabilitado_e_soltar_fora_nao_disparam() {
    let mut h = H::new();
    let el: Element<M> = column![button("a").on_press_maybe(None), button("b").on_press(M::Enviar)].spacing(40).into();
    let n = h.node(&el);
    assert_eq!(h.click(&el, centro(n.children[0].rect)), vec![]);
    // Apertar no botão e soltar fora cancela, como no navegador.
    let b = centro(n.children[1].rect);
    h.ev(&el, b, Event::MousePressed(MouseButton::Left));
    assert_eq!(h.ev(&el, Point::new(390.0, 290.0), Event::MouseReleased(MouseButton::Left)).0, vec![]);
}

#[test]
fn botao_dentro_de_botao_recebe_o_clique() {
    let mut h = H::new();
    let el: Element<M> = button(row![text("aba"), button("x").on_press(M::Clique(2)).width(30.0)].spacing(20)).on_press(M::Clique(1)).into();
    let n = h.node(&el);
    fn achar(n: &Node) -> Option<Rect> {
        if n.rect.width == 30.0 { Some(n.rect) } else { n.children.iter().find_map(achar) }
    }
    let x = achar(&n).expect("tem o x");
    assert_eq!(h.click(&el, centro(x)), vec![M::Clique(2)]);
    assert_eq!(h.click(&el, Point::new(n.rect.x + 3.0, centro(n.rect).y)), vec![M::Clique(1)]);
}

// ---------- Layout ----------

#[test]
fn wrap_quebra_a_linha_quando_nao_cabe() {
    let mut h = H::new();
    let el: Element<()> = container(row((0..5).map(|_| Space::new(100.0, 40.0).into())).spacing(10).wrap()).width(330.0).into();
    let linha = &h.node(&el).children[0];
    let pos: Vec<(f32, f32)> = linha.children.iter().map(|c| (c.rect.x, c.rect.y)).collect();
    assert_eq!(pos, vec![(0.0, 0.0), (110.0, 0.0), (220.0, 0.0), (0.0, 50.0), (110.0, 50.0)]);
    assert_eq!(linha.rect.height, 90.0);
}

#[test]
fn espaco_que_enche_respeita_a_altura_fixa_da_coluna() {
    let mut h = H::new();
    let el: Element<()> = column![Space::new(10.0, 50.0), vertical_space(), Space::new(10.0, 30.0)].height(200.0).into();
    let n = h.node(&el);
    assert_eq!(n.rect.height, 200.0);
    // O último encosta no fundo da coluna, não da tela (300).
    assert_eq!(n.children[2].rect.y + n.children[2].rect.height, 200.0);
}

#[test]
fn alinhamento_no_centro_e_no_fim() {
    let mut h = H::new();
    let el: Element<()> = container(Space::new(100.0, 50.0)).width(400.0).height(300.0).align_x(Align::End).align_y(Align::Center).into();
    let filho = h.node(&el).children[0].rect;
    assert_eq!((filho.x, filho.y), (300.0, 125.0));
}

// ---------- Pixels ----------

fn pixel(h: &H, x: u32, y: u32) -> [u8; 4] {
    let p = h.r.pixmap.pixel(x, y).expect("dentro do quadro");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn imagem_segue_o_raio_do_conteiner() {
    let mut h = H::new();
    let vermelho = Image::from_rgba(4, 4, [255, 0, 0, 255].repeat(16)).expect("4x4");
    let el: Element<()> = container(image(vermelho).width(Length::FILL).height(Length::FILL).content_fit(ContentFit::Fill)).width(200.0).height(100.0).clip(true).style(|_| Style::default().border(Border::rounded(30.0))).into();
    h.desenhar(&el, None);
    assert_eq!(pixel(&h, 2, 2)[3], 0, "o canto fica transparente");
    assert_eq!(pixel(&h, 100, 50), [255, 0, 0, 255], "o meio é a imagem");
    assert_eq!(pixel(&h, 199 - 2, 99 - 2)[3], 0, "o canto oposto também");
}

#[test]
fn vidro_desfoca_o_que_esta_atras() {
    let mut h = H::new();
    // Metade preta, metade branca; o vidro em cima da divisa mistura as duas.
    let fundo = canvas(Length::FILL, Length::FILL, |r, a, _| {
        r.fill_quad(Rect { width: a.width / 2.0, ..a }, Some(&Color::BLACK.into()), Border::default(), None);
        r.fill_quad(Rect { x: a.width / 2.0, width: a.width / 2.0, ..a }, Some(&Color::WHITE.into()), Border::default(), None);
    });
    let vidro = container(Space::new(Length::FILL, Length::FILL)).margin(100.0).style(|_| Style::default().backdrop_blur(20.0));
    let el: Element<()> = stack![fundo, vidro].into();
    h.desenhar(&el, None);
    let meio = pixel(&h, 200, 150)[0];
    assert!((60..200).contains(&meio), "na divisa o vidro dá cinza, veio {meio}");
    // Fora do vidro continua nítido.
    assert_eq!(pixel(&h, 198, 5)[0], 0);
    assert_eq!(pixel(&h, 202, 5)[0], 255);
}

#[test]
fn quadro_parcial_igual_ao_cheio() {
    // Degradê, sombra, vidro desfocado e texto: redesenhar só um pedaço tem que dar os mesmos pixels.
    let cena = || -> Element<'static, ()> {
        let fundo = container(Space::new(Length::FILL, Length::FILL))
            .style(|_| Style::default().background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(200, 30, 90)), (1.0, Color::rgb8(20, 40, 200))] }));
        let vidro = container(text("vidro"))
            .margin(60.0)
            .padding(20.0)
            .style(|_| Style::default().backdrop_blur(20.0).border(Border::rounded(14.0)).shadow(Shadow { color: Color::BLACK.alpha(0.5), offset: Point::new(0.0, 6.0), blur: 20.0 }));
        stack![fundo, vidro].into()
    };
    let (size, tema, st) = (Size::new(400.0, 300.0), Theme::dark(), States::default());
    let mut r = Renderer::new();
    let el = cena();
    r.begin(size, 1.0, Color::BLACK);
    let n = layout(&el, size, &mut r, &st, &tema);
    draw(&el, &n, &mut r, &st, &tema, None, false);
    let cheio = r.pixmap.data().to_vec();
    assert!(r.begin_parcial(size, 1.0, Color::BLACK, Rect { x: 37.0, y: 21.0, width: 150.0, height: 110.0 }));
    let n = layout(&el, size, &mut r, &st, &tema);
    draw(&el, &n, &mut r, &st, &tema, None, false);
    r.end();
    let pior = cheio.iter().zip(r.pixmap.data()).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0), "o pedaço redesenhado tem que bater com o quadro cheio");
}

#[test]
fn hover_redesenha_so_o_que_mudou_e_bate_com_o_cheio() {
    // Botão sobre um vidro: gravar, comparar e pintar só as regiões tem que dar o mesmo que o quadro cheio.
    let el: Element<()> = stack![
        container(Space::new(Length::FILL, Length::FILL)).style(|_| Style::default().background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(200, 30, 90)), (1.0, Color::rgb8(20, 40, 200))] })),
        container(button("ok").on_press(())).margin(60.0).padding(20.0).style(|_| Style::default().backdrop_blur(20.0).border(Border::rounded(14.0)))
    ]
    .into();
    let (size, tema, st) = (Size::new(400.0, 300.0), Theme::dark(), States::default());
    let n = { let mut r = Renderer::new(); layout(&el, size, &mut r, &st, &tema) };
    let quadro = |r: &mut Renderer, cursor: Option<Point>| {
        r.record();
        draw(&el, &n, r, &st, &tema, cursor, false);
        let c = r.changes();
        match &c {
            Changes::Regions(rs) => {
                for x in rs {
                    assert!(r.begin_parcial(size, 1.0, Color::BLACK, *x));
                    draw(&el, &n, r, &st, &tema, cursor, false);
                    r.end();
                }
            }
            Changes::All => {
                r.begin(size, 1.0, Color::BLACK);
                draw(&el, &n, r, &st, &tema, cursor, false);
            }
            Changes::None => {}
        }
        c
    };
    let mut r = Renderer::new();
    assert_eq!(quadro(&mut r, None), Changes::All);
    assert_eq!(quadro(&mut r, Some(Point::new(5.0, 5.0))), Changes::None, "mouse no vazio não muda nada");
    let botao = Point::new(85.0, 85.0);
    assert!(matches!(quadro(&mut r, Some(botao)), Changes::Regions(_)));
    let mut cheio = Renderer::new();
    cheio.begin(size, 1.0, Color::BLACK);
    draw(&el, &n, &mut cheio, &st, &tema, Some(botao), false);
    assert_ne!(r.pixmap.data(), { let mut v = Renderer::new(); v.begin(size, 1.0, Color::BLACK); draw(&el, &n, &mut v, &st, &tema, None, false); v.pixmap.data().to_vec() }.as_slice(), "o hover tem que mudar algo");
    let pior = cheio.pixmap.data().iter().zip(r.pixmap.data()).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0), "as regiões redesenhadas têm que bater com o quadro cheio");
}

#[test]
fn rolagem_de_dentro_tem_a_vez() {
    // Uma lista que rola dentro de uma página que também rola: a roda sobre a lista move a lista.
    let el: Element<M> = scrollable(column![Space::new(10.0, 20.0), lista(), Space::new(10.0, 800.0)]).into();
    let mut h = H::new();
    let rolagens = |h: &mut H| {
        let pagina = h.node(&el);
        let lista = &pagina.children[0].children[1];
        (pagina.rect.y - pagina.children[0].rect.y, lista.rect.y - lista.children[0].rect.y)
    };
    h.ev(&el, Point::new(100.0, 100.0), Event::Wheel { dx: 0.0, dy: -120.0, modifiers: Modifiers::empty() });
    assert_eq!(rolagens(&mut h), (0.0, 120.0), "só a lista rolou");
    // No fim da lista a página não rola por tabela (o evento já foi da lista).
    h.ev(&el, Point::new(100.0, 100.0), Event::Wheel { dx: 0.0, dy: -100_000.0, modifiers: Modifiers::empty() });
    assert_eq!(rolagens(&mut h).1, 800.0);
    // Fora da lista, rola a página.
    h.ev(&el, Point::new(350.0, 100.0), Event::Wheel { dx: 0.0, dy: -50.0, modifiers: Modifiers::empty() });
    assert_eq!(rolagens(&mut h).0, 50.0);
}

#[test]
fn imagem_nova_nao_herda_o_cache_da_velha() {
    // A imagem liberada deixa o endereço para a próxima; o cache não pode confundir as duas.
    let mut h = H::new();
    for cor in [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]] {
        let img = Image::from_rgba(4, 4, cor.repeat(16)).expect("4x4");
        let el: Element<()> = image(img).width(40.0).height(40.0).content_fit(ContentFit::Fill).into();
        h.desenhar(&el, None);
        assert_eq!(pixel(&h, 20, 20), cor);
    }
}

#[test]
fn sombra_grande_sem_emenda() {
    // A sombra de caixa grande é a de uma pequena esticada pelo meio: não pode ter degrau.
    let mut r = Renderer::new();
    let tam = Size::new(400.0, 300.0);
    r.begin(tam, 1.0, Color::TRANSPARENT);
    let sombra = Shadow { color: Color::BLACK, offset: Point::ORIGIN, blur: 12.0 };
    r.fill_quad(Rect { x: 50.0, y: 50.0, width: 300.0, height: 200.0 }, None, Border::rounded(10.0), Some(sombra));
    let alfa = |r: &Renderer, x: u32, y: u32| r.pixmap.pixel(x, y).expect("dentro").alpha() as i32;
    // Uma linha acima da caixa: o valor varia aos poucos no canto e fica igual no meio.
    for y in [40, 46] {
        let linha: Vec<i32> = (40..360).map(|x| alfa(&r, x, y)).collect();
        let salto = linha.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap_or(0);
        assert!(salto <= 12, "degrau de {salto} na linha {y}");
        assert_eq!(alfa(&r, 150, y), alfa(&r, 250, y), "o meio da borda é constante");
    }
    // O canto da grande bate com o de uma caixa pequena (sem esticar).
    let canto = alfa(&r, 48, 48);
    r.begin(tam, 1.0, Color::TRANSPARENT);
    r.fill_quad(Rect { x: 50.0, y: 50.0, width: 45.0, height: 45.0 }, None, Border::rounded(10.0), Some(sombra));
    assert!((alfa(&r, 48, 48) - canto).abs() <= 8, "canto {} x {}", alfa(&r, 48, 48), canto);
}

#[derive(Clone, Debug, PartialEq)]
enum E {
    Mudou,
}

#[test]
fn editor_digita_desfaz_copia_e_troca() {
    let mut h = H::new();
    let c = Content::with_text("um\ndois\r\ntrês");
    let el: Element<E> = text_editor(&c).on_change(E::Mudou).into();
    let r = h.node(&el).rect;
    h.click(&el, Point::new(r.x + 20.0, r.y + 18.0));
    let ctrl = |h: &mut H, k: &str| h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character(k.into()), text: Some(k.into()), modifiers: Modifiers::CONTROL, pressed: true }).0;
    h.tecla(&el, Key::Named(NamedKey::Home), None);
    h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Named(NamedKey::Home), text: None, modifiers: Modifiers::CONTROL, pressed: true });
    // Digitar avisa e junta num passo só no desfazer; as quebras originais (\r\n) ficam.
    assert_eq!(h.tecla(&el, Key::Character("o".into()), Some("o")), vec![E::Mudou]);
    h.tecla(&el, Key::Character("i".into()), Some("i"));
    assert_eq!(c.text(), "oium\ndois\r\ntrês");
    assert_eq!(c.cursor(), (1, 3));
    ctrl(&mut h, "z");
    assert_eq!(c.text(), "um\ndois\r\ntrês");
    ctrl(&mut h, "y");
    assert_eq!(c.text(), "oium\ndois\r\ntrês");
    // Ctrl+A, Ctrl+C copia tudo; colar no lugar não muda nada.
    ctrl(&mut h, "a");
    ctrl(&mut h, "c");
    assert_eq!(h.area.0.as_deref(), Some("oium\ndois\ntrês"));
    // Localizar sem diferenciar maiúsculas, e trocar tudo num passo.
    assert!(c.find_next("DOIS", false));
    assert!(!c.find_next("nada", false));
    assert_eq!(c.replace_all("s", "S", true), 2);
    assert_eq!(c.text(), "oium\ndoiS\r\ntrêS");
    c.undo();
    assert_eq!(c.text(), "oium\ndois\r\ntrês");
}

#[test]
fn autofocus_pega_a_proxima_tecla_e_clique_cancela() {
    let mut h = H::new();
    let campo = |pedido| -> Element<M> { text_input("", "").on_input(M::Texto).autofocus(pedido).width(200.0).into() };
    // Sem clicar: a primeira tecla já vai para o campo.
    assert_eq!(h.tecla(&campo(1), Key::Character("a".into()), Some("a")), vec![M::Texto("a".into())]);
    // Saiu do foco com um clique fora; o mesmo pedido não volta a pegar.
    h.click(&campo(1), Point::new(700.0, 500.0));
    assert_eq!(h.tecla(&campo(1), Key::Character("b".into()), Some("b")), vec![]);
    // Um pedido novo pega de novo; um clique antes da tecla cancela o pedido.
    assert_eq!(h.tecla(&campo(2), Key::Character("c".into()), Some("c")), vec![M::Texto("c".into())]);
    h.click(&campo(3), Point::new(700.0, 500.0));
    assert_eq!(h.tecla(&campo(3), Key::Character("d".into()), Some("d")), vec![]);
}

#[test]
fn autofocus_nao_volta_depois_do_esc() {
    let mut h = H::new();
    let campo: Element<M> = text_input("", "").on_input(M::Texto).autofocus(1).width(200.0).into();
    assert_eq!(h.tecla(&campo, Key::Character("a".into()), Some("a")), vec![M::Texto("a".into())]);
    h.tecla(&campo, nomeada(NamedKey::Escape), None);
    // O cursor do campo não apaga o pedido já atendido: o mesmo pedido não pega o foco de novo.
    assert_eq!(h.tecla(&campo, Key::Character("b".into()), Some("b")), vec![]);
}

#[test]
fn acha_e_conta_caracteres_ocultos() {
    let mut h = H::new();
    let c = Content::with_text("a\u{200b}b\nc d\u{a0}e\u{feff}");
    let el: Element<E> = text_editor(&c).invisibles(true).into();
    h.desenhar(&el, None);
    assert_eq!(c.count_chars(is_invisible), 3);
    // Espaço comum, tab e quebra não contam.
    assert!(!is_invisible(' ') && !is_invisible('\t') && !is_invisible('\n'));
    assert!(c.find_char(is_invisible));
    assert_eq!(c.cursor(), (1, 3));
    assert!(c.find_char(is_invisible));
    assert_eq!(c.cursor(), (2, 5));
    assert!(c.find_char(is_invisible));
    // Do fim, volta ao primeiro.
    assert!(c.find_char(is_invisible));
    assert_eq!(c.cursor(), (1, 3));
}

#[test]
fn formatar_liga_e_desliga() {
    let mut h = H::new();
    let c = Content::with_text("oi mundo\nsegunda");
    let el: Element<E> = text_editor(&c).into();
    h.desenhar(&el, None);
    // Sem seleção: os sinais entram no cursor, que fica no meio.
    c.wrap_selection("**", "**", |_| 0);
    assert_eq!(c.text(), "****oi mundo\nsegunda");
    assert_eq!(c.cursor(), (1, 3));
    c.undo();
    // Com seleção ("oi"): envolve, e o mesmo comando tira.
    c.find_next("oi", true);
    c.wrap_selection("**", "**", |_| 0);
    assert_eq!(c.text(), "**oi** mundo\nsegunda");
    c.wrap_selection("**", "**", |_| 0);
    assert_eq!(c.text(), "oi mundo\nsegunda");
    // Os três juntos: negrito, depois itálico e tachado na mesma palavra, e cada um sai sozinho.
    c.find_next("oi", true);
    c.wrap_selection("**", "**", |_| 0);
    c.wrap_selection("*", "*", |_| 0);
    c.wrap_selection("~~", "~~", |_| 0);
    assert_eq!(c.text(), "***~~oi~~*** mundo\nsegunda");
    c.wrap_selection("*", "*", |_| 0);
    assert_eq!(c.text(), "**~~oi~~** mundo\nsegunda");
    c.wrap_selection("*", "*", |_| 0);
    c.wrap_selection("**", "**", |_| 0);
    assert_eq!(c.text(), "~~*oi*~~ mundo\nsegunda");
    c.undo();
    c.undo();
    c.undo();
    c.undo();
    c.undo();
    c.undo();
    // Seleção que pega o "# " do começo: os sinais entram depois dele.
    let t = Content::with_text("# oi");
    let el: Element<E> = text_editor(&t).into();
    h.desenhar(&el, None);
    t.find_next("# oi", true);
    t.wrap_selection("~~", "~~", |l| if l.starts_with("# ") { 2 } else { 0 });
    assert_eq!(t.text(), "# ~~oi~~");
    // Começo de linha: título nas duas linhas selecionadas e um desfazer só.
    c.find_next("oi", true);
    c.edit_lines(|l| (0, if l.starts_with('#') { String::new() } else { "# ".into() }));
    assert_eq!(c.text(), "# oi mundo\nsegunda");
    c.undo();
    assert_eq!(c.text(), "oi mundo\nsegunda");
}

#[test]
fn trocar_linha_a_linha_guarda_as_quebras_e_desfaz() {
    let mut h = H::new();
    let c = Content::with_text("ab\r\ncd\nef");
    let el: Element<E> = text_editor(&c).into();
    h.desenhar(&el, None);
    c.map_lines(|l| l.to_uppercase());
    assert_eq!(c.text(), "AB\r\nCD\nEF");
    c.undo();
    assert_eq!(c.text(), "ab\r\ncd\nef");
}

#[test]
fn enter_e_colar_no_fim_guardam_a_quebra() {
    let mut h = H::new();
    let c = Content::with_text("um");
    let el: Element<E> = text_editor(&c).into();
    h.desenhar(&el, None);
    h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Named(NamedKey::End), text: None, modifiers: Modifiers::CONTROL, pressed: true });
    h.tecla(&el, Key::Named(NamedKey::Enter), None);
    h.tecla(&el, Key::Character("x".into()), Some("x"));
    assert_eq!(c.text(), "um\nx");
    h.area.0 = Some("a\nb".into());
    h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character("v".into()), text: Some("v".into()), modifiers: Modifiers::CONTROL, pressed: true });
    assert_eq!(c.text(), "um\nxa\nb");
}

#[test]
fn clique_na_caixinha_marca_e_o_cursor_fica_na_linha() {
    let mut h = H::new();
    let c = Content::with_text("a\n- [ ] tarefa");
    let el: Element<E> = text_editor(&c).markdown(true).on_change(E::Mudou).into();
    let r = h.node(&el).rect;
    h.desenhar(&el, None);
    // O meio do "[ ]" na segunda linha (padding 12, linha de 15 × 1.45).
    let x = r.x + 12.0 + h.r.measure("- ", 15.0, Font::DEFAULT, None).width + h.r.measure("[ ]", 15.0, Font::DEFAULT, None).width / 2.0;
    let y = r.y + 12.0 + (15.0f32 * 1.45).ceil() * 1.5;
    assert_eq!(h.click(&el, Point::new(x, y)), vec![E::Mudou]);
    assert_eq!(c.text(), "a\n- [x] tarefa");
    // O cursor fica na linha clicada: voltar para onde estava (fora da tela) rolava a tela até lá.
    assert_eq!(c.cursor().0, 2);
    h.desenhar(&el, None);
    h.click(&el, Point::new(x, y));
    assert_eq!(c.text(), "a\n- [ ] tarefa");
    // Clicar no texto da tarefa não marca nada.
    h.click(&el, Point::new(x + 80.0, y));
    assert_eq!(c.text(), "a\n- [ ] tarefa");
}

#[test]
fn trilho_da_rolagem_como_o_do_iced() {
    let verde = Color::rgb8(0, 255, 0);
    let mut h = H::new();
    // Com espaço próprio o trilho aparece mesmo sem o que rolar.
    let el: Element<()> = scrollable(Space::new(100.0, 50.0)).width(200.0).height(100.0).scrollbar_width(6).scrollbar_spacing(4).scrollbar_track(verde).into();
    h.desenhar(&el, None);
    assert_eq!(pixel(&h, 197, 50), [0, 255, 0, 255]);
    // Por cima do conteúdo, só quando há o que rolar.
    let el: Element<()> = scrollable(Space::new(100.0, 50.0)).width(200.0).height(100.0).scrollbar_track(verde).into();
    h.desenhar(&el, None);
    assert_ne!(pixel(&h, 195, 50), [0, 255, 0, 255]);
    let el: Element<()> = scrollable(Space::new(100.0, 500.0)).width(200.0).height(100.0).scrollbar_track(verde).into();
    h.desenhar(&el, None);
    assert_eq!(pixel(&h, 195, 95), [0, 255, 0, 255]);
}

#[test]
fn editor_rolado_no_fim_apaga_tudo_sem_cair() {
    let mut h = H::new();
    let texto: String = (0..1000).map(|i| format!("linha {i}\n")).collect();
    let c = Content::with_text(&texto);
    let el: Element<()> = text_editor(&c).into();
    h.desenhar(&el, None);
    let ctrl = |k: Key, t: Option<&str>| Event::Key { key: k, text: t.map(Into::into), modifiers: Modifiers::CONTROL, pressed: true };
    h.ev(&el, Point::ORIGIN, ctrl(Key::Named(NamedKey::End), None));
    h.desenhar(&el, None);
    h.ev(&el, Point::ORIGIN, ctrl(Key::Character("a".into()), Some("a")));
    h.tecla(&el, Key::Named(NamedKey::Backspace), None);
    h.desenhar(&el, None);
    assert!(c.text().trim().is_empty());
}

#[test]
fn edit_lines_com_prefixo_nao_ascii_deixa_o_cursor_inteiro() {
    let mut h = H::new();
    let c = Content::with_text("1. item");
    let el: Element<()> = text_editor(&c).into();
    h.desenhar(&el, None);
    h.tecla(&el, Key::Named(NamedKey::Home), None);
    h.tecla(&el, Key::Named(NamedKey::ArrowRight), None);
    c.edit_lines(|l| if l.starts_with("1. ") { (3, "• ".into()) } else { (0, String::new()) });
    h.desenhar(&el, None);
    h.tecla(&el, Key::Character("x".into()), Some("x"));
    assert_eq!(c.text(), "• xitem");
}

#[test]
fn slider_com_faixa_invertida_nao_cai() {
    let mut h = H::new();
    let el: Element<f32> = slider(10.0..=0.0, 5.0, |v| v).step(1.0).into();
    let r = h.node(&el).rect;
    let (m, _) = h.ev(&el, Point::new(r.x + 50.0, r.y + 10.0), Event::MousePressed(MouseButton::Left));
    assert!(m.iter().all(|v| (0.0..=10.0).contains(v)));
}

#[test]
fn cor_arrastada_pelo_cinza_guarda_o_matiz() {
    let mut h = H::new();
    let mut cor = Color::hex("#3b82f6").unwrap();
    let el = |c: Color| -> Element<Color> { column([color_picker(c, |c| c).into()]).into() };
    let campo = h.node(&el(cor)).children[0].rect;
    h.click(&el(cor), Point::new(campo.x + 5.0, campo.y + 5.0));
    h.desenhar(&el(cor), None);
    let (qx, qy) = (campo.x + 12.0, campo.y + campo.height + 16.0);
    for p in [Point::new(qx + 1.0, qy + 20.0), Point::new(qx - 5.0, qy + 20.0), Point::new(qx + 200.0, qy + 20.0)] {
        let ev = if p.x == qx + 1.0 { Event::MousePressed(MouseButton::Left) } else { Event::CursorMoved(p) };
        cor = h.ev(&el(cor), p, ev).0.pop().unwrap_or(cor);
        h.desenhar(&el(cor), None);
    }
    assert!(cor.b > cor.r, "virou vermelho: {cor:?}");
}

#[test]
fn clique_direito_no_campo_mantem_o_foco() {
    let mut h = H::new();
    let el: Element<String> = text_input("", "abc").id("campo").on_input(|s| s).into();
    let r = h.node(&el).rect;
    h.click(&el, centro(r));
    let foco = h.st.focus;
    assert!(foco.is_some());
    h.ev(&el, centro(r), Event::MousePressed(MouseButton::Right));
    assert_eq!(h.st.focus, foco);
}
