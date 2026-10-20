//! Seletores que abrem um painel: data e cor.
use crate::widget::{surgir, ABRIR, CursorIcon, DrawCx, Element, EventCx, LayoutCx, Limits, Node, Widget};
use crate::{Background, Border, Captured, Color, Event, Font, Key, Length, MouseButton, NamedKey, Point, Rect, Renderer, Shadow, Size, Theme};

fn painel(r: &mut Renderer, area: Rect, t: &Theme) {
    r.fill_quad(area, Some(&t.surface.into()), Border::rounded(10.0).color(t.border, 1.0), Some(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 6.0), blur: 16.0 }));
}

/// O campo fechado dos dois seletores: caixa com borda que acende ao abrir.
fn campo(r: &mut Renderer, area: Rect, t: &Theme, aberto: bool, hover: bool) {
    let borda = if aberto { t.primary } else if hover { t.muted } else { t.border };
    r.fill_quad(area, Some(&t.surface.into()), Border::rounded(6.0).color(borda, 1.0), None);
}

fn texto_centrado_y(r: &mut Renderer, s: &str, size: f32, font: Font, cor: Color, x: f32, area: Rect) {
    let h = r.measure(s, size, font, None).height;
    r.draw_text(s, size, font, cor, Point::new(x, area.y + (area.height - h) / 2.0), None);
}

/// Onde abre um painel de `altura` preso ao `campo`: embaixo, ou em cima se embaixo não
/// couber na janela (e em cima couber).
pub(crate) fn painel_y(campo: Rect, altura: f32, janela: f32) -> f32 {
    let baixo = campo.y + campo.height + 4.0;
    let cima = campo.y - 4.0 - altura;
    if baixo + altura > janela && cima >= 0.0 { cima } else { baixo }
}

// ---------- Data ----------

/// Uma data do calendário (sem fuso nem hora).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// `None` se o dia não existe (31 de abril, 29 de fevereiro fora do bissexto).
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)).then_some(Date { year, month, day })
    }
    /// Hoje, pelo relógio do sistema em UTC.
    /// shortcut: UTC, não o fuso local; perto da meia-noite pode marcar o dia vizinho. Ler o fuso do sistema se incomodar.
    pub fn today() -> Self {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Date::from_days((s / 86_400) as i64)
    }
    /// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
    fn from_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        Date { year: (yoe + era * 400 + i64::from(m <= 2)) as i32, month: m, day: d }
    }
    /// 0 = domingo.
    pub fn weekday(&self) -> u8 {
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.month < 3 { self.year - 1 } else { self.year };
        ((y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[self.month as usize - 1] + self.day as i32).rem_euclid(7)) as u8
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}/{:02}/{:04}", self.day, self.month, self.year)
    }
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
const SEMANA: [&str; 7] = ["D", "S", "T", "Q", "Q", "S", "S"];
const CEL: f32 = 34.0;

#[derive(Default)]
struct Calendario {
    aberto: bool,
    /// O mês à mostra: (ano, mês).
    vendo: Option<(i32, u8)>,
    /// No intervalo: o primeiro dia já clicado, esperando o segundo.
    inicio: Option<Date>,
}

enum Escolha<'a, M> {
    Uma(Box<dyn Fn(Date) -> M + 'a>),
    Intervalo(Box<dyn Fn(Date, Date) -> M + 'a>),
}

pub struct DatePicker<'a, M> {
    value: Option<Date>,
    /// O fim do intervalo (só no `date_range_picker`).
    end: Option<Date>,
    escolha: Escolha<'a, M>,
    placeholder: &'a str,
    width: Length,
}

/// Campo de data que abre um calendário.
pub fn date_picker<'a, M>(value: Option<Date>, on_select: impl Fn(Date) -> M + 'a) -> DatePicker<'a, M> {
    DatePicker { value, end: None, escolha: Escolha::Uma(Box::new(on_select)), placeholder: "dd/mm/aaaa", width: Length::Fixed(160.0) }
}

