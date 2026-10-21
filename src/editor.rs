//! Editor de texto de várias linhas: o `Editor` do cosmic-text (cursor, seleção, quebra de
//! linha, e só as linhas visíveis montadas) com desfazer/refazer, área de transferência,
//! localizar/substituir e números de linha.
use crate::widget::{CursorIcon, DrawCx, EventCx, Node, Style, Widget};
use crate::{Border, Captured, Color, Event, Font, Key, LayoutCx, Length, Limits, MouseButton, NamedKey, Padding, Point, Rect, Size, Status, Theme};
use cosmic_text::{Action, Attrs, AttrsList, Buffer, Change, Cursor, Edit, Editor, Family, FontSystem, Metrics, Motion, Selection, Shaping, Style as Estilo, Weight, Wrap};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};

/// O texto de um [`text_editor`], guardado pelo app. Barato até aparecer na tela: o texto só
/// vira linhas montadas (e só as visíveis) no primeiro desenho.
pub struct Content(RefCell<Interno>);

struct Interno {
    ed: Option<Editor<'static>>,
    /// O texto antes do primeiro desenho (sem fontes ainda não dá para montar o buffer).
    inicial: String,
    /// A linha pedida por [`Content::go_to_line`] antes do primeiro desenho.
    linha_inicial: Option<usize>,
    /// Sobe a cada mudança do texto: o app compara para saber se há algo a salvar.
    versao: u64,
    desfazer: Vec<Change>,
    refazer: Vec<Change>,
    /// A última mudança foi digitação: letras seguidas viram um passo só no desfazer.
    digitando: Option<std::time::Instant>,
    /// (tamanho físico × 64, altura da linha × 64, fonte, quebra) com que o buffer está montado.
    config: Option<(u32, u32, Font, bool)>,
    /// (versão, primeira linha à vista) -> se ela começa dentro de um bloco ```.
    cercas: Option<(u64, usize, bool)>,
    #[cfg(feature = "highlighter")]
    realce: crate::highlight::Realce,
}

const DESFAZER_MAX: usize = 500;

impl Default for Content {
    fn default() -> Self {
        Content::with_text("")
    }
}

