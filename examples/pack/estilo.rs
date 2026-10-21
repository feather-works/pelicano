//! O visual do pack de jogos (Baralho Pixelado): paleta, fontes e os botões e placas pixelados.
//! Cada jogo inclui com `#[path = "pack/estilo.rs"] mod estilo;`.
#![allow(dead_code)] // cada jogo usa uma parte
use pelicano::*;

// O letreiro na fonte das mesas do baralho; o resto na Pixelify, que tem as maiúsculas acentuadas.
pub const LETREIRO: Font = Font::with_name("Press Start 2P");
pub const PIXEL: Font = Font::with_name("Pixelify Sans");
/// O pixel da arte (bordas, sombras duras, desenhos), em pixels da tela.
pub const PX: f32 = 4.0;

// A paleta do Baralho Pixelado.
pub const FUNDO: Color = Color::rgb8(0x1c, 0x1a, 0x2b);
pub const PAINEL: Color = Color::rgb8(0x22, 0x20, 0x2f);
pub const OURO: Color = Color::rgb8(0xf4, 0xb4, 0x1a);
pub const OURO_ESCURO: Color = Color::rgb8(0xc1, 0x7f, 0x0b);
pub const MARINHO: Color = Color::rgb8(0x1b, 0x1e, 0x3f);
pub const MADEIRA: Color = Color::rgb8(0x9c, 0x64, 0x38);
pub const MADEIRA_ESCURA: Color = Color::rgb8(0x5c, 0x36, 0x20);
pub const MADEIRA_CLARA: Color = Color::rgb8(0xe8, 0xc0, 0x8a);
pub const FELTRO: Color = Color::rgb8(0x2e, 0x7a, 0x4c);
pub const FELTRO_CLARO: Color = Color::rgb8(0x37, 0x90, 0x5a);
pub const FELTRO_ESCURO: Color = Color::rgb8(0x28, 0x6b, 0x43);
pub const CREME: Color = Color::rgb8(0xf2, 0xe3, 0xc2);
pub const MENTA: Color = Color::rgb8(0x9f, 0xd8, 0xb2);
pub const ROSA: Color = Color::rgb8(0xe8, 0x40, 0x5a);

pub fn texto(s: impl Into<String>, tamanho: f32, cor: Color) -> Text<'static> {
    text(s.into()).font(PIXEL).size(tamanho).color(cor)
}

/// A placa do título, igual às mesas do baralho: borda dourada e sombra dura.
pub fn placa() -> Style {
    Style::default().background(FUNDO).border(Border::rounded(0.0).color(OURO, PX)).shadow(Shadow { color: Color::BLACK, offset: Point::new(PX, PX), blur: 0.0 })
}

pub fn sombra_dura(d: f32) -> Shadow {
    Shadow { color: Color::BLACK.alpha(0.5), offset: Point::new(d, d), blur: 0.0 }
}

/// Botão pixelado: afunda na sombra quando apertado.
pub fn botao<'a, M: Clone + 'a>(rotulo: &'static str, tamanho: f32, msg: Option<M>, ativo: bool) -> Element<'a, M> {
    let corpo = button(container(text(rotulo).font(PIXEL).size(tamanho)).width(Length::FILL).align_x(Align::Center))
        .padding([14u16, 8])
        .width(Length::FILL)
        .on_press_maybe(msg)
        .style(move |_, s| {
            let (fundo, tinta) = if ativo { (OURO, FUNDO) } else { (PAINEL, OURO) };
            let (fundo, tinta) = match s {
                Status::Hovered => (fundo.mix(Color::WHITE, 0.14), tinta),
                Status::Disabled => (fundo, tinta.alpha(0.35)),
                _ => (fundo, tinta),
            };
            let d = if s == Status::Pressed || s == Status::Disabled { 0.0 } else { PX };
            Style::default().background(fundo).text_color(tinta).border(Border::rounded(0.0).color(if s == Status::Disabled { OURO.alpha(0.35) } else { OURO }, PX)).shadow(Shadow { color: Color::BLACK, offset: Point::new(d, d), blur: 0.0 })
        });
    animate(corpo).pressed(Keyframe::default().offset(PX, PX)).transition(Transition::new(60)).into()
}

/// As duas fontes do pack, embutidas no executável.
pub fn carregar_fontes() {
    load_font(include_bytes!("../assets/PressStart2P-Regular.ttf").to_vec());
    load_font(include_bytes!("../assets/PixelifySans.ttf").to_vec());
}