/// Intervalo de datas: o primeiro clique marca o início, o segundo o fim (a ordem se acerta sozinha).
pub fn date_range_picker<'a, M>(start: Option<Date>, end: Option<Date>, on_select: impl Fn(Date, Date) -> M + 'a) -> DatePicker<'a, M> {
    DatePicker { value: start, end, escolha: Escolha::Intervalo(Box::new(on_select)), placeholder: "início – fim", width: Length::Fixed(250.0) }
}

impl<'a, M> DatePicker<'a, M> {
    pub fn placeholder(mut self, p: &'a str) -> Self {
        self.placeholder = p;
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    fn mes(&self, st: Option<&Calendario>) -> (i32, u8) {
        st.and_then(|s| s.vendo).or(self.value.map(|d| (d.year, d.month))).unwrap_or_else(|| {
            let h = Date::today();
            (h.year, h.month)
        })
    }
}

/// Onde fica cada parte do calendário aberto abaixo do campo.
struct Geo {
    painel: Rect,
    anterior: Rect,
    proximo: Rect,
    grade: Rect,
}

fn geo(campo: Rect, janela: f32) -> Geo {
    let altura = 36.0 + 22.0 + 6.0 * CEL + 14.0;
    let painel = Rect { x: campo.x, y: painel_y(campo, altura, janela), width: 7.0 * CEL + 16.0, height: altura };
    let anterior = Rect { x: painel.x + 8.0, y: painel.y + 6.0, width: 28.0, height: 28.0 };
    let proximo = Rect { x: painel.x + painel.width - 36.0, ..anterior };
    let grade = Rect { x: painel.x + 8.0, y: painel.y + 36.0 + 22.0, width: 7.0 * CEL, height: 6.0 * CEL };
    Geo { painel, anterior, proximo, grade }
}

/// O dia sob `p` na grade do mês, se houver.
fn dia_em(g: &Geo, (ano, mes): (i32, u8), p: Point) -> Option<Date> {
    if !g.grade.contains(p) {
        return None;
    }
    let i = ((p.y - g.grade.y) / CEL) as usize * 7 + ((p.x - g.grade.x) / CEL) as usize;
    let inicio = Date { year: ano, month: mes, day: 1 }.weekday() as usize;
    i.checked_sub(inicio).and_then(|d| Date::new(ano, mes, d as u8 + 1))
}

fn mudar_mes((a, m): (i32, u8), d: i32) -> (i32, u8) {
    let i = a * 12 + m as i32 - 1 + d;
    (i.div_euclid(12), (i.rem_euclid(12) + 1) as u8)
}

impl<M> Widget<M> for DatePicker<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let h = cx.renderer.measure("Ag", 14.0, Font::DEFAULT, None).height + 14.0;
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 160.0), h))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let t = *cx.theme;
        let st = cx.states.get::<Calendario>(n.key);
        let aberto = st.is_some_and(|s| s.aberto);
        let hover = cx.hovered(&n.rect);
        if hover {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        campo(cx.renderer, n.rect, &t, aberto, hover);
        let (s, cor) = match (&self.escolha, self.value, self.end) {
            (Escolha::Uma(_), Some(d), _) => (d.to_string(), cx.text_color),
            (Escolha::Intervalo(_), Some(a), Some(b)) => (format!("{a} – {b}"), cx.text_color),
            _ => (self.placeholder.to_string(), t.muted),
        };
        texto_centrado_y(cx.renderer, &s, 14.0, Font::DEFAULT, cor, n.rect.x + 10.0, n.rect);
        // Ícone de calendário desenhado com dois traços e uma caixa.
        let (ix, iy) = (n.rect.x + n.rect.width - 24.0, n.rect.y + n.rect.height / 2.0 - 7.0);
        cx.renderer.fill_quad(Rect { x: ix, y: iy + 2.0, width: 14.0, height: 12.0 }, None, Border::rounded(2.0).color(t.muted, 1.5), None);
        cx.renderer.stroke(&[Point::new(ix, iy + 6.0), Point::new(ix + 14.0, iy + 6.0)], 1.5, t.muted);
        // O painel surge ao abrir (fechar é na hora: o estado de dentro já se foi).
        let p = cx.transition(n.key, if aberto { 1.0f32 } else { 0.0 }, ABRIR);
        if !aberto {
            return;
        }
        let (ano, mes) = self.mes(st);
        let (valor, cursor, g) = (self.value, cx.popup_cursor(), geo(n.rect, cx.renderer.window_size().height));
        cx.popup(g.painel);
        // O trecho pintado: o escolhido, ou do primeiro clique até o dia sob o mouse.
        let faixa = match (&self.escolha, st.and_then(|s| s.inicio)) {
            (Escolha::Intervalo(_), Some(a)) => {
                let b = cursor.and_then(|p| dia_em(&g, (ano, mes), p)).unwrap_or(a);
                Some((a.min(b), a.max(b)))
            }
            (Escolha::Intervalo(_), None) => self.value.zip(self.end),
            _ => None,
        };
        cx.overlay(move |r| surgir(r, g.painel, p, |r| {
            painel(r, g.painel, &t);
            let titulo = format!("{} {}", MESES[mes as usize - 1], ano);
            let tw = r.measure(&titulo, 14.0, Font::SEMIBOLD, None).width;
            texto_centrado_y(r, &titulo, 14.0, Font::SEMIBOLD, t.text, g.painel.x + (g.painel.width - tw) / 2.0, g.anterior);
            for (b, seta) in [(g.anterior, [(4.0, -5.0), (-1.0, 0.0), (4.0, 5.0)]), (g.proximo, [(-4.0, -5.0), (1.0, 0.0), (-4.0, 5.0)])] {
                if cursor.is_some_and(|p| b.contains(p)) {
                    r.fill_quad(b, Some(&t.text.alpha(0.08).into()), Border::rounded(6.0), None);
                }
                let c = Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0);
                r.stroke(&seta.map(|(dx, dy)| Point::new(c.x + dx, c.y + dy)), 1.6, t.text);
            }
            for (i, d) in SEMANA.iter().enumerate() {
                let w = r.measure(d, 11.0, Font::SEMIBOLD, None).width;
                r.draw_text(d, 11.0, Font::SEMIBOLD, t.muted, Point::new(g.grade.x + i as f32 * CEL + (CEL - w) / 2.0, g.grade.y - 20.0), None);
            }
            let inicio = Date { year: ano, month: mes, day: 1 }.weekday() as usize;
            let hoje = Date::today();
            for dia in 1..=days_in_month(ano, mes) {
                let i = inicio + dia as usize - 1;
                let cel = Rect { x: g.grade.x + (i % 7) as f32 * CEL + 2.0, y: g.grade.y + (i / 7) as f32 * CEL + 2.0, width: CEL - 4.0, height: CEL - 4.0 };
                let data = Date { year: ano, month: mes, day: dia };
                let escolhido = match faixa {
                    Some((a, b)) => data == a || data == b,
                    None => valor == Some(data),
                };
                if faixa.is_some_and(|(a, b)| data > a && data < b) {
                    // O meio do intervalo: faixa reta de célula a célula.
                    r.fill_quad(Rect { x: cel.x - 2.0, width: CEL, ..cel }, Some(&t.primary.alpha(0.2).into()), Border::default(), None);
                }
                if escolhido {
                    r.fill_quad(cel, Some(&t.primary.into()), Border::rounded(CEL / 2.0), None);
                } else if cursor.is_some_and(|p| cel.contains(p)) {
                    r.fill_quad(cel, Some(&t.text.alpha(0.08).into()), Border::rounded(CEL / 2.0), None);
                }
                if data == hoje && !escolhido {
                    r.fill_quad(cel, None, Border::rounded(CEL / 2.0).color(t.primary, 1.0), None);
                }
                let s = dia.to_string();
                let w = r.measure(&s, 13.0, Font::DEFAULT, None).width;
                texto_centrado_y(r, &s, 13.0, Font::DEFAULT, if escolhido { t.on_primary } else { t.text }, cel.x + (cel.width - w) / 2.0, cel);
            }
        }));
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let aberto = cx.states.get::<Calendario>(n.key).is_some_and(|s| s.aberto);
        let p = cx.popup_cursor();
        match ev {
            Event::MousePressed(MouseButton::Left) if aberto => {
                let g = geo(n.rect, cx.renderer.window_size().height);
                let mes = self.mes(cx.states.get::<Calendario>(n.key));
                cx.redraw = true;
                let Some(p) = p.filter(|p| g.painel.contains(*p)) else {
                    cx.states.get_mut::<Calendario>(n.key).aberto = false;
                    return if p.is_some_and(|p| n.rect.contains(p)) { Captured::Yes } else { Captured::No };
                };
                let st = cx.states.get_mut::<Calendario>(n.key);
                if g.anterior.contains(p) {
                    st.vendo = Some(mudar_mes(mes, -1));
                } else if g.proximo.contains(p) {
                    st.vendo = Some(mudar_mes(mes, 1));
                } else if let Some(d) = dia_em(&g, mes, p) {
                    match (&self.escolha, st.inicio.take()) {
                        (Escolha::Uma(f), _) => {
                            st.aberto = false;
                            cx.publish(f(d));
                        }
                        (Escolha::Intervalo(_), None) => st.inicio = Some(d),
                        (Escolha::Intervalo(f), Some(a)) => {
                            st.aberto = false;
                            cx.publish(f(a.min(d), a.max(d)));
                        }
                    }
                }
                Captured::Yes
            }
            Event::MousePressed(MouseButton::Left) if p.is_some_and(|p| n.rect.contains(p)) => {
                // Abre no mês da data escolhida (ou no de hoje).
                *cx.states.get_mut::<Calendario>(n.key) = Calendario { aberto: true, ..Default::default() };
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key: Key::Named(NamedKey::Escape), pressed: true, .. } if aberto => {
                cx.states.get_mut::<Calendario>(n.key).aberto = false;
                cx.redraw = true;
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

// ---------- Cor ----------

/// Matiz (0..360), saturação e brilho (0..1).
pub fn to_hsv(c: Color) -> (f32, f32, f32) {
    let (mx, mn) = (c.r.max(c.g).max(c.b), c.r.min(c.g).min(c.b));
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == c.r {
        60.0 * ((c.g - c.b) / d).rem_euclid(6.0)
    } else if mx == c.g {
        60.0 * ((c.b - c.r) / d + 2.0)
    } else {
        60.0 * ((c.r - c.g) / d + 4.0)
    };
    (h, if mx == 0.0 { 0.0 } else { d / mx }, mx)
}

pub fn from_hsv(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color::rgb(r + m, g + m, b + m)
}

pub fn to_hex(c: Color) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

const PRONTAS: [&str; 8] = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#8b5cf6", "#ec4899"];

#[derive(Default, Clone, Copy, PartialEq)]
enum Arraste {
    #[default]
    Nada,
    Quadro,
    Matiz,
}

#[derive(Default)]
struct Paleta {
    aberto: bool,
    arraste: Arraste,
    /// A matiz guardada à parte: no cinza ela some da cor e o quadro perderia o tom.
    matiz: Option<f32>,
    /// O hex sendo digitado (sem `#`); `None` = campo sem foco.
    hex: Option<String>,
}

pub struct ColorPicker<'a, M> {
    value: Color,
    on_change: Box<dyn Fn(Color) -> M + 'a>,
    width: Length,
}

/// Amostra da cor que abre o seletor: quadro de saturação/brilho, barra de matiz e cores prontas.
pub fn color_picker<'a, M>(value: Color, on_change: impl Fn(Color) -> M + 'a) -> ColorPicker<'a, M> {
    ColorPicker { value, on_change: Box::new(on_change), width: Length::Fixed(140.0) }
}

