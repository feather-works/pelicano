//! Data, cor, radio, abas e número recebendo cliques de verdade.
use pelicano::column;
use pelicano::*;

const TELA: Size = Size::new(600.0, 600.0);

struct H {
    r: Renderer,
    st: States,
}

impl H {
    fn new() -> Self {
        H { r: Renderer::new(), st: States::default() }
    }
    fn node<M>(&mut self, el: &Element<'_, M>) -> Node {
        self.r.begin(TELA, 1.0, Color::TRANSPARENT);
        layout(el, TELA, &mut self.r, &self.st, &Theme::dark())
    }
    fn ev<M>(&mut self, el: &Element<'_, M>, p: Point, ev: Event) -> Vec<M> {
        let n = self.node(el);
        let mut msgs = Vec::new();
        let mut cx = EventCx { renderer: &mut self.r, states: &mut self.st, cursor: Some(p), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut MemoryClipboard::default() };
        dispatch(el, &n, &ev, &mut cx);
        msgs
    }
    fn click<M>(&mut self, el: &Element<'_, M>, p: Point) -> Vec<M> {
        let mut m = self.ev(el, p, Event::MousePressed(MouseButton::Left));
        m.extend(self.ev(el, p, Event::MouseReleased(MouseButton::Left)));
        m
    }
}

#[test]
fn datas_dia_da_semana_e_bissexto() {
    let d = |a, m, d| Date::new(a, m, d).expect("data válida");
    assert_eq!(d(2026, 10, 20).weekday(), 2, "terça");
    assert_eq!(d(2000, 1, 1).weekday(), 6, "sábado");
    assert_eq!(d(1970, 1, 1).weekday(), 4, "quinta");
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(1900, 2), 28);
    assert_eq!(days_in_month(2000, 2), 29);
    assert_eq!(Date::new(2025, 2, 29), None);
    assert_eq!(Date::new(2026, 4, 31), None);
    assert_eq!(Date::new(2026, 13, 1), None);
    assert_eq!(d(2026, 3, 5).to_string(), "05/03/2026");
    assert!(Date::today().year >= 2024);
}

#[test]
fn calendario_escolhe_dia_e_troca_de_mes() {
    let mut h = H::new();
    let hoje = Date::new(2026, 10, 20);
    let el: Element<Date> = date_picker(hoje, |d| d).into();
    let campo = h.node(&el).rect;
    let (painel_y, cel) = (campo.height + 4.0, 34.0);
    let grade_y = painel_y + 58.0;
    let dia = |i: usize| Point::new(8.0 + (i % 7) as f32 * cel + 17.0, grade_y + (i / 7) as f32 * cel + 17.0);
    // Fechado, clicar onde estaria o calendário não faz nada.
    assert_eq!(h.click(&el, dia(18)), vec![]);
    h.click(&el, Point::new(20.0, campo.height / 2.0));
    // Outubro de 2026 começa numa quinta (índice 4): o dia 15 é a célula 18.
    assert_eq!(h.click(&el, dia(18)), vec![Date::new(2026, 10, 15).unwrap()]);
    // Escolher fecha.
    assert_eq!(h.click(&el, dia(18)), vec![]);
    // Abre, vai para novembro (começa no domingo) e escolhe o dia 1.
    h.click(&el, Point::new(20.0, campo.height / 2.0));
    h.click(&el, Point::new(8.0 + 7.0 * cel + 16.0 - 36.0 + 14.0, painel_y + 20.0));
    assert_eq!(h.click(&el, dia(0)), vec![Date::new(2026, 11, 1).unwrap()]);
    // Célula antes do dia 1 não é data.
    h.click(&el, Point::new(20.0, campo.height / 2.0));
    assert_eq!(h.click(&el, dia(0)), vec![]);
}