impl Content {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(texto: &str) -> Self {
        Content(RefCell::new(Interno { ed: None, inicial: texto.to_owned(), linha_inicial: None, versao: 0, desfazer: Vec::new(), refazer: Vec::new(), digitando: None, config: None, cercas: None, #[cfg(feature = "highlighter")] realce: Default::default() }))
    }

    /// O texto inteiro, com as quebras de linha que o arquivo tinha.
    pub fn text(&self) -> String {
        let i = self.0.borrow();
        match &i.ed {
            None => i.inicial.clone(),
            Some(ed) => ed.with_buffer(|b| {
                let mut s = String::new();
                let n = b.lines.len();
                for (k, l) in b.lines.iter().enumerate() {
                    s.push_str(l.text());
                    // O cosmic-text dá "sem quebra" às linhas criadas depois da última (Enter, colar
                    // no fim): se há linha depois, a quebra existe.
                    s.push_str(match l.ending().as_str() {
                        "" if k + 1 < n => "\n",
                        e => e,
                    });
                }
                s
            }),
        }
    }

    /// O texto selecionado, se houver.
    pub fn selection(&self) -> Option<String> {
        self.0.borrow().ed.as_ref()?.copy_selection()
    }

    /// O texto da linha `linha` (contando de 1, como o [`Content::cursor`]), sem a quebra.
    pub fn line(&self, linha: usize) -> Option<String> {
        let i = self.0.borrow();
        let k = linha.checked_sub(1)?;
        match &i.ed {
            None => i.inicial.lines().nth(k).map(str::to_owned),
            Some(ed) => ed.with_buffer(|b| b.lines.get(k).map(|l| l.text().to_owned())),
        }
    }

    /// Escreve `texto` no cursor, no lugar da seleção, num passo do desfazer.
    pub fn insert(&self, texto: &str) {
        let mut i = self.0.borrow_mut();
        if i.ed.is_none() {
            i.inicial.push_str(texto);
            i.versao += 1;
            return;
        }
        i.editar(false, |ed| {
            ed.delete_selection();
            ed.insert_string(texto, None);
        });
    }

    /// Muda a cada edição (não com cursor ou rolagem).
    pub fn version(&self) -> u64 {
        self.0.borrow().versao
    }

    /// Linha e coluna do cursor, contando de 1.
    pub fn cursor(&self) -> (usize, usize) {
        let i = self.0.borrow();
        match &i.ed {
            None => (1, 1),
            Some(ed) => {
                let c = ed.cursor();
                let col = ed.with_buffer(|b| b.lines.get(c.line).map_or(0, |l| l.text()[..c.index.min(l.text().len())].chars().count()));
                (c.line + 1, col + 1)
            }
        }
    }

    pub fn line_count(&self) -> usize {
        let i = self.0.borrow();
        match &i.ed {
            None => i.inicial.lines().count().max(1),
            Some(ed) => ed.with_buffer(|b| b.lines.len()),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.0.borrow().desfazer.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.0.borrow().refazer.is_empty()
    }

    pub fn undo(&self) {
        self.0.borrow_mut().voltar(true);
    }

    pub fn redo(&self) {
        self.0.borrow_mut().voltar(false);
    }

    /// Leva o cursor ao começo da linha `linha` (contando de 1; passa do fim, vai à última).
    /// Antes do primeiro desenho, vale quando o editor for montado.
    pub fn go_to_line(&self, linha: usize) {
        let mut i = self.0.borrow_mut();
        let Some(ed) = i.ed.as_mut() else {
            i.linha_inicial = Some(linha);
            return;
        };
        let n = ed.with_buffer(|b| b.lines.len());
        ed.set_selection(Selection::None);
        ed.set_cursor(Cursor::new(linha.clamp(1, n) - 1, 0));
        i.digitando = None;
    }

    /// Põe `antes` e `depois` em volta da seleção (sem seleção, no cursor, que fica no meio).
    /// Se já estão lá, tira: o mesmo atalho liga e desliga. `inicio` diz onde o texto de uma linha
    /// começa (depois de um `## `, por exemplo): a seleção nunca começa antes disso.
    pub fn wrap_selection(&self, antes: &str, depois: &str, inicio: impl Fn(&str) -> usize) {
        let mut i = self.0.borrow_mut();
        let Some(ed) = i.ed.as_ref() else { return };
        let (mut a, mut z) = ed.selection_bounds().unwrap_or((ed.cursor(), ed.cursor()));
        let p = ed.with_buffer(|b| inicio(b.lines[a.line].text()));
        a.index = a.index.max(p);
        if z.line == a.line {
            z.index = z.index.max(p);
        }
        let (ka, kd) = (antes.len(), depois.len());
        // Onde estão os sinais que já envolvem a seleção: o fim do da esquerda e o começo do da direita.
        let ja_tem = ed.with_buffer(|b| {
            let (la, lz) = (b.lines[a.line].text(), b.lines[z.line].text());
            if antes != depois {
                return (a.index >= ka && la.get(a.index - ka..a.index) == Some(antes) && lz.get(z.index..z.index + kd) == Some(depois)).then_some((a.index, z.index));
            }
            // Os sinais podem estar por fora de outros (***~~oi~~***): procura o grupo de `c` nas
            // sequências de sinais coladas na seleção. "*" junto de "**" é o negrito: um sinal de uma
            // letra só conta se o grupo dele é ímpar (*oi* ou ***oi***).
            let c = antes.chars().next()?;
            let sinal = |x: char| "*_~`".contains(x);
            let esq = &la[la[..a.index].trim_end_matches(sinal).len()..a.index];
            let dir = &lz[z.index..lz.len() - lz[z.index..].trim_start_matches(sinal).len()];
            let vale = |n: usize| if ka > 1 { n >= ka } else { n % 2 == 1 };
            // Os grupos de `c` de dentro para fora; vale o primeiro que serve de cada lado.
            let grupos = |run: &str| {
                let mut g: Vec<(usize, usize)> = Vec::new();
                for (p, x) in run.char_indices() {
                    match g.last_mut() {
                        Some((ini, n)) if x == c && *ini + *n == p => *n += 1,
                        _ if x == c => g.push((p, 1)),
                        _ => {}
                    }
                }
                g
            };
            let base = a.index - esq.len();
            let fim = grupos(esq).into_iter().rev().find(|&(_, n)| vale(n)).map(|(p, n)| base + p + n)?;
            let ini = grupos(dir).into_iter().find(|&(_, n)| vale(n)).map(|(p, _)| z.index + p)?;
            Some((fim, ini))
        });
        i.editar(false, |ed| {
            let mesma = a.line == z.line;
            if let Some((fim, ini)) = ja_tem {
                ed.delete_range(Cursor::new(z.line, ini), Cursor::new(z.line, ini + kd));
                ed.delete_range(Cursor::new(a.line, fim - ka), Cursor::new(a.line, fim));
                let cursor = Cursor::new(z.line, if mesma { z.index - ka } else { z.index });
                ed.set_selection(if a == z { Selection::None } else { Selection::Normal(Cursor::new(a.line, a.index - ka)) });
                ed.set_cursor(cursor);
            } else {
                ed.insert_at(z, depois, None);
                ed.insert_at(a, antes, None);
                let fim = Cursor::new(z.line, if mesma { z.index + ka } else { z.index });
                ed.set_selection(if a == z { Selection::None } else { Selection::Normal(Cursor::new(a.line, a.index + ka)) });
                ed.set_cursor(fim);
            }
        });
    }

    /// Troca o começo de cada linha da seleção (ou da linha do cursor): `f` recebe a linha e
    /// devolve quantos bytes do começo saem e o que entra no lugar.
    pub fn edit_lines(&self, f: impl Fn(&str) -> (usize, String)) {
        let mut i = self.0.borrow_mut();
        let Some(ed) = i.ed.as_ref() else { return };
        let (a, z) = ed.selection_bounds().unwrap_or((ed.cursor(), ed.cursor()));
        let cursor = ed.cursor();
        i.editar(false, |ed| {
            let mut desloca = 0isize;
            for linha in a.line..=z.line {
                let texto = ed.with_buffer(|b| b.lines[linha].text().to_owned());
                let (sai, entra) = f(&texto);
                let sai = sai.min(texto.len());
                ed.delete_range(Cursor::new(linha, 0), Cursor::new(linha, sai));
                ed.insert_at(Cursor::new(linha, 0), &entra, None);
                if linha == cursor.line {
                    desloca = entra.len() as isize - sai as isize;
                }
            }
            ed.set_selection(Selection::None);
            ed.set_cursor(Cursor::new(cursor.line, (cursor.index as isize + desloca).max(0) as usize));
        });
    }

    /// Troca o texto de cada linha por `f(linha)`, num passo só do desfazer. As quebras de linha
    /// ficam como estão (consertos que não mudam as linhas, como o de acentos).
    pub fn map_lines(&self, f: impl Fn(&str) -> String) {
        let mut i = self.0.borrow_mut();
        if i.ed.is_none() {
            let novo: String = i.inicial.split_inclusive('\n').map(|l| {
                let corpo = l.trim_end_matches(['\r', '\n']);
                f(corpo) + &l[corpo.len()..]
            }).collect();
            i.inicial = novo;
            i.versao += 1;
            return;
        }
        i.editar(false, |ed| {
            let n = ed.with_buffer(|b| b.lines.len());
            for linha in 0..n {
                let velho = ed.with_buffer(|b| b.lines[linha].text().to_owned());
                let novo = f(&velho);
                if novo != velho {
                    ed.delete_range(Cursor::new(linha, 0), Cursor::new(linha, velho.len()));
                    ed.insert_at(Cursor::new(linha, 0), &novo, None);
                }
            }
            ed.set_selection(Selection::None);
            ed.set_cursor(Cursor::new(0, 0));
        });
    }

    /// O caractere logo depois do cursor.
    pub fn char_at_cursor(&self) -> Option<char> {
        let i = self.0.borrow();
        let ed = i.ed.as_ref()?;
        let c = ed.cursor();
        ed.with_buffer(|b| b.lines.get(c.line)?.text().get(c.index..)?.chars().next())
    }

    /// Quantos caracteres do texto passam em `f` (sem juntar o texto numa String).
    pub fn count_chars(&self, f: impl Fn(char) -> bool) -> usize {
        let i = self.0.borrow();
        match &i.ed {
            None => i.inicial.chars().filter(|&c| f(c)).count(),
            Some(ed) => ed.with_buffer(|b| b.lines.iter().map(|l| l.text().chars().filter(|&c| f(c)).count()).sum()),
        }
    }

    /// Seleciona o próximo caractere depois do cursor que passa em `f` (volta ao começo no fim).
    pub fn find_char(&self, f: impl Fn(char) -> bool) -> bool {
        let mut i = self.0.borrow_mut();
        let Some(ed) = i.ed.as_mut() else { return false };
        let desde = ed.selection_bounds().map_or(ed.cursor(), |(_, fim)| fim);
        let achado = ed.with_buffer(|b| {
            let n = b.lines.len();
            (0..=n).find_map(|k| {
                let linha = (desde.line + k) % n;
                let texto = b.lines[linha].text();
                let inicio = if k == 0 { desde.index } else { 0 };
                texto.char_indices().find(|&(a, c)| a >= inicio && f(c)).map(|(a, c)| (linha, a, a + c.len_utf8()))
            })
        });
        let Some((linha, a, z)) = achado else { return false };
        ed.set_selection(Selection::Normal(Cursor::new(linha, a)));
        ed.set_cursor(Cursor::new(linha, z));
        true
    }

    /// Seleciona a próxima ocorrência de `termo` depois do cursor (volta ao começo no fim).
    /// `false` se não há nenhuma.
    pub fn find_next(&self, termo: &str, maiusculas: bool) -> bool {
        let mut i = self.0.borrow_mut();
        let Some(ed) = i.ed.as_mut() else { return false };
        if termo.is_empty() {
            return false;
        }
        let desde = ed.selection_bounds().map_or(ed.cursor(), |(_, fim)| fim);
        let achado = ed.with_buffer(|b| {
            let n = b.lines.len();
            (0..=n).find_map(|k| {
                let linha = (desde.line + k) % n;
                let inicio = if k == 0 { desde.index } else { 0 };
                let fim = if k == n { desde.index } else { usize::MAX };
                achar(b.lines[linha].text(), termo, inicio, maiusculas).filter(|&(a, _)| a < fim || k < n).map(|(a, z)| (linha, a, z))
            })
        });
        let Some((linha, a, z)) = achado else { return false };
        ed.set_selection(Selection::Normal(Cursor::new(linha, a)));
        ed.set_cursor(Cursor::new(linha, z));
        true
    }

    /// Troca a seleção por `novo` se ela for uma ocorrência de `termo`, e seleciona a próxima.
    pub fn replace_next(&self, termo: &str, novo: &str, maiusculas: bool) -> bool {
        {
            let mut i = self.0.borrow_mut();
            let selecionado = i.ed.as_ref().and_then(|ed| ed.copy_selection());
            if selecionado.is_some_and(|s| achar(&s, termo, 0, maiusculas) == Some((0, s.len()))) {
                i.editar(false, |ed| {
                    ed.delete_selection();
                    ed.insert_string(novo, None);
                });
            }
        }
        self.find_next(termo, maiusculas)
    }

    /// Troca todas as ocorrências; devolve quantas. Um passo só no desfazer.
    pub fn replace_all(&self, termo: &str, novo: &str, maiusculas: bool) -> usize {
        if termo.is_empty() {
            return 0;
        }
        let mut i = self.0.borrow_mut();
        if i.ed.is_none() {
            let (texto, n) = trocar_tudo(&i.inicial, termo, novo, maiusculas);
            i.inicial = texto;
            i.versao += n as u64;
            return n;
        }
        let mut n = 0;
        i.editar(false, |ed| {
            let linhas = ed.with_buffer(|b| b.lines.len());
            for linha in 0..linhas {
                let mut desde = 0;
                while let Some((a, z)) = ed.with_buffer(|b| achar(b.lines[linha].text(), termo, desde, maiusculas)) {
                    ed.delete_range(Cursor::new(linha, a), Cursor::new(linha, z));
                    ed.insert_at(Cursor::new(linha, a), novo, None);
                    desde = a + novo.len();
                    n += 1;
                }
            }
            ed.set_selection(Selection::None);
        });
        n
    }
}

impl Interno {
    /// O editor montado com as fontes do renderer, na configuração pedida.
    fn editor(&mut self, fonts: &mut FontSystem, tamanho: f32, altura: f32, escala: f32, fonte: Font, quebra: bool) -> &mut Editor<'static> {
        let config = ((tamanho * escala * 64.0) as u32, (altura * escala * 64.0) as u32, fonte, quebra);
        let metricas = Metrics::new(tamanho * escala, (altura * escala).ceil());
        let attrs = crate::render::attrs(fonte);
        let ed = self.ed.get_or_insert_with(|| {
            let mut b = Buffer::new(fonts, metricas);
            // Sem altura, o set_text monta todas as linhas; com zero, nenhuma (o desenho monta as visíveis).
            b.set_size(fonts, None, Some(0.0));
            b.set_text(fonts, &std::mem::take(&mut self.inicial), attrs, Shaping::Advanced);
            let mut ed = Editor::new(b);
            if let Some(linha) = self.linha_inicial.take() {
                let n = ed.with_buffer(|b| b.lines.len());
                ed.set_cursor(Cursor::new(linha.clamp(1, n) - 1, 0));
            }
            ed
        });
        if self.config != Some(config) {
            let outra_fonte = self.config.is_some_and(|(_, _, f, _)| f != fonte);
            let cursor = ed.cursor();
            ed.with_buffer_mut(|b| {
                b.set_metrics(fonts, metricas);
                b.set_wrap(fonts, if quebra { Wrap::WordOrGlyph } else { Wrap::None });
                // Outra fonte: remonta o mesmo texto (o desfazer continua valendo, as posições não mudam).
                if outra_fonte {
                    let texto: String = b.lines.iter().map(|l| format!("{}{}", l.text(), l.ending().as_str())).collect();
                    let rolagem = b.scroll();
                    b.set_text(fonts, &texto, attrs, Shaping::Advanced);
                    b.set_scroll(rolagem);
                }
            });
            if outra_fonte {
                ed.set_cursor(cursor);
            }
            self.config = Some(config);
        }
        ed
    }

    /// Uma edição: guarda o que mudou para o desfazer (letras seguidas juntam num passo).
    fn editar(&mut self, digitacao: bool, f: impl FnOnce(&mut Editor<'static>)) {
        let Some(ed) = self.ed.as_mut() else { return };
        ed.start_change();
        f(ed);
        let Some(mudou) = ed.finish_change().filter(|c| !c.items.is_empty()) else { return };
        let agora = std::time::Instant::now();
        let junta = digitacao && self.digitando.is_some_and(|t| agora.duration_since(t).as_millis() < 1500);
        match self.desfazer.last_mut() {
            Some(ultimo) if junta => ultimo.items.extend(mudou.items),
            _ => self.desfazer.push(mudou),
        }
        // Cada passo guarda o texto mexido: sem teto, uma sessão longa só cresce.
        if self.desfazer.len() > DESFAZER_MAX {
            self.desfazer.remove(0);
        }
        self.digitando = digitacao.then_some(agora);
        self.refazer.clear();
        self.versao += 1;
    }

    fn voltar(&mut self, desfazer: bool) {
        let Some(ed) = self.ed.as_mut() else { return };
        let pilha = if desfazer { &mut self.desfazer } else { &mut self.refazer };
        let Some(mut c) = pilha.pop() else { return };
        c.reverse();
        let antes = ed.cursor();
        ed.set_selection(Selection::None);
        ed.apply_change(&c);
        // O apply_change move o cursor sem avisar a rolagem; ida e volta faz ela seguir.
        let depois = ed.cursor();
        ed.set_cursor(antes);
        ed.set_cursor(depois);
        if desfazer { &mut self.refazer } else { &mut self.desfazer }.push(c);
        self.digitando = None;
        self.versao += 1;
    }
}

/// Caractere que não aparece (ou parece um espaço comum): controles, larguras zero, marcas
/// de direção, BOM, NBSP e os outros espaços do Unicode. Tab e quebra de linha não contam.
pub fn is_invisible(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{a0}' | '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}' | '\u{1160}' | '\u{180e}' | '\u{2000}'..='\u{200f}' | '\u{2028}'..='\u{202f}' | '\u{205f}'..='\u{206f}' | '\u{3000}' | '\u{3164}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}')
}

// ---------- Markdown ----------

/// Marcas que o desenho completa (o cosmic-text não risca nem sublinha): vão no `metadata` do glifo.
const TACHADO: usize = 1;
const LINK: usize = 2;
const CAIXA: usize = 3;
const CAIXA_MARCADA: usize = 4;
const MARCADOR: usize = 5;
const CITACAO: usize = 6;

/// Onde fica o "[" da caixinha de uma linha de tarefa ("- [ ] …"), e se está marcada.
fn caixa_da_linha(texto: &str) -> Option<(usize, bool)> {
    let recuo = texto.len() - texto.trim_start().len();
    let resto = &texto[recuo..];
    let marca = ["- ", "* ", "+ "].iter().find(|m| resto.starts_with(**m))?.len();
    let caixa = &resto[marca..];
    (caixa.starts_with("[ ] ") || caixa.starts_with("[x] ") || caixa.starts_with("[X] ")).then(|| (recuo + marca, !caixa.starts_with("[ ]")))
}

struct Tons {
    sinal: cosmic_text::Color,
    link: cosmic_text::Color,
    /// Os trechos com aviso nesta linha: link que toca um deles fica como texto, com o endereço à vista.
    avisos: Vec<std::ops::Range<usize>>,
}

/// O endereço do `[texto](endereço)` que tem o byte `i`, lido como o desenho lê: o link que toca
/// um aviso não conta (fica como texto).
fn link_em(texto: &str, i: usize, avisos: &[std::ops::Range<usize>]) -> Option<String> {
    let mut desde = 0;
    while let Some(a) = texto[desde..].find('[').map(|a| desde + a) {
        let f = texto[a..].find("](").map(|f| a + f)?;
        let z = texto[f..].find(')').map(|z| f + z)?;
        if avisos.iter().any(|r| r.start <= z && a < r.end) {
            desde = a + 1;
            continue;
        }
        if (a..=z).contains(&i) {
            return Some(texto[f + 2..z].to_owned());
        }
        desde = z + 1;
    }
    None
}

fn cor_ct(c: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8, (c.a * 255.0) as u8)
}

/// Os estilos de uma linha de Markdown. `em_codigo` atravessa as linhas (blocos ```).
fn markdown(texto: &str, base: Attrs<'static>, m: Metrics, t: &Tons, em_codigo: &mut bool, esconder: bool) -> AttrsList {
    let mut l = AttrsList::new(base);
    // Escondido: transparente e quase sem largura (continua no texto; o cursor passa por ele).
    let sumir = |a: Attrs<'static>| if esconder { a.color(cosmic_text::Color::rgba(0, 0, 0, 0)).metrics(Metrics::new(0.01, m.line_height)) } else { a.color(t.sinal) };
    let n = texto.len();
    let recuo = texto.len() - texto.trim_start().len();
    let resto = &texto[recuo..];
    let mono = base.family(Family::Monospace);
    if resto.starts_with("```") {
        *em_codigo = !*em_codigo;
        l.add_span(0..n, mono.color(t.sinal));
        return l;
    }
    if *em_codigo {
        l.add_span(0..n, mono);
        return l;
    }
    let mut desde = recuo;
    // O estilo da linha, que o negrito/itálico de dentro mantém (o tamanho do título, por exemplo).
    let mut linha = base;
    let titulo = resto.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&titulo) && resto[titulo..].starts_with(' ') || (1..=6).contains(&titulo) && resto.len() == titulo {
        let k = [1.6, 1.4, 1.25, 1.12, 1.05, 1.0][titulo - 1];
        let grande = base.weight(Weight::BOLD).metrics(Metrics::new(m.font_size * k, m.line_height * k));
        l.add_span(0..n, grande);
        desde = (recuo + titulo + 1).min(n);
        l.add_span(0..desde, sumir(grande));
        linha = grande;
    } else if resto.starts_with("> ") || resto == ">" {
        linha = base.style(Estilo::Italic).color(t.sinal);
        l.add_span(0..n, linha);
        desde = (recuo + 2).min(n);
        // O ">" some e o desenho põe uma barra no lugar.
        l.add_span(0..desde, if esconder { invisivel(base).metadata(CITACAO) } else { base.color(t.sinal) });
    } else if resto.len() >= 3 && resto.trim_end().chars().all(|c| c == '-' || c == ' ') && resto.matches('-').count() >= 3 || resto.trim_end() == "***" {
        l.add_span(0..n, base.color(t.sinal));
        return l;
    } else if resto.starts_with('|') {
        // Tabela: as barras apagadas; a linha do cabeçalho (|---|) toda.
        if resto.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            l.add_span(0..n, base.color(t.sinal));
            return l;
        }
        for (i, _) in texto.match_indices('|') {
            l.add_span(i..i + 1, base.color(t.sinal));
        }
    } else {
        // Lista: "- ", "* ", "+ ", "1. ", "1) ", e a caixinha "[ ] " / "[x] ".
        let numero = resto.bytes().take_while(u8::is_ascii_digit).count();
        let marca = if resto.starts_with("- ") || resto.starts_with("* ") || resto.starts_with("+ ") {
            2
        } else if numero > 0 && (resto[numero..].starts_with(". ") || resto[numero..].starts_with(") ")) {
            numero + 2
        } else {
            0
        };
        if marca > 0 {
            desde = recuo + marca;
            let caixa = &texto[desde..];
            let tarefa = marca == 2 && (caixa.starts_with("[ ] ") || caixa.starts_with("[x] ") || caixa.starts_with("[X] "));
            // Escondidos: o "- " de tarefa some (a caixinha basta); o de lista vira um "•" desenhado.
            let estilo_marca = match (esconder, tarefa, numero > 0) {
                (true, true, _) => sumir(base),
                (true, false, false) => invisivel(base).metadata(MARCADOR),
                _ => base.weight(Weight::BOLD).color(t.link),
            };
            l.add_span(recuo..desde, estilo_marca);
            if tarefa {
                let marcada = !caixa.starts_with("[ ]");
                // Escondida: some o texto (com a largura) e o desenho põe uma caixinha no lugar.
                let estilo = if esconder { invisivel(base.weight(Weight::BOLD)).metadata(if marcada { CAIXA_MARCADA } else { CAIXA }) } else { base.weight(Weight::BOLD).color(t.link) };
                l.add_span(desde..desde + 3, estilo);
                if marcada {
                    // O negrito de dentro continua riscado e apagado.
                    linha = base.color(t.sinal).metadata(TACHADO);
                    l.add_span(desde + 4..n, linha);
                }
                desde += 4;
            }
        }
    }
    inline(texto, desde, &mut l, linha, t, &sumir);
    l
}