impl<M> ColorPicker<'_, M> {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
}

struct GeoCor {
    painel: Rect,
    quadro: Rect,
    matiz: Rect,
    prontas: Rect,
    hex: Rect,
}

fn geo_cor(campo: Rect, janela: f32) -> GeoCor {
    let altura = 12.0 + 150.0 + 12.0 + 14.0 + 12.0 + 22.0 + 12.0 + 28.0 + 12.0;
    let painel = Rect { x: campo.x, y: painel_y(campo, altura, janela), width: 240.0, height: altura };
    let quadro = Rect { x: painel.x + 12.0, y: painel.y + 12.0, width: 216.0, height: 150.0 };
    let matiz = Rect { y: quadro.y + quadro.height + 12.0, height: 14.0, ..quadro };
    let prontas = Rect { y: matiz.y + matiz.height + 12.0, height: 22.0, ..quadro };
    let hex = Rect { y: prontas.y + prontas.height + 12.0, height: 28.0, ..quadro };
    GeoCor { painel, quadro, matiz, prontas, hex }
}

impl<M> ColorPicker<'_, M> {
    fn hsv(&self, st: Option<&Paleta>) -> (f32, f32, f32) {
        let (h, s, v) = to_hsv(self.value);
        (st.and_then(|p| p.matiz).unwrap_or(h), s, v)
    }
    /// A cor no ponto `p`, conforme o que está sendo arrastado.
    fn cor_em(&self, n: &Node, p: Point, cx: &mut EventCx<M>) {
        let g = geo_cor(n.rect, cx.renderer.window_size().height);
        let st = cx.states.get_mut::<Paleta>(n.key);
        let (h, s, v) = {
            let (h0, s0, v0) = to_hsv(self.value);
            (st.matiz.unwrap_or(h0), s0, v0)
        };
        let nova = match st.arraste {
            Arraste::Quadro => {
                let s = ((p.x - g.quadro.x) / g.quadro.width).clamp(0.0, 1.0);
                let v = 1.0 - ((p.y - g.quadro.y) / g.quadro.height).clamp(0.0, 1.0);
                from_hsv(h, s, v)
            }
            Arraste::Matiz => {
                let h = ((p.x - g.matiz.x) / g.matiz.width).clamp(0.0, 0.9999) * 360.0;
                st.matiz = Some(h);
                from_hsv(h, s, v)
            }
            Arraste::Nada => return,
        };
        cx.redraw = true;
        cx.publish((self.on_change)(nova.alpha(self.value.a)));
    }
}