#[test]
fn cores_hsv_e_hex() {
    for hex in ["#ff0000", "#22c55e", "#3b82f6", "#808080", "#000000", "#ffffff"] {
        let c = Color::hex(hex).unwrap();
        let (h, s, v) = to_hsv(c);
        assert_eq!(to_hex(from_hsv(h, s, v)), hex);
    }
    assert_eq!(to_hsv(Color::rgb(0.0, 1.0, 0.0)).0, 120.0);
}

#[test]
fn seletor_de_cor_quadro_e_prontas() {
    let mut h = H::new();
    let el: Element<String> = color_picker(Color::hex("#ff0000").unwrap(), to_hex).into();
    let campo = h.node(&el).rect;
    let painel_y = campo.height + 4.0;
    h.click(&el, Point::new(20.0, campo.height / 2.0));
    // Canto superior direito do quadro = matiz pura; inferior esquerdo = preto.
    assert_eq!(h.click(&el, Point::new(12.0 + 215.9, painel_y + 12.0)), vec!["#ff0000".to_string()]);
    assert_eq!(h.click(&el, Point::new(12.0, painel_y + 12.0 + 149.9)), vec!["#000000".to_string()]);
    // Arrastar no quadro manda a cor a cada movimento.
    h.ev(&el, Point::new(12.0, painel_y + 12.0), Event::MousePressed(MouseButton::Left));
    let p = Point::new(12.0 + 108.0, painel_y + 12.0);
    assert_eq!(h.ev(&el, p, Event::CursorMoved(p)), vec!["#ff8080".to_string()]);
    h.ev(&el, p, Event::MouseReleased(MouseButton::Left));
    // A quarta cor pronta é o verde.
    assert_eq!(h.click(&el, Point::new(12.0 + 3.0 * 27.5 + 11.0, painel_y + 200.0 + 11.0)), vec!["#22c55e".to_string()]);
    // Campo hex: clica e já digita (o clique não deixa a cor antiga no campo); com 6 dígitos a cor é aplicada.
    let hex = Point::new(40.0, painel_y + 200.0 + 22.0 + 12.0 + 14.0);
    h.click(&el, hex);
    let tecla = |k: Key, t: Option<&str>| Event::Key { key: k, text: t.map(Into::into), modifiers: Modifiers::empty(), pressed: true };
    assert_eq!(h.ev(&el, hex, tecla(Key::Character("#".into()), Some("#"))), Vec::<String>::new(), "o # é ignorado");
    assert_eq!(h.ev(&el, hex, tecla(Key::Character("ABC12".into()), Some("ABC12"))), Vec::<String>::new(), "5 dígitos ainda não é cor");
    assert_eq!(h.ev(&el, hex, tecla(Key::Character("3".into()), Some("3"))), vec!["#abc123".to_string()]);
    assert_eq!(h.ev(&el, hex, tecla(Key::Character("z9".into()), Some("z9"))), vec!["#abc123".to_string()], "não passa de 6 nem aceita z");
    assert_eq!(h.ev(&el, hex, tecla(Key::Named(NamedKey::Backspace), None)), Vec::<String>::new(), "apagar volta a 5 dígitos");
    assert_eq!(h.ev(&el, hex, tecla(Key::Character("f".into()), Some("f"))), vec!["#abc12f".to_string()]);
    // Clique fora fecha.
    h.click(&el, Point::new(590.0, 590.0));
    assert_eq!(h.click(&el, Point::new(12.0 + 3.0 * 27.5 + 11.0, painel_y + 200.0 + 11.0)), Vec::<String>::new());
}

