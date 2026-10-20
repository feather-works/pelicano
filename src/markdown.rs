//! Markdown (CommonMark com tabelas, tachado e tarefas) montado com os widgets de sempre:
//! parágrafos em [`rich_text`], listas, citações, blocos de código e tabelas.
use crate::rich::{rich_text, span, Span};
use crate::widget::{column, container, row, text, Element, Space, Style};
use crate::{Align, Border, Color, Font, Length};
use pulldown_cmark::{Event as Ev, HeadingLevel, Options, Parser, Tag, TagEnd};

enum Tipo {
    Raiz,
    Citacao,
    /// O número do próximo item (lista numerada) ou `None` (marcadores).
    Lista(Option<u64>),
    Item,
    Tabela,
    Linha(bool),
}

struct Bloco<'a, M> {
    tipo: Tipo,
    filhos: Vec<Element<'a, M>>,
}

/// Fundo neutro que serve nos dois temas (código em linha e em bloco).
const FUNDO_CODIGO: Color = Color { r: 0.5, g: 0.5, b: 0.5, a: 0.16 };

/// O texto `src` em Markdown, com letra de `size`. Clicar num link manda `on_link(endereço)`.
pub fn markdown<'a, M: Clone + 'a>(src: &str, size: f32, on_link: impl Fn(String) -> M) -> Element<'a, M> {
    let mut pilha: Vec<Bloco<'a, M>> = vec![Bloco { tipo: Tipo::Raiz, filhos: Vec::new() }];
    let mut spans: Vec<Span<'a, M>> = Vec::new();
    let (mut negrito, mut italico, mut tachado) = (0u32, 0u32, 0u32);
    let mut link: Option<M> = None;
    let mut titulo: Option<HeadingLevel> = None;
    let mut codigo: Option<String> = None;

    let tam_titulo = |h: HeadingLevel| {
        size * match h {
            HeadingLevel::H1 => 1.6,
            HeadingLevel::H2 => 1.35,
            HeadingLevel::H3 => 1.15,
            _ => 1.0,
        }
    };
    // O parágrafo aberto vira um rich_text no bloco de cima.
    let fechar = |spans: &mut Vec<Span<'a, M>>, pilha: &mut Vec<Bloco<'a, M>>, titulo: Option<HeadingLevel>| {
        if spans.is_empty() {
            return;
        }
        let tam = titulo.map_or(size, tam_titulo);
        let p = rich_text(std::mem::take(spans)).size(tam).width(Length::FILL);
        pilha.last_mut().expect("raiz").filhos.push(p.into());
    };

    let opcoes = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for ev in Parser::new_ext(src, opcoes) {
        // Texto solto antes de um bloco (item de lista apertada com sublista) fecha antes dele.
        if matches!(ev, Ev::Start(Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::CodeBlock(_) | Tag::Table(_))) {
            fechar(&mut spans, &mut pilha, None);
        }
        match ev {
            Ev::Start(Tag::Heading { level, .. }) => titulo = Some(level),
            Ev::End(TagEnd::Heading(_)) => {
                fechar(&mut spans, &mut pilha, titulo);
                titulo = None;
            }
            Ev::End(TagEnd::Paragraph) => fechar(&mut spans, &mut pilha, None),
            Ev::Start(Tag::BlockQuote(_)) => pilha.push(Bloco { tipo: Tipo::Citacao, filhos: Vec::new() }),
            Ev::Start(Tag::List(inicio)) => pilha.push(Bloco { tipo: Tipo::Lista(inicio), filhos: Vec::new() }),
            Ev::Start(Tag::Item) => pilha.push(Bloco { tipo: Tipo::Item, filhos: Vec::new() }),
            Ev::Start(Tag::Table(_)) => pilha.push(Bloco { tipo: Tipo::Tabela, filhos: Vec::new() }),
            Ev::Start(Tag::TableHead) => {
                negrito += 1;
                pilha.push(Bloco { tipo: Tipo::Linha(true), filhos: Vec::new() });
            }
            Ev::Start(Tag::TableRow) => pilha.push(Bloco { tipo: Tipo::Linha(false), filhos: Vec::new() }),
            Ev::End(TagEnd::TableCell) => {
                let celula = rich_text(std::mem::take(&mut spans)).size(size);
                let c = container(celula).padding([4u16, 8]).width(Length::FILL);
                pilha.last_mut().expect("linha").filhos.push(c.into());
            }
            Ev::Start(Tag::CodeBlock(_)) => codigo = Some(String::new()),
            Ev::End(TagEnd::CodeBlock) => {
                let t = codigo.take().unwrap_or_default();
                let bloco = container(text(t.trim_end_matches('\n').to_owned()).font(Font::MONO).size(size - 1.0)).padding(10.0).width(Length::FILL).style(|_| Style::default().background(FUNDO_CODIGO).border(Border::rounded(6.0)));
                pilha.last_mut().expect("raiz").filhos.push(bloco.into());
            }
            Ev::End(TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow) => {
                // Item sem parágrafo ("- texto" numa lista apertada): o texto solto fecha aqui.
                fechar(&mut spans, &mut pilha, None);
                let b = pilha.pop().expect("aberto antes");
                if matches!(b.tipo, Tipo::Linha(true)) {
                    negrito = negrito.saturating_sub(1);
                }
                let pai = pilha.last_mut().expect("raiz");
                let el: Element<'a, M> = match b.tipo {
                    Tipo::Citacao => container(column(b.filhos).spacing(size * 0.5)).padding([6u16, 12]).width(Length::FILL).style(|t| Style::default().background(t.primary.alpha(0.08)).border(Border::rounded(6.0))).into(),
                    Tipo::Lista(_) => column(b.filhos).spacing(size * 0.3).into(),
                    Tipo::Item => {
                        let marca = match &mut pai.tipo {
                            Tipo::Lista(Some(n)) => {
                                *n += 1;
                                format!("{}.", *n - 1)
                            }
                            _ => "•".to_owned(),
                        };
                        row([text(marca).size(size).into(), column(b.filhos).spacing(size * 0.3).width(Length::FILL).into()]).spacing(8).align(Align::Start).into()
                    }
                    Tipo::Tabela => container(column(b.filhos)).width(Length::FILL).style(|t| Style::default().border(Border::rounded(6.0).color(t.border, 1.0))).into(),
                    Tipo::Linha(cabecalho) => {
                        let linha = row(b.filhos).width(Length::FILL);
                        if cabecalho {
                            container(linha).width(Length::FILL).style(|_| Style::default().background(FUNDO_CODIGO)).into()
                        } else {
                            linha.into()
                        }
                    }
                    Tipo::Raiz => unreachable!("a raiz não fecha"),
                };
                pai.filhos.push(el);
            }
            Ev::Start(Tag::Emphasis) => italico += 1,
            Ev::End(TagEnd::Emphasis) => italico = italico.saturating_sub(1),
            Ev::Start(Tag::Strong) => negrito += 1,
            Ev::End(TagEnd::Strong) => negrito = negrito.saturating_sub(1),
            Ev::Start(Tag::Strikethrough) => tachado += 1,
            Ev::End(TagEnd::Strikethrough) => tachado = tachado.saturating_sub(1),
            Ev::Start(Tag::Link { dest_url, .. }) => link = Some(on_link(dest_url.into_string())),
            Ev::End(TagEnd::Link) => link = None,
            Ev::Start(Tag::Image { .. }) => italico += 1,
            Ev::End(TagEnd::Image) => italico = italico.saturating_sub(1),
            Ev::Text(t) if codigo.is_some() => codigo.as_mut().expect("conferido").push_str(&t),
            Ev::Text(t) | Ev::Html(t) | Ev::InlineHtml(t) | Ev::InlineMath(t) | Ev::DisplayMath(t) => {
                let peso = if negrito > 0 || titulo.is_some_and(|h| h <= HeadingLevel::H3) { Font::BOLD } else if titulo.is_some() { Font::SEMIBOLD } else { Font::DEFAULT };
                let mut s = span(t.into_string()).font(if italico > 0 { peso.italic() } else { peso });
                if tachado > 0 {
                    s = s.strikethrough();
                }
                if let Some(m) = &link {
                    s = s.link(m.clone());
                }
                spans.push(s);
            }
            Ev::Code(t) => {
                let mut s = span(t.into_string()).font(Font::MONO).size(size - 1.0).background(FUNDO_CODIGO);
                if let Some(m) = &link {
                    s = s.link(m.clone());
                }
                spans.push(s);
            }
            Ev::SoftBreak => spans.push(span(" ")),
            Ev::HardBreak => spans.push(span("\n")),
            Ev::TaskListMarker(feito) => spans.push(span(if feito { "☑ " } else { "☐ " })),
            Ev::Rule => {
                let linha = container(Space::new(Length::FILL, 1.0)).style(|t| Style::default().background(t.border));
                pilha.last_mut().expect("raiz").filhos.push(linha.into());
            }
            _ => {}
        }
    }
    fechar(&mut spans, &mut pilha, None);
    let raiz = pilha.into_iter().next().expect("raiz");
    column(raiz.filhos).spacing(size * 0.6).width(Length::FILL).into()
}
