//! Cores de código no [`crate::text_editor`]: as gramáticas e temas do Sublime (syntect), linha
//! a linha. Liga com a feature `highlighter`.
use cosmic_text::{Attrs, AttrsList, Style as Estilo, Weight};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use syntect::highlighting::{FontStyle, HighlightIterator, HighlightState, Highlighter, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

/// Os temas que vêm junto (os mesmos do `highlighter` do Iced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Highlight {
    SolarizedDark,
    Base16Mocha,
    Base16Ocean,
    Base16Eighties,
    InspiredGitHub,
}

impl Highlight {
    pub const ALL: [Highlight; 5] = [Highlight::SolarizedDark, Highlight::Base16Mocha, Highlight::Base16Ocean, Highlight::Base16Eighties, Highlight::InspiredGitHub];

    pub fn is_dark(self) -> bool {
        self != Highlight::InspiredGitHub
    }

    fn tema(self) -> &'static syntect::highlighting::Theme {
        static TEMAS: OnceLock<ThemeSet> = OnceLock::new();
        let nome = match self {
            Highlight::SolarizedDark => "Solarized (dark)",
            Highlight::Base16Mocha => "base16-mocha.dark",
            Highlight::Base16Ocean => "base16-ocean.dark",
            Highlight::Base16Eighties => "base16-eighties.dark",
            Highlight::InspiredGitHub => "InspiredGitHub",
        };
        &TEMAS.get_or_init(ThemeSet::load_defaults).themes[nome]
    }
}

fn sintaxes() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    S.get_or_init(SyntaxSet::load_defaults_newlines)
}

type Trechos = Vec<(std::ops::Range<usize>, cosmic_text::Color, FontStyle)>;

/// O estado do analisador no começo de cada linha já vista, com o texto que o gerou: editar a
/// linha 500 refaz dali para baixo, e só até onde a tela mostra.
#[derive(Default)]
pub(crate) struct Realce {
    pedido: Option<(String, Highlight)>,
    /// `estados[k]` vale no começo da linha k; tem uma entrada a mais que `linhas`.
    estados: Vec<(ParseState, HighlightState)>,
    linhas: Vec<(u64, Trechos)>,
}

impl Realce {
    /// As cores das linhas `primeira..ultima`, analisando o que falta desde a primeira mudada.
    pub(crate) fn cores(&mut self, ext: &str, tema: Highlight, textos: &[&str], primeira: usize, base: Attrs) -> Vec<AttrsList> {
        let hl = Highlighter::new(tema.tema());
        if self.pedido.as_ref().is_none_or(|(e, t)| e != ext || *t != tema) {
            let ss = sintaxes();
            let sintaxe = ss.find_syntax_by_extension(ext).or_else(|| ss.find_syntax_by_token(ext)).unwrap_or_else(|| ss.find_syntax_plain_text());
            *self = Realce { pedido: Some((ext.to_owned(), tema)), estados: vec![(ParseState::new(sintaxe), HighlightState::new(&hl, ScopeStack::new()))], linhas: Vec::new() };
        }
        let mudou = textos.iter().zip(&self.linhas).position(|(t, (h, _))| hash(t) != *h).unwrap_or(self.linhas.len());
        self.linhas.truncate(mudou);
        self.estados.truncate(mudou + 1);
        let mut linha = String::new();
        for t in textos.get(self.linhas.len()..).unwrap_or_default() {
            let (mut parse, mut estado) = self.estados.last().expect("sempre há o do começo").clone();
            // As gramáticas "newlines" esperam o \n no fim.
            linha.clear();
            linha.push_str(t);
            linha.push('\n');
            let ops = parse.parse_line(&linha, sintaxes()).unwrap_or_default();
            let mut trechos = Vec::new();
            let mut pos = 0;
            for (s, trecho) in HighlightIterator::new(&mut estado, &ops, &linha, &hl) {
                let fim = (pos + trecho.len()).min(t.len());
                if pos < fim {
                    let c = s.foreground;
                    trechos.push((pos..fim, cosmic_text::Color::rgba(c.r, c.g, c.b, c.a), s.font_style));
                }
                pos += trecho.len();
            }
            self.linhas.push((hash(t), trechos));
            self.estados.push((parse, estado));
        }
        self.linhas[primeira.min(textos.len())..textos.len()]
            .iter()
            .map(|(_, trechos)| {
                let mut a = AttrsList::new(base);
                for (r, cor, f) in trechos {
                    let mut at = base.color(*cor);
                    if f.contains(FontStyle::BOLD) {
                        at = at.weight(Weight::BOLD);
                    }
                    if f.contains(FontStyle::ITALIC) {
                        at = at.style(Estilo::Italic);
                    }
                    a.add_span(r.clone(), at);
                }
                a
            })
            .collect()
    }
}

fn hash(t: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn colore_e_refaz_so_dali_para_baixo() {
        let mut r = Realce::default();
        let base = Attrs::new();
        let cor = |a: &AttrsList, i: usize| a.get_span(i).color_opt;
        let texto = ["fn main() {", "    let s = \"oi\";", "}"];
        let v = r.cores("rs", Highlight::Base16Ocean, &texto, 0, base);
        assert_eq!(v.len(), 3);
        assert_ne!(cor(&v[0], 0), cor(&v[1], 13), "palavra-chave e texto entre aspas com cores diferentes");
        let antes = cor(&v[1], 13);
        // Abrir aspas na linha 1 faz o resto virar string: a linha 2 muda de cor.
        let texto2 = ["fn main() {", "    let s = \"oi;", "}"];
        let v2 = r.cores("rs", Highlight::Base16Ocean, &texto2, 1, base);
        assert_eq!(v2.len(), 2);
        assert_eq!(cor(&v2[1], 0), antes, "a chave de baixo virou parte da string");
    }
}