#[test]
fn radio_abas_e_numero() {
    let mut h = H::new();
    let el: Element<u8> = column![radio("um", 1u8, Some(2), |v| v), radio("dois", 2u8, Some(2), |v| v)].spacing(10).into();
    let n = h.node(&el);
    let r0 = n.children[0].rect;
    assert_eq!(h.click(&el, Point::new(r0.x + 5.0, r0.y + 5.0)), vec![1]);

    let el: Element<usize> = tabs(&["Geral", "Avançado", "Sobre"], 0, |i| i);
    let abas = &h.node(&el).children[0];
    let terceira = abas.children[2].rect;
    assert_eq!(h.click(&el, Point::new(terceira.x + 5.0, terceira.y + 5.0)), vec![2]);

    let el: Element<f64> = number_input(9.0, 0.0..=10.0, 2.0, |v| v);
    let linha = &h.node(&el).children[0];
    let (menos, mais) = (linha.children[0].rect, linha.children[2].rect);
    assert_eq!(h.click(&el, Point::new(menos.x + 5.0, menos.y + 5.0)), vec![7.0]);
    assert_eq!(h.click(&el, Point::new(mais.x + 5.0, mais.y + 5.0)), vec![10.0], "preso no máximo");
    let el: Element<f64> = number_input(10.0, 0.0..=10.0, 2.0, |v| v);
    let linha = &h.node(&el).children[0];
    let mais = linha.children[2].rect;
    assert_eq!(h.click(&el, Point::new(mais.x + 5.0, mais.y + 5.0)), vec![], "no máximo o + desabilita");
}

#[test]
fn intervalo_de_datas_ordena_e_atravessa_o_mes() {
    let mut h = H::new();
    let d = |a, m, d| Date::new(a, m, d).unwrap();
    let el: Element<(Date, Date)> = date_range_picker(Some(d(2026, 10, 1)), Some(d(2026, 10, 2)), |a, b| (a, b)).into();
    let campo = h.node(&el).rect;
    let (painel_y, cel) = (campo.height + 4.0, 34.0);
    let dia = |i: usize| Point::new(8.0 + (i % 7) as f32 * cel + 17.0, painel_y + 58.0 + (i / 7) as f32 * cel + 17.0);
    let abrir = Point::new(20.0, campo.height / 2.0);
    // Outubro/2026 começa na quinta (índice 4): dia N é a célula N + 3.
    h.click(&el, abrir);
    assert_eq!(h.click(&el, dia(10 + 3)), vec![], "o primeiro clique só marca o início");
    assert_eq!(h.click(&el, dia(5 + 3)), vec![(d(2026, 10, 5), d(2026, 10, 10))], "fim antes do início: troca");
    // De 28/10 até 2/11, passando para o mês seguinte entre os cliques.
    h.click(&el, abrir);
    h.click(&el, dia(28 + 3));
    h.click(&el, Point::new(8.0 + 7.0 * cel + 16.0 - 36.0 + 14.0, painel_y + 20.0));
    assert_eq!(h.click(&el, dia(1)), vec![(d(2026, 10, 28), d(2026, 11, 2))], "novembro começa no domingo: dia 2 é a célula 1");
    // Fechar no meio esquece o início.
    h.click(&el, abrir);
    h.click(&el, dia(3 + 3));
    h.click(&el, Point::new(590.0, 590.0));
    h.click(&el, abrir);
    assert_eq!(h.click(&el, dia(7 + 3)), vec![]);
}

#[test]
fn painel_abre_para_cima_sem_espaco_embaixo() {
    // Campo de data colado no fim da tela: o calendário abre acima dele, e o clique no dia vale.
    let el: Element<Date> = container(date_picker(Date::new(2026, 10, 1), |d| d)).padding(Padding { top: 560.0, ..Padding::ZERO }).into();
    let mut h = H::new();
    let campo = h.node(&el).children[0].rect;
    h.click(&el, Point::new(campo.x + 10.0, campo.y + 10.0));
    // 1º de outubro de 2026 é quinta: 5ª coluna da 1ª linha. A grade fica 58 px abaixo do topo do painel.
    let painel_topo = campo.y - 4.0 - (36.0 + 22.0 + 6.0 * 34.0 + 14.0);
    assert!(painel_topo >= 0.0);
    let dia = Point::new(campo.x + 8.0 + 4.0 * 34.0 + 17.0, painel_topo + 58.0 + 17.0);
    assert_eq!(h.click(&el, dia), vec![Date::new(2026, 10, 1).unwrap()]);
}