/// Transparente mas com a largura de sempre: o desenho põe algo no lugar.
fn invisivel(a: Attrs<'static>) -> Attrs<'static> {
    a.color(cosmic_text::Color::rgba(0, 0, 0, 0))
}

/// Negrito, itálico, tachado, código e links dentro da linha, a partir do byte `desde`.
fn inline(texto: &str, desde: usize, l: &mut AttrsList, base: Attrs<'static>, t: &Tons, sumir: &dyn Fn(Attrs<'static>) -> Attrs<'static>) {
    let b = texto.as_bytes();
    let sinal = sumir(base);
    let mut i = desde;
    while i < b.len() {
        let antes_letra = i > 0 && (b[i - 1] as char).is_alphanumeric();
        // (abre, fecha, estilo do miolo)
        let par: Option<(&str, Attrs<'static>)> = match b[i] {
            b'`' => Some(("`", base.family(Family::Monospace))),
            b'*' if b.get(i + 1) == Some(&b'*') => Some(("**", base.weight(Weight::BOLD))),
            b'_' if b.get(i + 1) == Some(&b'_') && !antes_letra => Some(("__", base.weight(Weight::BOLD))),
            b'~' if b.get(i + 1) == Some(&b'~') => Some(("~~", base.metadata(TACHADO))),
            b'*' => Some(("*", base.style(Estilo::Italic))),
            b'_' if !antes_letra => Some(("_", base.style(Estilo::Italic))),
            _ => None,
        };
        if let Some((marca, estilo)) = par {
            let k = marca.len();
            // "***oi***": o "**" fecha nos dois últimos da sequência, e o "*" de dentro acha o seu.
            let fim_da_sequencia = |j: usize| j + texto[j..].bytes().take_while(|&x| x == b[i]).count() - k;
            if let Some(j) = texto[i + k..].find(marca).map(|j| fim_da_sequencia(i + k + j)).filter(|&j| j > i + k) {
                l.add_span(i + k..j, estilo);
                l.add_span(i..i + k, sinal);
                l.add_span(j..j + k, sinal);
                // Um dentro do outro (**~~x~~**); código fica como está.
                if marca != "`" {
                    inline(&texto[..j], i + k, l, estilo, t, sumir);
                }
                i = j + k;
                continue;
            }
        }
        // [texto](endereço)
        if b[i] == b'[' {
            if let Some(f) = texto[i..].find("](").map(|f| i + f) {
                if let Some(z) = texto[f..].find(')').map(|z| f + z).filter(|&z| !t.avisos.iter().any(|a| a.start <= z && i < a.end)) {
                    l.add_span(i + 1..f, base.color(t.link).metadata(LINK));
                    l.add_span(i..i + 1, sinal);
                    l.add_span(f..z + 1, sinal);
                    i = z + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// `termo` em `texto` a partir do byte `desde`: (início, fim) em bytes.
fn achar(texto: &str, termo: &str, desde: usize, maiusculas: bool) -> Option<(usize, usize)> {
    if maiusculas {
        return texto.get(desde..)?.find(termo).map(|a| (desde + a, desde + a + termo.len()));
    }
    // Sem diferenciar maiúsculas, caractere a caractere (o minúsculo pode ter outro tamanho em bytes).
    let igual = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    texto.char_indices().filter(|&(i, _)| i >= desde).find_map(|(i, _)| {
        let mut fim = i;
        let mut resto = texto[i..].chars();
        for t in termo.chars() {
            let c = resto.next()?;
            if !igual(c, t) {
                return None;
            }
            fim += c.len_utf8();
        }
        Some((i, fim))
    })
}

fn trocar_tudo(texto: &str, termo: &str, novo: &str, maiusculas: bool) -> (String, usize) {
    let (mut saida, mut desde, mut n) = (String::new(), 0, 0);
    while let Some((a, z)) = achar(texto, termo, desde, maiusculas) {
        saida.push_str(&texto[desde..a]);
        saida.push_str(novo);
        desde = z;
        n += 1;
    }
    saida.push_str(&texto[desde..]);
    (saida, n)
}

// ---------- O widget ----------

pub struct TextEditor<'a, M> {
    content: &'a Content,
    size: f32,
    font: Font,
    wrap: bool,
    line_numbers: bool,
    padding: Padding,
    on_change: Option<M>,
    style: Option<Box<dyn Fn(&Theme, Status) -> Style + 'a>>,
    autofocus: Option<u64>,
    invisibles: bool,
    markdown: bool,
    line_height: Option<f32>,
    placeholder: &'a str,
    id: Option<u64>,
    on_key: Option<Box<dyn Fn(&Key, crate::Modifiers) -> Option<M> + 'a>>,
    idle_focus: bool,
    current_line: Option<Color>,
    stripes: Option<Color>,
    warnings: Vec<Warning>,
    on_link: Option<Box<dyn Fn(String) -> M + 'a>>,
    #[cfg(feature = "highlighter")]
    highlight: Option<(&'a str, crate::Highlight)>,
}

/// Um trecho sublinhado em onda laranja ([`TextEditor::warnings`]): a linha (contando de 0), os
/// bytes dentro dela e a dica que aparece com o mouse em cima.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Warning {
    pub line: usize,
    pub range: std::ops::Range<usize>,
    pub tip: String,
}

/// Editor de várias linhas sobre um [`Content`]. Edita o conteúdo direto e avisa com
/// `on_change` (para o app marcar "não salvo").
pub fn text_editor<'a, M>(content: &'a Content) -> TextEditor<'a, M> {
    TextEditor { content, size: 15.0, font: Font::DEFAULT, wrap: true, line_numbers: false, padding: Padding::from(12.0), on_change: None, style: None, autofocus: None, invisibles: false, markdown: false, line_height: None, placeholder: "", id: None, on_key: None, idle_focus: true, current_line: None, stripes: None, warnings: Vec::new(), on_link: None, #[cfg(feature = "highlighter")] highlight: None }
}

impl<'a, M> TextEditor<'a, M> {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn font(mut self, f: Font) -> Self {
        self.font = f;
        self
    }
    /// Quebra automática de linha (ligada por padrão).
    pub fn wrap(mut self, w: bool) -> Self {
        self.wrap = w;
        self
    }
    /// Mostra espaço `·`, tab `→`, fim de linha `↵`, e marca os caracteres ocultos ([`is_invisible`]).
    pub fn invisibles(mut self, v: bool) -> Self {
        self.invisibles = v;
        self
    }
    /// Markdown leve no próprio texto: títulos, negrito, itálico, tachado, código, links,
    /// listas, citações e tabelas. Os sinais ficam à vista, apagados.
    pub fn markdown(mut self, m: bool) -> Self {
        self.markdown = m;
        self
    }
    /// Altura de cada linha, em pixels lógicos. Padrão: 1,45 × o tamanho.
    pub fn line_height(mut self, h: f32) -> Self {
        self.line_height = Some(h);
        self
    }
    /// O nome para [`crate::Task::focus`].
    pub fn id(mut self, id: impl std::hash::Hash) -> Self {
        self.id = Some(crate::id_hash(&id));
        self
    }
    /// O texto apagado que aparece com o editor vazio.
    pub fn placeholder(mut self, p: &'a str) -> Self {
        self.placeholder = p;
        self
    }
    /// Vê cada tecla antes do editor (com foco). `Some(msg)` manda a mensagem e o editor não faz
    /// nada com a tecla: Tab que recua, Enter que envia, atalhos do app.
    pub fn on_key(mut self, f: impl Fn(&Key, crate::Modifiers) -> Option<M> + 'a) -> Self {
        self.on_key = Some(Box::new(f));
        self
    }
    /// Cores de código pela extensão do arquivo (`"rs"`, `"py"`, `"toml"`…). Desconhecida: sem cor.
    #[cfg(feature = "highlighter")]
    pub fn highlight(mut self, ext: &'a str, tema: crate::Highlight) -> Self {
        self.highlight = Some((ext, tema));
        self
    }
    fn altura(&self) -> f32 {
        self.line_height.unwrap_or(self.size * 1.45)
    }
    /// Sem nenhum campo focado, o editor pega a digitação (o padrão). `false` para quando há
    /// mais de um editor na tela: cada um só escreve depois de clicado ou de um [`crate::Task::focus`].
    pub fn idle_focus(mut self, f: bool) -> Self {
        self.idle_focus = f;
        self
    }
    /// Faixa desta cor na linha do cursor (com foco).
    pub fn current_line(mut self, c: Color) -> Self {
        self.current_line = Some(c);
        self
    }
    /// Faixa desta cor nas linhas pares (2, 4, 6…), para seguir a linha com os olhos.
    pub fn stripes(mut self, c: Color) -> Self {
        self.stripes = Some(c);
        self
    }
    /// Sublinha estes trechos em onda (aviso, erro de ortografia); a dica de cada um aparece com o
    /// mouse em cima. Trecho fora do texto é ignorado.
    pub fn warnings(mut self, w: Vec<Warning>) -> Self {
        self.warnings = w;
        self
    }
    /// Com [`markdown`](Self::markdown): Ctrl+clique num `[texto](endereço)` manda o endereço (o
    /// clique simples só põe o cursor). Com o mouse em cima, a dica mostra o endereço.
    pub fn on_link(mut self, f: impl Fn(String) -> M + 'a) -> Self {
        self.on_link = Some(Box::new(f));
        self
    }
    pub fn line_numbers(mut self, n: bool) -> Self {
        self.line_numbers = n;
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    /// Como [`crate::TextInput::autofocus`]: a próxima tecla volta para o editor.
    pub fn autofocus(mut self, pedido: u64) -> Self {
        self.autofocus = Some(pedido);
        self
    }
    pub fn on_change(mut self, m: M) -> Self {
        self.on_change = Some(m);
        self
    }
    /// Fundo, borda e cor do texto; `Status::Focused` com foco.
    pub fn style(mut self, f: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Some(Box::new(f));
        self
    }

    fn focado(&self, foco: Option<u64>, key: u64) -> bool {
        foco == Some(key) || self.idle_focus && foco.is_none()
    }

    /// Onde fica a calha dos números e onde começa o texto.
    /// O polegar da barra de rolagem, se o texto não cabe; `topo` é a primeira linha à vista (com fração).
    // shortcut: conta linhas do texto, não as visuais; com `wrap` a barra fica aproximada, contar layout_runs se incomodar.
    fn polegar(&self, n: &Node, area: Rect, total: usize, topo: f32) -> Option<Rect> {
        let visiveis = area.height / self.altura();
        let total = total as f32;
        if total <= visiveis {
            return None;
        }
        let altura = (area.height * visiveis / total).max(24.0).min(area.height);
        let fracao = (topo / (total - visiveis)).clamp(0.0, 1.0);
        let t = trilho(n);
        Some(Rect { x: t.x, y: area.y + (area.height - altura) * fracao, width: t.width, height: altura })
    }
    fn areas(&self, n: &Node, r: &mut crate::Renderer) -> (Rect, Rect) {
        let dentro = n.rect.shrink(self.padding);
        let calha = if self.line_numbers {
            let digitos = self.content.line_count().to_string().len().max(2);
            r.measure(&"0".repeat(digitos), self.size * 0.85, Font::DEFAULT, None).width + 26.0
        } else {
            0.0
        };
        (Rect { width: calha, ..dentro }, Rect { x: dentro.x + calha, width: (dentro.width - calha).max(1.0), ..dentro })
    }
}

/// A faixa da barra de rolagem, na borda direita do editor.
fn trilho(n: &Node) -> Rect {
    Rect { x: n.rect.x + n.rect.width - 10.0, width: 10.0, ..n.rect }
}

#[derive(Default)]
struct Mouse {
    arrastando: bool,
    /// Arrastando a barra de rolagem: a distância do clique ao topo do polegar.
    barra: Option<f32>,
    /// O último clique marcou uma caixinha: o próximo não vira clique duplo.
    caixa: bool,
    /// Ctrl apertado: o link sob o mouse vira mãozinha e abre no clique.
    ctrl: bool,
    ultimo: Option<(std::time::Instant, u8)>,
}

impl<M: Clone> Widget<M> for TextEditor<'_, M> {
    fn size(&self) -> (Length, Length) {
        (Length::FILL, Length::FILL)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        if let Some(id) = self.id {
            cx.states.ids.borrow_mut().insert(id, cx.key());
        }
        Node::leaf(cx.key(), l.fill(Size::new(400.0, 300.0)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        // Sem outro campo focado, o editor recebe a digitação (abrir e já escrever).
        let focado = self.focado(cx.states.focus, n.key) || crate::widget::foco_pendente(cx.states, n.key, self.autofocus);
        let hover = cx.hovered(&n.rect);
        if hover {
            cx.cursor_icon = CursorIcon::Text;
        }
        let status = if focado { Status::Focused } else if hover { Status::Hovered } else { Status::Idle };
        let st = match &self.style {
            Some(f) => f(cx.theme, status),
            None => Style::default().text_color(cx.theme.text),
        };
        let st = cx.styled(n.key, st);
        st.paint(n.rect, cx);
        cx.access(n.key, n.rect, || {
            let mut a = crate::widget::no(crate::accesskit::Role::MultilineTextInput, &[crate::accesskit::Action::Click, crate::accesskit::Action::Focus]);
            a.set_value(self.content.text());
            a
        });
        let cor = st.text_color.unwrap_or(cx.text_color);
        let (calha, area) = self.areas(n, cx.renderer);
        let s = cx.renderer.scale();
        let mut i = self.content.0.borrow_mut();
        let versao = i.versao;
        let mut cercas = i.cercas;
        #[cfg(feature = "highlighter")]
        let mut realce = std::mem::take(&mut i.realce);
        let ed = i.editor(cx.renderer.fonts(), self.size, self.altura(), s, self.font, self.wrap);
        let fonts = cx.renderer.fonts();
        let vazio = ed.with_buffer(|b| b.lines.len() == 1 && b.lines[0].text().is_empty());
        ed.with_buffer_mut(|b| {
            b.set_size(fonts, Some(area.width * s), Some(area.height * s));
            // Só as linhas à vista (e algumas a mais) ganham estilo; as outras quando aparecerem.
            let base = crate::render::attrs(self.font);
            let primeira = b.scroll().line;
            let ultima = (primeira + (area.height / self.altura()) as usize + 2).min(b.lines.len());
            let mut tons = Tons { sinal: cor_ct(cx.theme.muted), link: cor_ct(cx.theme.primary), avisos: Vec::new() };
            // Dentro de um bloco ``` ou não: conta as cercas desde o começo (só comparação de prefixo).
            let mut em_codigo = self.markdown
                && match cercas {
                    Some((v, p, dentro)) if (v, p) == (versao, primeira) => dentro,
                    _ => b.lines[..primeira].iter().filter(|l| l.text().trim_start().starts_with("```")).count() % 2 == 1,
                };
            if self.markdown {
                cercas = Some((versao, primeira, em_codigo));
            }
            let metricas = b.metrics();
            #[cfg(feature = "highlighter")]
            if let Some((ext, tema)) = self.highlight.filter(|_| !self.markdown) {
                let cores = realce.cores(ext, tema, versao, |k| b.lines[k].text(), primeira..ultima, base);
                for (linha, a) in b.lines[primeira..ultima].iter_mut().zip(cores) {
                    linha.set_attrs_list(a);
                }
                return;
            }
            for (k, linha) in b.lines[primeira..ultima].iter_mut().enumerate() {
                tons.avisos = self.warnings.iter().filter(|w| w.line == primeira + k).map(|w| w.range.clone()).collect();
                // Os sinais ficam escondidos sempre (como no Bloco de Notas do Windows): formatar é pelos atalhos.
                let estilos = if self.markdown { markdown(linha.text(), base, metricas, &tons, &mut em_codigo, true) } else { AttrsList::new(base) };
                linha.set_attrs_list(estilos);
            }
        });
        // Só as linhas à vista são montadas; as que saíram da tela são soltas.
        ed.shape_as_needed(fonts, true);

        // Tudo em lógico: seleção, números das linhas e o cursor, lidos do layout.
        let selecao = ed.selection_bounds();
        let (mut realces, mut numeros) = (Vec::new(), Vec::new());
        // (x, topo da linha, marca, centrada no espaço que ocupa) e as caixas dos ocultos.
        let (mut marcas, mut caixas) = (Vec::<(f32, f32, &str, bool)>::new(), Vec::new());
        let mut tracos: Vec<(Rect, usize)> = Vec::new();
        let mut tarefas: Vec<(Rect, bool, usize)> = Vec::new();
        let mut enfeites: Vec<(Rect, usize)> = Vec::new();
        let mut faixas: Vec<(usize, f32, f32)> = Vec::new();
        let mut ondas: Vec<(Rect, usize)> = Vec::new();
        let (mouse, mut link_sob_mouse) = (cx.cursor, None::<(Rect, String)>);
        let (rolagem, altura_linha) = ed.with_buffer(|b| {
            let h = b.scroll().horizontal;
            let mut anterior = usize::MAX;
            // A última parte de cada linha (com quebra automática, a linha tem várias) leva o ↵.
            let mut fim: Option<(usize, f32, f32)> = None;
            let marcar_fim = |f: Option<(usize, f32, f32)>, marcas: &mut Vec<(f32, f32, &str, bool)>| {
                if let Some((_, x, y)) = f.filter(|&(l, ..)| !b.lines[l].ending().as_str().is_empty()) {
                    marcas.push((area.x + (x - h) / s + 2.0, y, "↵", false));
                }
            };
            for run in b.layout_runs() {
                let y = area.y + run.line_top / s;
                if self.markdown {
                    for g in run.glyphs.iter().filter(|g| g.metadata == CAIXA || g.metadata == CAIXA_MARCADA) {
                        let (gx, gw) = (area.x + (g.x - h) / s, g.w / s);
                        // Os três glifos de "[ ]" viram um retângulo só.
                        match tarefas.last_mut() {
                            Some((r, _, linha)) if *linha == run.line_i && (r.x + r.width - gx).abs() < 1.0 => r.width += gw,
                            _ => tarefas.push((Rect { x: gx, y, width: gw, height: run.line_height / s }, g.metadata == CAIXA_MARCADA, run.line_i)),
                        }
                    }
                    for g in run.glyphs.iter().filter(|g| g.metadata == MARCADOR || g.metadata == CITACAO) {
                        let (gx, gw) = (area.x + (g.x - h) / s, g.w / s);
                        // Um sinal por marca: o "-" vira "•"; no "> " só o primeiro glifo leva a barra.
                        if g.metadata == MARCADOR && run.text[g.start..g.end].trim().is_empty() || g.metadata == CITACAO && run.text[g.start..g.end] != *">" {
                            continue;
                        }
                        enfeites.push((Rect { x: gx, y, width: gw, height: run.line_height / s }, g.metadata));
                    }
                    for g in run.glyphs.iter().filter(|g| g.metadata == TACHADO || g.metadata == LINK) {
                        let (gx, gw) = (area.x + (g.x - h) / s, g.w / s);
                        let base = area.y + run.line_y / s;
                        let y = if g.metadata == TACHADO { base - g.font_size / s * 0.3 } else { base + 2.0 };
                        tracos.push((Rect { x: gx, y, width: gw, height: 1.0 }, g.metadata));
                        let caixa = Rect { x: gx, y: area.y + run.line_top / s, width: gw, height: run.line_height / s };
                        if g.metadata == LINK && link_sob_mouse.is_none() && mouse.is_some_and(|p| caixa.contains(p)) {
                            let avisos: Vec<_> = self.warnings.iter().filter(|w| w.line == run.line_i).map(|w| w.range.clone()).collect();
                            link_sob_mouse = link_em(run.text, g.start, &avisos).map(|u| (caixa, u));
                        }
                    }
                }
                if self.invisibles {
                    if fim.is_some_and(|(l, ..)| l != run.line_i) {
                        marcar_fim(fim, &mut marcas);
                    }
                    fim = Some((run.line_i, run.line_w, y));
                    for g in run.glyphs.iter().filter(|g| g.metadata != CAIXA && g.metadata != CAIXA_MARCADA && g.color_opt.is_none_or(|c| c.a() > 0)) {
                        let trecho = &run.text[g.start..g.end];
                        let (gx, gw) = (area.x + (g.x - h) / s, g.w / s);
                        match trecho {
                            " " => marcas.push((gx + gw / 2.0, y, "·", true)),
                            "\t" => marcas.push((gx + gw / 2.0, y, "→", true)),
                            // Largura zero: uma barrinha entre as letras, sem cobrir a do lado.
                            _ if trecho.chars().any(is_invisible) => caixas.push(if gw < 1.0 { Rect { x: gx - 1.5, y: y + 2.0, width: 3.0, height: run.line_height / s - 4.0 } } else { Rect { x: gx, y: y + 2.0, width: gw, height: run.line_height / s - 4.0 } }),
                            _ => {}
                        }
                    }
                }
                for (k, aviso) in self.warnings.iter().enumerate().filter(|(_, a)| a.line == run.line_i) {
                    if let Some((x, w)) = run.highlight(Cursor::new(aviso.line, aviso.range.start), Cursor::new(aviso.line, aviso.range.end)).filter(|&(_, w)| w > 0.0) {
                        ondas.push((Rect { x: area.x + (x - h) / s, y, width: w / s, height: run.line_height / s }, k));
                    }
                }
                if let Some((a, z)) = selecao {
                    if let Some((x, w)) = run.highlight(a, z) {
                        // Linha vazia selecionada: um tanto de largura para aparecer.
                        let w = if w <= 0.0 && a.line <= run.line_i && run.line_i < z.line { self.size * 0.4 * s } else { w };
                        realces.push(Rect { x: area.x + (x - h) / s, y, width: w / s, height: run.line_height / s });
                    }
                }
                faixas.push((run.line_i, y, run.line_height / s));
                if run.line_i != anterior {
                    numeros.push((run.line_i + 1, y));
                    anterior = run.line_i;
                }
            }
            marcar_fim(fim, &mut marcas);
            (b.scroll(), b.metrics().line_height / s)
        });
        let (total, topo) = ed.with_buffer(|b| (b.lines.len(), b.scroll().line as f32 + b.scroll().vertical / (self.altura() * s)));
        let caret = focado.then(|| ed.cursor_position()).flatten().map(|(x, y)| Point::new(area.x + (x as f32 - rolagem.horizontal) / s, area.y + y as f32 / s));
        let mut chave = std::collections::hash_map::DefaultHasher::new();
        (versao, rolagem.line, rolagem.vertical.to_bits(), rolagem.horizontal.to_bits(), (area.width * s) as u32, (self.size * s * 64.0) as u32, self.wrap, self.invisibles, self.markdown).hash(&mut chave);
        self.warnings.hash(&mut chave);
        #[cfg(feature = "highlighter")]
        self.highlight.hash(&mut chave);

        cx.renderer.push_clip(area, [0.0; 4]);
        let linha_cursor = ed.cursor().line;
        for &(linha, y, h) in &faixas {
            let tom = if focado && linha == linha_cursor { self.current_line } else { self.stripes.filter(|_| linha % 2 == 1) };
            if let Some(tom) = tom {
                cx.renderer.fill_quad(Rect { x: area.x, y, width: area.width, height: h }, Some(&tom.into()), Border::default(), None);
            }
        }
        let cor_selecao = cx.theme.primary.alpha(if focado { 0.30 } else { 0.18 });
        for r in &realces {
            cx.renderer.fill_quad(*r, Some(&cor_selecao.into()), Border::default(), None);
        }
        let at = Point::new(area.x - rolagem.horizontal / s, area.y);
        ed.with_buffer(|b| cx.renderer.draw_buffer(b, at, cor, area, chave.finish()));
        if vazio && !self.placeholder.is_empty() {
            let desce = (altura_linha - cx.renderer.measure(self.placeholder, self.size, self.font, None).height) / 2.0;
            cx.renderer.draw_text(self.placeholder, self.size, self.font, cx.theme.muted, Point::new(area.x, area.y + desce), Some(area.width));
        }
        for &(r, tipo) in &enfeites {
            if tipo == CITACAO {
                cx.renderer.fill_quad(Rect { x: r.x + 1.0, y: r.y + 2.0, width: 3.0, height: r.height - 4.0 }, Some(&cx.theme.primary.alpha(0.6).into()), Border::rounded(1.5), None);
            } else {
                let m = cx.renderer.measure("•", self.size, self.font, None);
                cx.renderer.draw_text("•", self.size, self.font, cx.theme.primary, Point::new(r.x + (r.width - m.width) / 2.0, r.y + (r.height - m.height) / 2.0), None);
            }
        }
        for &(r, marcada, _) in &tarefas {
            let lado = (r.height * 0.62).min(r.width).round();
            let q = Rect { x: r.x + (r.width - lado) / 2.0, y: r.y + (r.height - lado) / 2.0, width: lado, height: lado };
            let fundo = if marcada { cx.theme.primary } else { Color::TRANSPARENT };
            cx.renderer.fill_quad(q, Some(&fundo.into()), Border::rounded(lado * 0.22).color(cx.theme.primary, 1.5), None);
            if marcada {
                // O "✓" da fonte sai quebrado em tamanho pequeno: um traço desenhado fica nítido.
                static CERTO: std::sync::OnceLock<crate::Svg> = std::sync::OnceLock::new();
                let certo = CERTO.get_or_init(|| crate::Svg::from_bytes(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>"##).expect("svg fixo"));
                cx.renderer.draw_svg(certo, q, Some(cx.theme.on_primary));
            }
        }
        for (r, tipo) in &tracos {
            let tom = if *tipo == LINK { cx.theme.primary } else { cor };
            cx.renderer.fill_quad(*r, Some(&tom.into()), Border::default(), None);
        }
        let laranja = Color::rgb8(0xe8, 0x71, 0x0a);
        for r in &caixas {
            cx.renderer.fill_quad(*r, Some(&laranja.alpha(0.25).into()), Border::rounded(2.0).color(laranja, 1.0), None);
        }
        // A onda: degraus de 2 px que sobem e descem logo abaixo das letras.
        let mut dica = None;
        for &(r, k) in &ondas {
            let base = r.y + r.height - 3.5;
            let mut x = r.x;
            let mut sobe = false;
            while x < r.x + r.width {
                let w = (r.x + r.width - x).min(2.0);
                cx.renderer.fill_quad(Rect { x, y: if sobe { base - 1.0 } else { base }, width: w, height: 1.2 }, Some(&laranja.into()), Border::default(), None);
                x += 2.0;
                sobe = !sobe;
            }
            if cx.hovered(&r) {
                dica = Some((r, self.warnings[k].tip.clone()));
            }
        }
        if link_sob_mouse.is_some() && cx.states.get::<Mouse>(n.key).is_some_and(|m| m.ctrl) && self.on_link.is_some() {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        // Link escondido pelo markdown: a dica mostra para onde ele leva de verdade.
        let dica = dica.or_else(|| link_sob_mouse.map(|(r, u)| (r, if self.on_link.is_some() { format!("{u}\nCtrl+clique para abrir") } else { u })));
        let desce = (altura_linha - cx.renderer.measure("·", self.size, self.font, None).height) / 2.0;
        for &(x, y, m, centrada) in &marcas {
            let w = if centrada { cx.renderer.measure(m, self.size, self.font, None).width } else { 0.0 };
            cx.renderer.draw_text(m, self.size, self.font, cx.theme.muted, Point::new(x - w / 2.0, y + desce), None);
        }
        if let Some(p) = caret {
            cx.renderer.fill_quad(Rect { x: p.x, y: p.y, width: 1.5, height: altura_linha }, Some(&cor.into()), Border::default(), None);
        }
        cx.renderer.pop_clip();
        // A dica do trecho sob o mouse, por cima de tudo (como a do `tooltip`).
        let p = cx.transition(n.key, if dica.is_some() { 1.0f32 } else { 0.0 }, if dica.is_some() { crate::Transition::new(120).delay(250) } else { crate::Transition::new(90) });
        if let Some((r, tip)) = dica.filter(|_| p > 0.0) {
            let (theme, janela) = (*cx.theme, cx.renderer.window_size());
            cx.overlay(move |rd| {
                rd.push_opacity(p);
                let t = rd.measure(&tip, 12.0, Font::DEFAULT, Some(420.0));
                let (w, h) = (t.width + 20.0, t.height + 12.0);
                let x = r.x.clamp(4.0, (janela.width - w - 4.0).max(4.0));
                // Embaixo da palavra; sem espaço, em cima.
                let y = if r.y + r.height + 4.0 + h > janela.height { r.y - h - 4.0 } else { r.y + r.height + 4.0 };
                let sombra = crate::Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 2.0), blur: 6.0 };
                rd.fill_quad(Rect { x, y, width: w, height: h }, Some(&theme.surface.into()), Border::rounded(6.0).color(theme.border, 1.0), Some(sombra));
                rd.draw_text(&tip, 12.0, Font::DEFAULT, theme.text, Point::new(x + 10.0, y + 6.0), Some(420.0));
                rd.pop_opacity();
            });
        }
        i.cercas = cercas;
        #[cfg(feature = "highlighter")]
        {
            i.realce = realce;
        }
        drop(i);

        if self.line_numbers {
            cx.renderer.push_clip(calha, [0.0; 4]);
            let atual = self.content.cursor().0;
            let pequeno = self.size * 0.85;
            let desce = (altura_linha - cx.renderer.measure("0", pequeno, Font::DEFAULT, None).height) / 2.0;
            for (k, y) in numeros {
                let texto = k.to_string();
                let w = cx.renderer.measure(&texto, pequeno, Font::DEFAULT, None).width;
                let tom = if focado && k == atual { cor } else { cx.theme.muted };
                cx.renderer.draw_text(&texto, pequeno, Font::DEFAULT, tom, Point::new(calha.x + calha.width - 20.0 - w, y + desce), None);
            }
            cx.renderer.pop_clip();
            // O fio que separa os números do texto, de cima a baixo do editor.
            let fio = Rect { x: calha.x + calha.width - 8.0, y: n.rect.y, width: 1.0, height: n.rect.height };
            cx.renderer.fill_quad(fio, Some(&cx.theme.border.into()), Border::default(), None);
        }
        if let Some(p) = self.polegar(n, area, total, topo) {
            let em_cima = cx.cursor.is_some_and(|c| trilho(n).contains(c)) || cx.states.get::<Mouse>(n.key).is_some_and(|m| m.barra.is_some());
            let cor = cx.theme.muted.alpha(if em_cima { 0.55 } else { 0.3 });
            cx.renderer.fill_quad(Rect { x: p.x + 2.0, width: p.width - 4.0, ..p }, Some(&cor.into()), Border::rounded(3.0), None);
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        crate::widget::atender_foco(ev, cx.states, n.key, self.autofocus);
        // Sem outro campo focado, o editor recebe a digitação (abrir e já escrever).
        let focado = self.focado(cx.states.focus, n.key);
        let (_, area) = self.areas(n, cx.renderer);
        let s = cx.renderer.scale();
        let ponto = |p: Point| (((p.x - area.x) * s) as i32, ((p.y - area.y) * s) as i32);
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        let mut i = self.content.0.borrow_mut();
        // O primeiro evento pode vir antes do primeiro desenho (testes): monta já.
        i.editor(cx.renderer.fonts(), self.size, self.altura(), s, self.font, self.wrap);
        let antes = i.versao;
        let (total, topo) = i.ed.as_ref().expect("montado acima").with_buffer(|b| (b.lines.len(), b.scroll().line as f32 + b.scroll().vertical / (self.altura() * s)));
        let polegar = self.polegar(n, area, total, topo);
        let barra = cx.states.get::<Mouse>(n.key).and_then(|m| m.barra);
        // Rola para que o topo do polegar fique em `y` (posição na janela).
        let rolar_para = |i: &mut Interno, p: Rect, y: f32| {
            let livre = (area.height - p.height).max(1.0);
            let linha = ((y - area.y) / livre).clamp(0.0, 1.0) * (total as f32 - area.height / self.altura()).max(0.0);
            i.ed.as_mut().expect("montado acima").with_buffer_mut(|b| {
                let mut sc = b.scroll();
                sc.line = linha as usize;
                sc.vertical = linha.fract() * self.altura() * s;
                b.set_scroll(sc);
            });
        };
        // Ctrl+clique num link: o endereço vai para o app e o cursor fica onde estava.
        let ctrl = cx.states.get::<Mouse>(n.key).is_some_and(|m| m.ctrl);
        let mut link = None;
        if self.markdown && ctrl && self.on_link.is_some() && dentro && matches!(ev, Event::MousePressed(MouseButton::Left)) {
            let (x, y) = ponto(cx.cursor.unwrap_or(Point::ORIGIN));
            link = i.ed.as_ref().expect("montado acima").with_buffer(|b| {
                let c = b.hit(x as f32, y as f32)?;
                let avisos: Vec<_> = self.warnings.iter().filter(|w| w.line == c.line).map(|w| w.range.clone()).collect();
                link_em(b.lines[c.line].text(), c.index, &avisos)
            });
        }
        let resultado = match ev {
            Event::ModifiersChanged(m) => {
                cx.states.get_mut::<Mouse>(n.key).ctrl = m.control_key();
                cx.redraw = true;
                Captured::No
            }
            Event::MousePressed(MouseButton::Left) if link.is_some() => {
                cx.publish((self.on_link.as_ref().expect("conferido"))(link.clone().expect("conferido")));
                Captured::Yes
            }
            // A barra vem antes do texto: clicar nela não mexe no cursor.
            Event::MousePressed(MouseButton::Left) if polegar.is_some() && cx.cursor.is_some_and(|c| trilho(n).contains(c)) => {
                let (p, y) = (polegar.expect("conferido"), cx.cursor.expect("conferido").y);
                // No polegar, segura onde pegou; fora dele, o polegar pula para centrar no clique.
                let pega = if (p.y..p.y + p.height).contains(&y) { y - p.y } else { p.height / 2.0 };
                rolar_para(&mut i, p, y - pega);
                cx.states.get_mut::<Mouse>(n.key).barra = Some(pega);
                Captured::Yes
            }
            Event::CursorMoved(c) if barra.is_some() => {
                if let Some(p) = polegar {
                    rolar_para(&mut i, p, c.y - barra.unwrap_or(0.0));
                }
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) if barra.is_some() => {
                cx.states.get_mut::<Mouse>(n.key).barra = None;
                Captured::Yes
            }
            Event::MousePressed(MouseButton::Left) if dentro => {
                cx.states.focus = Some(n.key);
                let (x, y) = ponto(cx.cursor.unwrap_or(Point::ORIGIN));
                let agora = std::time::Instant::now();
                let m = cx.states.get_mut::<Mouse>(n.key);
                let vezes = match m.ultimo {
                    Some((t, k)) if !m.caixa && agora.duration_since(t).as_millis() < 400 => k % 3 + 1,
                    _ => 1,
                };
                m.ultimo = Some((agora, vezes));
                m.arrastando = true;
                let acao = match vezes {
                    1 => Action::Click { x, y },
                    2 => Action::DoubleClick { x, y },
                    _ => Action::TripleClick { x, y },
                };
                let fonts = cx.renderer.fonts();
                let ed = i.ed.as_mut().expect("montado acima");
                let antes = ed.cursor();
                ed.action(fonts, acao);
                let c = ed.cursor();
                // Clique na caixinha desenhada: marca/desmarca e o cursor fica onde estava.
                let caixa = (self.markdown && vezes == 1).then(|| ed.with_buffer(|b| caixa_da_linha(b.lines[c.line].text()))).flatten();
                let caixa = caixa.filter(|&(k, _)| (k..=k + 3).contains(&c.index));
                cx.states.get_mut::<Mouse>(n.key).caixa = caixa.is_some();
                if let Some((k, marcada)) = caixa {
                    i.editar(false, |ed| {
                        ed.delete_range(Cursor::new(c.line, k + 1), Cursor::new(c.line, k + 2));
                        ed.insert_at(Cursor::new(c.line, k + 1), if marcada { " " } else { "x" }, None);
                        ed.set_selection(Selection::None);
                        ed.set_cursor(antes);
                    });
                }
                i.digitando = None;
                Captured::Yes
            }
            Event::CursorMoved(p) if cx.states.get::<Mouse>(n.key).is_some_and(|m| m.arrastando) => {
                let (x, y) = ponto(*p);
                let fonts = cx.renderer.fonts();
                i.ed.as_mut().expect("montado acima").action(fonts, Action::Drag { x, y });
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) if cx.states.get::<Mouse>(n.key).is_some_and(|m| m.arrastando) => {
                cx.states.get_mut::<Mouse>(n.key).arrastando = false;
                Captured::Yes
            }
            Event::Wheel { dy, modifiers, .. } if dentro && !modifiers.control_key() => {
                let ed = i.ed.as_mut().expect("montado acima");
                ed.with_buffer_mut(|b| {
                    let mut sc = b.scroll();
                    sc.vertical -= dy * s;
                    b.set_scroll(sc);
                });
                Captured::Yes
            }
            Event::Key { key, modifiers, pressed: true, .. } if focado && self.on_key.as_ref().and_then(|f| f(key, *modifiers)).map(|m| cx.publish(m)).is_some() => Captured::Yes,
            Event::Key { key, text, modifiers, pressed: true } if focado => self.tecla(&mut i, key, text.as_deref(), *modifiers, cx),
            _ => Captured::No,
        };
        if resultado == Captured::Yes {
            cx.redraw = true;
            if i.versao != antes {
                if let Some(m) = &self.on_change {
                    cx.publish(m.clone());
                }
            }
        }
        resultado
    }
}

impl<M: Clone> TextEditor<'_, M> {
    fn tecla(&self, i: &mut Interno, key: &Key, text: Option<&str>, modifiers: crate::Modifiers, cx: &mut EventCx<M>) -> Captured {
        let (ctrl, shift) = (modifiers.control_key() || modifiers.super_key(), modifiers.shift_key());
        let fonts = cx.renderer.fonts();
        let movimento = |m: Motion, i: &mut Interno, fonts: &mut FontSystem| {
            let ed = i.ed.as_mut().expect("montado");
            match (shift, ed.selection()) {
                (true, Selection::None) => ed.set_selection(Selection::Normal(ed.cursor())),
                (false, sel) if sel != Selection::None => {
                    let pontas = ed.selection_bounds();
                    ed.set_selection(Selection::None);
                    // Esquerda/direita com seleção só recolhem para a ponta.
                    if let (Some((a, z)), Motion::Left | Motion::Right) = (pontas, m) {
                        ed.set_cursor(if m == Motion::Left { a } else { z });
                        return;
                    }
                }
                _ => {}
            }
            ed.action(fonts, Action::Motion(m));
            i.digitando = None;
        };
        let acao = |a: Action, digitacao: bool, i: &mut Interno, fonts: &mut FontSystem| i.editar(digitacao, |ed| ed.action(fonts, a));
        match key {
            Key::Named(NamedKey::ArrowLeft) => movimento(if ctrl { Motion::LeftWord } else { Motion::Left }, i, fonts),
            Key::Named(NamedKey::ArrowRight) => movimento(if ctrl { Motion::RightWord } else { Motion::Right }, i, fonts),
            Key::Named(NamedKey::ArrowUp) => movimento(Motion::Up, i, fonts),
            Key::Named(NamedKey::ArrowDown) => movimento(Motion::Down, i, fonts),
            Key::Named(NamedKey::Home) => movimento(if ctrl { Motion::BufferStart } else { Motion::Home }, i, fonts),
            Key::Named(NamedKey::End) => movimento(if ctrl { Motion::BufferEnd } else { Motion::End }, i, fonts),
            Key::Named(NamedKey::PageUp) => movimento(Motion::PageUp, i, fonts),
            Key::Named(NamedKey::PageDown) => movimento(Motion::PageDown, i, fonts),
            Key::Named(NamedKey::Backspace) => acao(Action::Backspace, false, i, fonts),
            Key::Named(NamedKey::Delete) => acao(Action::Delete, false, i, fonts),
            Key::Named(NamedKey::Enter) => acao(Action::Enter, false, i, fonts),
            Key::Named(NamedKey::Tab) if !ctrl => acao(Action::Insert('\t'), false, i, fonts),
            Key::Character(k) if ctrl => {
                let ed = i.ed.as_mut().expect("montado");
                match k.to_lowercase().as_str() {
                    "a" => {
                        let fim = ed.with_buffer(|b| b.lines.last().map_or(Cursor::new(0, 0), |l| Cursor::new(b.lines.len() - 1, l.text().len())));
                        ed.set_selection(Selection::Normal(Cursor::new(0, 0)));
                        ed.set_cursor(fim);
                    }
                    "c" => {
                        if let Some(t) = ed.copy_selection() {
                            cx.clipboard.write(t);
                        }
                    }
                    "x" => {
                        if let Some(t) = ed.copy_selection() {
                            cx.clipboard.write(t);
                            i.editar(false, |ed| { ed.delete_selection(); });
                        }
                    }
                    "v" => {
                        if let Some(t) = cx.clipboard.read() {
                            // \r\n de outro programa vira \n (o buffer guarda a quebra de cada linha).
                            let t = t.replace("\r\n", "\n");
                            i.editar(false, |ed| ed.insert_string(&t, None));
                        }
                    }
                    "z" if shift => i.voltar(false),
                    "z" => i.voltar(true),
                    "y" => i.voltar(false),
                    _ => return Captured::No,
                }
            }
            _ => match text.filter(|t| !ctrl && !t.is_empty() && t.chars().all(|c| !c.is_control())) {
                Some(t) => i.editar(true, |ed| {
                    for c in t.chars() {
                        ed.action(fonts, Action::Insert(c));
                    }
                }),
                None => return Captured::No,
            },
        }
        Captured::Yes
    }
}

impl<'a, M: Clone + 'a> From<TextEditor<'a, M>> for crate::Element<'a, M> {
    fn from(w: TextEditor<'a, M>) -> Self {
        crate::Element::new(w)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn polegar_da_barra_de_rolagem() {
        let c = Content::new();
        let ed: TextEditor<'_, ()> = text_editor(&c).line_height(20.0);
        let n = Node { rect: Rect { x: 0.0, y: 0.0, width: 300.0, height: 200.0 }, ..Default::default() };
        let area = n.rect;
        // 10 linhas à vista: com 8 linhas não há barra.
        assert!(ed.polegar(&n, area, 8, 0.0).is_none());
        let cima = ed.polegar(&n, area, 100, 0.0).expect("100 linhas não cabem");
        assert_eq!((cima.y, cima.height), (0.0, 24.0));
        let baixo = ed.polegar(&n, area, 100, 90.0).expect("idem");
        assert_eq!(baixo.y + baixo.height, 200.0);
        assert_eq!(baixo.x, 290.0);
    }

    struct Visto {
        weight: Weight,
        metrics_opt: bool,
        color_opt: Option<cosmic_text::Color>,
        metadata: usize,
        style: Estilo,
    }

    #[test]
    fn markdown_marca_cada_trecho() {
        let base = Attrs::new();
        let t = Tons { sinal: cosmic_text::Color::rgb(1, 1, 1), link: cosmic_text::Color::rgb(2, 2, 2), avisos: Vec::new() };
        let m = Metrics::new(15.0, 22.0);
        // (peso, tem tamanho próprio, cor, marca, estilo) do byte `i`.
        let estilo = |texto: &str, i: usize| {
            let l = markdown(texto, base, m, &t, &mut false, false);
            let a = l.get_span(i);
            Visto { weight: a.weight, metrics_opt: a.metrics_opt.is_some(), color_opt: a.color_opt, metadata: a.metadata, style: a.style }
        };
        // Título: maior e em negrito; o "#" apagado.
        assert_eq!(estilo("# Título", 3).weight, Weight::BOLD);
        assert!(estilo("# Título", 3).metrics_opt);
        assert_eq!(estilo("# Título", 0).color_opt, Some(t.sinal));
        // Negrito com os sinais apagados, e o texto depois volta ao normal.
        assert_eq!(estilo("**n** x", 2).weight, Weight::BOLD);
        assert_eq!(estilo("**n** x", 0).color_opt, Some(t.sinal));
        assert_eq!(estilo("**n** x", 6).weight, Weight::NORMAL);
        // Item feito riscado; link sublinhado; sublinhado no meio da palavra não é itálico.
        assert_eq!(estilo("- [x] feito", 7).metadata, TACHADO);
        assert_eq!(estilo("- [ ] falta", 7).metadata, 0);
        assert_eq!(estilo("[a](b)", 1).metadata, LINK);
        // Com aviso no endereço, o link fica como texto: nada escondido.
        let t2 = Tons { sinal: t.sinal, link: t.link, avisos: vec![4..5] };
        let l = markdown("[a](b)", base, m, &t2, &mut false, true);
        assert!((0..6).all(|i| l.get_span(i).metadata != LINK && l.get_span(i).color_opt.is_none_or(|c| c.a() > 0)));
        assert_eq!(estilo("snake_case_nome", 6).style, Estilo::Normal);
        assert_eq!(estilo("um *dois* tres", 5).style, Estilo::Italic);
        // Fora da linha do cursor os sinais somem (transparentes); o texto fica.
        let longe = markdown("**n**", base, m, &t, &mut false, true);
        assert_eq!(longe.get_span(0).color_opt.map(|c| c.a()), Some(0));
        assert_eq!(longe.get_span(2).weight, Weight::BOLD);
        // Escondido: o negrito dentro do título fica do tamanho do título; "-" vira marcador, o de tarefa some.
        let esc = |texto: &str, i: usize| { let l = markdown(texto, base, m, &t, &mut false, true); let a = l.get_span(i); (a.metrics_opt.is_some(), a.metadata, a.color_opt.map(|c| c.a())) };
        assert!(esc("## **oi**", 5).0);
        assert_eq!(esc("~~a **oi** b~~", 5).2, Some(0));
        assert_eq!(esc("~~a **oi** b~~", 7).1, TACHADO);
        assert_eq!(esc("- [x] **oi**", 9).1, TACHADO);
        // Os três juntos: nenhum sinal à mostra.
        let tres = "***~~oi~~***";
        assert!((0..tres.len()).filter(|&i| tres.as_bytes()[i] != b'o' && tres.as_bytes()[i] != b'i').all(|i| esc(tres, i).2 == Some(0)));
        assert_eq!(esc("- item", 0).1, MARCADOR);
        assert_eq!(esc("- [ ] x", 0).2, Some(0));
        assert_eq!(esc("- [ ] x", 0).1, 0);
        assert_eq!(esc("> dito", 0).1, CITACAO);
        // Dentro de um bloco de código nada vira negrito.
        let mut dentro = false;
        markdown("```", base, m, &t, &mut dentro, false);
        assert_eq!(markdown("**x**", base, m, &t, &mut dentro, false).get_span(2).weight, Weight::NORMAL);
    }

    /// Abrir e ir até o fim de um texto grande monta só as linhas à vista, nunca o arquivo todo.
    #[test]
    fn texto_grande_so_monta_o_visivel() {
        let mut fonts = FontSystem::new();
        let texto = "uma linha de texto qualquer\n".repeat(20_000);
        let c = Content::with_text(&texto);
        let mut i = c.0.borrow_mut();
        let montadas = |ed: &Editor<'static>| ed.with_buffer(|b| b.lines.iter().filter(|l| l.shape_opt().is_some()).count());
        let ed = i.editor(&mut fonts, 15.0, 15.0 * 1.45, 1.0, Font::DEFAULT, true);
        // O pico ao abrir: montar tudo e soltar depois deixa a memória presa no alocador.
        assert!(montadas(ed) < 100, "pôr o texto montou {}", montadas(ed));
        ed.with_buffer_mut(|b| b.set_size(&mut fonts, Some(600.0), Some(400.0)));
        ed.shape_as_needed(&mut fonts, true);
        assert!(montadas(ed) < 100, "abrir montou {}", montadas(ed));
        ed.action(&mut fonts, Action::Motion(Motion::BufferEnd));
        ed.shape_as_needed(&mut fonts, true);
        assert!(montadas(ed) < 100, "ir ao fim montou {}", montadas(ed));
        assert!(ed.with_buffer(|b| b.scroll().line) > 19_000);
    }

    #[test]
    fn desfazer_tem_teto() {
        let mut fonts = FontSystem::new();
        let c = Content::with_text("");
        c.0.borrow_mut().editor(&mut fonts, 15.0, 15.0 * 1.45, 1.0, Font::DEFAULT, true);
        for _ in 0..600 {
            c.insert("x");
        }
        assert_eq!(c.0.borrow().desfazer.len(), DESFAZER_MAX);
        c.undo();
        assert_eq!(c.text().trim_end(), "x".repeat(599));
    }
}
