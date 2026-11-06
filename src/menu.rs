//! Menu flutuante pronto: o do botão direito e o que abre de um botão da barra. Abre onde
//! pedirem, não sai da janela e fecha com clique fora.
use crate::widget::{button, canvas, column, container, mouse_area, responsive, row, stack, text, Element, Space};
use crate::{Align, Border, Color, Length, Padding, Point, Shadow, Status, Style};

/// Um item do menu; [`menu_separator`] faz a linha (com título, se tiver).
#[derive(Clone)]
pub struct MenuItem<M> {
    rotulo: String,
    atalho: String,
    msg: Option<M>,
    marcado: bool,
    separador: bool,
}

/// O item com o `rotulo`; clicar manda `msg` (e o menu fecha pelo app, que troca o estado).
pub fn menu_item<M>(rotulo: impl Into<String>, msg: M) -> MenuItem<M> {
    MenuItem { rotulo: rotulo.into(), atalho: String::new(), msg: Some(msg), marcado: false, separador: false }
}

/// A linha entre grupos; `titulo` vazio é só a linha.
pub fn menu_separator<M>(titulo: impl Into<String>) -> MenuItem<M> {
    MenuItem { rotulo: titulo.into(), atalho: String::new(), msg: None, marcado: false, separador: true }
}

impl<M> MenuItem<M> {
    /// O atalho escrito à direita ("Ctrl+S").
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.atalho = s.into();
        self
    }
    /// O visto à esquerda (opção ligada).
    pub fn checked(mut self, sim: bool) -> Self {
        self.marcado = sim;
        self
    }
    /// Apagado e sem clique.
    pub fn disabled(mut self) -> Self {
        self.msg = None;
        self
    }
}

const LARGURA: f32 = 250.0;
const ALTURA_ITEM: f32 = 27.0;

/// O menu com o canto de cima à esquerda em `canto` (posição na janela), por cima de tudo: ponha
/// por último num `stack` da janela inteira. Perto da borda, abre para dentro. Clique fora manda
/// `fechar`; Esc fica com o app (o `on_unhandled_event`).
pub fn context_menu<'a, M: Clone + 'a>(itens: Vec<MenuItem<M>>, canto: Point, fechar: M) -> Element<'a, M> {
    let fundo = mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(fechar);
    let altura: f32 = itens.iter().map(|i| if i.separador { if i.rotulo.is_empty() { 9.0 } else { 30.0 } } else { ALTURA_ITEM }).sum::<f32>() + 10.0;
    let caixa = responsive(move |janela| {
        let x = canto.x.min(janela.width - LARGURA - 8.0).max(0.0);
        let y = canto.y.min(janela.height - altura - 8.0).max(0.0);
        let lista = column(itens.iter().cloned().map(linha)).spacing(1).width(Length::FILL);
        let caixa = container(lista).width(LARGURA).padding(4).style(|t| {
            Style::default()
                .background(t.surface)
                .border(Border::rounded(8.0).color(t.border, 1.0))
                .shadow(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.3), offset: Point::new(0.0, 8.0), blur: 24.0 })
        });
        container(caixa).padding(Padding { top: y, left: x, right: 0.0, bottom: 0.0 }).into()
    });
    stack([fundo.into(), caixa.into()]).width(Length::FILL).height(Length::FILL).into()
}

fn linha<'a, M: Clone + 'a>(item: MenuItem<M>) -> Element<'a, M> {
    if item.separador {
        let traco = container(Space::new(Length::FILL, 1.0)).style(|t| Style::default().background(t.border));
        let mut c: Vec<Element<M>> = vec![container(traco).padding([4, 6]).into()];
        if !item.rotulo.is_empty() {
            c.push(apagado(text(item.rotulo).size(11)).padding([2, 10]).into());
        }
        return column(c).into();
    }
    let ativo = item.msg.is_some();
    let visto: Element<M> = if item.marcado {
        canvas(13.0, 13.0, |r, a, t| {
            let q = |x: f32, y: f32| Point::new(a.x + x, a.y + y);
            r.stroke(&[q(2.5, 7.0), q(5.5, 10.0), q(10.5, 3.5)], 1.6, t.text);
        })
        .into()
    } else {
        Space::new(13.0, 0.0).into()
    };
    let conteudo = row([visto, text(item.rotulo).size(12).width(Length::FILL).into(), apagado(text(item.atalho).size(11)).into()])
        .spacing(8)
        .align(Align::Center);
    button(conteudo)
        .on_press_maybe(item.msg)
        .width(Length::FILL)
        .padding([5, 10])
        .style(move |t, s| {
            let base = Style::default().border(Border::rounded(5.0)).text_color(if ativo { t.text } else { t.muted });
            if ativo && matches!(s, Status::Hovered | Status::Pressed) {
                base.background(t.text.alpha(0.08))
            } else {
                base
            }
        })
        .into()
}

/// O texto na cor apagada do tema (o `text` herda a cor do contêiner).
fn apagado<'a, M: 'a>(t: crate::widget::Text<'a>) -> crate::widget::Container<'a, M> {
    container(t).style(|t| Style::default().text_color(t.muted))
}