impl<M> Widget<M> for ColorPicker<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let h = cx.renderer.measure("Ag", 14.0, Font::DEFAULT, None).height + 14.0;
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 140.0), h))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let t = *cx.theme;
        let st = cx.states.get::<Paleta>(n.key);
        let aberto = st.is_some_and(|s| s.aberto);
        let hover = cx.hovered(&n.rect);
        if hover {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        campo(cx.renderer, n.rect, &t, aberto, hover);
        let amostra = Rect { x: n.rect.x + 6.0, y: n.rect.y + 6.0, width: n.rect.height - 12.0, height: n.rect.height - 12.0 };
        cx.renderer.fill_quad(amostra, Some(&self.value.into()), Border::rounded(4.0).color(t.text.alpha(0.2), 1.0), None);
        texto_centrado_y(cx.renderer, &to_hex(self.value), 13.0, Font::MONO, cx.text_color, amostra.x + amostra.width + 8.0, n.rect);
        // O painel surge ao abrir (fechar é na hora: o estado de dentro já se foi).
        let p = cx.transition(n.key, if aberto { 1.0f32 } else { 0.0 }, ABRIR);
        if !aberto {
            return;
        }
        let (h, s, v) = self.hsv(st);
        let digitando = st.and_then(|p| p.hex.clone());
        let (g, valor) = (geo_cor(n.rect, cx.renderer.window_size().height), self.value);
        cx.popup(g.painel);
        cx.overlay(move |r| surgir(r, g.painel, p, |r| {
            painel(r, g.painel, &t);
            // Quadro: matiz pura, clareada para a esquerda e escurecida para baixo.
            let raio = Border::rounded(6.0);
            r.fill_quad(g.quadro, Some(&from_hsv(h, 1.0, 1.0).into()), raio, None);
            r.fill_quad(g.quadro, Some(&Background::Linear { angle: 0.0, stops: vec![(0.0, Color::WHITE), (1.0, Color::WHITE.alpha(0.0))] }), raio, None);
            r.fill_quad(g.quadro, Some(&Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, Color::BLACK.alpha(0.0)), (1.0, Color::BLACK)] }), raio, None);
            let alvo = Point::new(g.quadro.x + s * g.quadro.width, g.quadro.y + (1.0 - v) * g.quadro.height);
            r.fill_quad(Rect { x: alvo.x - 7.0, y: alvo.y - 7.0, width: 14.0, height: 14.0 }, Some(&valor.alpha(1.0).into()), Border::rounded(7.0).color(Color::WHITE, 2.0), Some(Shadow { color: Color::BLACK.alpha(0.4), offset: Point::ORIGIN, blur: 3.0 }));
            let arco: Vec<(f32, Color)> = (0..=6).map(|i| (i as f32 / 6.0, from_hsv(i as f32 * 60.0, 1.0, 1.0))).collect();
            r.fill_quad(g.matiz, Some(&Background::Linear { angle: 0.0, stops: arco }), Border::rounded(7.0), None);
            let hx = g.matiz.x + h / 360.0 * g.matiz.width;
            r.fill_quad(Rect { x: hx - 5.0, y: g.matiz.y - 2.0, width: 10.0, height: g.matiz.height + 4.0 }, Some(&from_hsv(h, 1.0, 1.0).into()), Border::rounded(5.0).color(Color::WHITE, 2.0), None);
            for (i, c) in PRONTAS.iter().filter_map(|c| Color::hex(c)).enumerate() {
                let q = Rect { x: g.prontas.x + i as f32 * 27.5, y: g.prontas.y, width: 22.0, height: 22.0 };
                let borda = if to_hex(c) == to_hex(valor) { Border::rounded(6.0).color(t.text, 2.0) } else { Border::rounded(6.0) };
                r.fill_quad(q, Some(&c.into()), borda, None);
            }
            let borda = if digitando.is_some() { t.primary } else { t.border };
            r.fill_quad(g.hex, Some(&t.background.into()), Border::rounded(6.0).color(borda, 1.0), None);
            let vazio = digitando.as_deref().is_some_and(str::is_empty);
            let conteudo = match digitando.as_deref() {
                Some(b) if !b.is_empty() => format!("#{b}"),
                _ => to_hex(valor),
            };
            texto_centrado_y(r, &conteudo, 13.0, Font::MONO, if vazio { t.muted } else { t.text }, g.hex.x + 10.0, g.hex);
            if digitando.is_some() {
                let w = r.measure(if vazio { "#" } else { &conteudo }, 13.0, Font::MONO, None).width;
                r.fill_quad(Rect { x: g.hex.x + 10.0 + w + 1.0, y: g.hex.y + 6.0, width: 1.0, height: g.hex.height - 12.0 }, Some(&t.text.into()), Border::default(), None);
            }
            r.draw_text("Hex", 11.0, Font::DEFAULT, t.muted, Point::new(g.hex.x + g.hex.width - 30.0, g.hex.y + 8.0), None);
        }));
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let st = cx.states.get::<Paleta>(n.key);
        let (aberto, arraste) = (st.is_some_and(|s| s.aberto), st.map_or(Arraste::Nada, |s| s.arraste));
        let p = cx.popup_cursor();
        let g = geo_cor(n.rect, cx.renderer.window_size().height);
        // A barra de matiz é fina: o clique vale com 4 px de folga.
        let matiz_alvo = Rect { y: g.matiz.y - 4.0, height: g.matiz.height + 8.0, ..g.matiz };
        match ev {
            Event::MousePressed(MouseButton::Left) if aberto => {
                cx.redraw = true;
                let Some(p) = p.filter(|p| g.painel.contains(*p)) else {
                    cx.states.get_mut::<Paleta>(n.key).aberto = false;
                    return if p.is_some_and(|p| n.rect.contains(p)) { Captured::Yes } else { Captured::No };
                };
                // Qualquer clique no painel fora do campo hex tira o foco dele.
                cx.states.get_mut::<Paleta>(n.key).hex = None;
                if g.hex.contains(p) {
                    // Começa vazio: o que se digita substitui a cor (a atual aparece apagada até lá).
                    cx.states.get_mut::<Paleta>(n.key).hex = Some(String::new());
                } else if g.quadro.contains(p) {
                    cx.states.get_mut::<Paleta>(n.key).arraste = Arraste::Quadro;
                    self.cor_em(n, p, cx);
                } else if matiz_alvo.contains(p) {
                    cx.states.get_mut::<Paleta>(n.key).arraste = Arraste::Matiz;
                    self.cor_em(n, p, cx);
                } else if g.prontas.contains(p) {
                    let i = ((p.x - g.prontas.x) / 27.5) as usize;
                    if let Some(c) = PRONTAS.get(i).and_then(|c| Color::hex(c)) {
                        cx.states.get_mut::<Paleta>(n.key).matiz = None;
                        cx.publish((self.on_change)(c));
                    }
                }
                Captured::Yes
            }
            Event::Key { key, text, pressed: true, .. } if aberto && cx.states.get::<Paleta>(n.key).is_some_and(|p| p.hex.is_some()) => {
                let st = cx.states.get_mut::<Paleta>(n.key);
                let buf = st.hex.as_mut().expect("conferido no guarda");
                match key {
                    Key::Named(NamedKey::Backspace) => drop(buf.pop()),
                    Key::Named(NamedKey::Enter) => st.hex = None,
                    Key::Named(NamedKey::Escape) => {
                        st.hex = None;
                        st.aberto = false;
                    }
                    _ => {
                        // Aceita só dígitos hex; o `#` digitado é ignorado.
                        let novos = text.as_deref().unwrap_or("").chars().filter(|c| c.is_ascii_hexdigit());
                        buf.extend(novos.map(|c| c.to_ascii_lowercase()));
                        buf.truncate(6);
                    }
                }
                cx.redraw = true;
                let pronto = st.hex.as_deref().filter(|b| b.len() == 6).and_then(|b| Color::hex(b));
                if let Some(c) = pronto {
                    st.matiz = None;
                    cx.publish((self.on_change)(c.alpha(self.value.a)));
                }
                Captured::Yes
            }
            Event::CursorMoved(p) if arraste != Arraste::Nada => {
                self.cor_em(n, *p, cx);
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) if arraste != Arraste::Nada => {
                cx.states.get_mut::<Paleta>(n.key).arraste = Arraste::Nada;
                Captured::Yes
            }
            Event::MousePressed(MouseButton::Left) if p.is_some_and(|p| n.rect.contains(p)) => {
                *cx.states.get_mut::<Paleta>(n.key) = Paleta { aberto: true, ..Default::default() };
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key: Key::Named(NamedKey::Escape), pressed: true, .. } if aberto => {
                cx.states.get_mut::<Paleta>(n.key).aberto = false;
                cx.redraw = true;
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

impl<'a, M: 'a> From<DatePicker<'a, M>> for Element<'a, M> {
    fn from(w: DatePicker<'a, M>) -> Self {
        Element::new(w)
    }
}

impl<'a, M: 'a> From<ColorPicker<'a, M>> for Element<'a, M> {
    fn from(w: ColorPicker<'a, M>) -> Self {
        Element::new(w)
    }
}
