//! Desenho do quadro na CPU. Tudo entra em pixels lógicos e sai multiplicado pela escala.
use crate::{Background, Border, Color, Point, Rect, Shadow, Size};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use std::collections::HashMap;
use std::sync::Arc;
pub(crate) mod vetor;
pub(crate) mod gpu;
use tiny_skia::{FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, SpreadMode, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Font {
    /// `None` = sem serifa do sistema.
    pub family: Option<&'static str>,
    pub weight: u16,
    pub italic: bool,
}

impl Font {
    pub const DEFAULT: Font = Font { family: None, weight: 400, italic: false };
    pub const MEDIUM: Font = Font { family: None, weight: 500, italic: false };
    pub const SEMIBOLD: Font = Font { family: None, weight: 600, italic: false };
    pub const BOLD: Font = Font { family: None, weight: 700, italic: false };
    pub const MONO: Font = Font { family: Some("monospace"), weight: 400, italic: false };
    pub const SERIF: Font = Font { family: Some("serif"), weight: 400, italic: false };

    /// A família pelo nome (do sistema ou carregada com [`load_font`]).
    pub const fn with_name(nome: &'static str) -> Font {
        Font { family: Some(nome), weight: 400, italic: false }
    }
    pub const fn weight(mut self, w: u16) -> Font {
        self.weight = w;
        self
    }
    pub const fn italic(mut self) -> Font {
        self.italic = true;
        self
    }
}

/// O jeito do cosmic-text de pedir esta fonte.
pub(crate) fn attrs(f: Font) -> Attrs<'static> {
    let familia = match f.family {
        None => Family::SansSerif,
        Some("monospace") => Family::Monospace,
        Some("serif") => Family::Serif,
        Some(nome) => Family::Name(nome),
    };
    Attrs::new().weight(Weight(f.weight)).family(familia).style(if f.italic { cosmic_text::Style::Italic } else { cosmic_text::Style::Normal })
}

static FONTES_EXTRA: std::sync::Mutex<Vec<Arc<Vec<u8>>>> = std::sync::Mutex::new(Vec::new());

/// Registra uma fonte (TTF/OTF, ou coleção) para todas as janelas; depois é só pedir pelo nome
/// da família com [`Font::with_name`]. Vale do próximo quadro em diante.
pub fn load_font(bytes: impl Into<Vec<u8>>) {
    FONTES_EXTRA.lock().unwrap_or_else(|e| e.into_inner()).push(Arc::new(bytes.into()));
}

impl Default for Font {
    fn default() -> Self {
        Font::DEFAULT
    }
}

/// Número único de cada imagem/SVG criado: a chave dos caches. O endereço não serve,
/// porque o de uma imagem já liberada volta para a próxima.
fn novo_id() -> u64 {
    static PROXIMO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    PROXIMO.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Imagem já decodificada, compartilhada barato entre quadros.
#[derive(Clone)]
pub struct Image(Arc<Pixmap>, u64);

impl Image {
    /// Pixels RGBA8 sem pré-multiplicar, linha a linha. `None` se o tamanho não bate.
    pub fn from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Self> {
        if rgba.len() as u64 != width as u64 * height as u64 * 4 {
            return None;
        }
        for p in rgba.chunks_exact_mut(4) {
            let a = p[3] as u16;
            for c in &mut p[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
        Some(Image(Arc::new(Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(width, height)?)?), novo_id()))
    }
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        Pixmap::decode_png(bytes).ok().map(|p| Image(Arc::new(p), novo_id()))
    }
    pub fn size(&self) -> Size {
        Size::new(self.0.width() as f32, self.0.height() as f32)
    }
}

/// SVG analisado uma vez; a rasterização fica em cache por tamanho e cor.
#[derive(Clone)]
pub struct Svg(Arc<resvg::usvg::Tree>, u64);

impl Svg {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok().map(|t| Svg(Arc::new(t), novo_id()))
    }
    pub fn size(&self) -> Size {
        let s = self.0.size();
        Size::new(s.width(), s.height())
    }
}

/// Altura da linha padrão do texto, em múltiplos do tamanho.
const ALTURA_LINHA: f32 = 1.3;

#[derive(PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    size: u32,
    font: Font,
    width: Option<u32>,
    line_height: u32,
}

#[derive(PartialEq, Eq, Hash)]
struct SvgKey {
    tree: u64,
    w: u32,
    h: u32,
    color: Option<[u8; 4]>,
}

#[derive(PartialEq, Eq, Hash)]
struct ShadowKey {
    w: u32,
    h: u32,
    radius: [u32; 4],
    blur: u32,
    color: [u8; 4],
}

struct Clip {
    /// O retângulo visível: este recorte já cruzado com os de fora.
    rect: Rect,
    own: Rect,
    radius: [f32; 4],
    /// Algum canto arredondado da pilha alcança este recorte: aí só a máscara resolve.
    rounded: bool,
    /// Os quadrados dos cantos arredondados da pilha que tocam este recorte: um recorte de dentro
    /// que não encoste em nenhum é só retângulo.
    cantos: Vec<Canto>,
    /// Feita só quando um desenho pode sair do recorte; vale até o `pop_clip`.
    mask: Option<Mask>,
    /// O mesmo recorte para a GPU (veja [`gpu::SEM_RECORTE`]).
    gpu: [f32; 12],
}

/// O quadrado de um canto arredondado e o centro do arco dele (lógicos).
#[derive(Clone, Copy)]
struct Canto {
    quadrado: Rect,
    centro: Point,
    raio: f32,
}

/// A máscara do recorte atual, só se o desenho em `area` (pixels físicos) puder sair dele.
/// `None` em `area` = sempre (texto, que não sabe o tamanho antes).
fn clip_mask(clips: &mut [Clip], w: u32, h: u32, s: f32, area: Option<tiny_skia::Rect>) -> Option<&Mask> {
    let (r, rounded) = clips.last().map(|c| (c.rect, c.rounded))?;
    let dentro = area.is_some_and(|a| a.left() >= r.x * s && a.top() >= r.y * s && a.right() <= (r.x + r.width) * s && a.bottom() <= (r.y + r.height) * s);
    if !rounded && dentro {
        return None;
    }
    let n = clips.len();
    if clips[n - 1].mask.is_none() {
        let mut m = Mask::new(w, h)?;
        if !rounded {
            // Só retângulos: a máscara é o cruzamento deles (`r`), preenchido direto. Rasterizar
            // cada recorte na máscara inteira custava várias passadas na tela toda por quadro.
            let (x0, y0) = (((r.x * s).round().max(0.0) as usize).min(w as usize), ((r.y * s).round().max(0.0) as usize).min(h as usize));
            let (x1, y1) = ((((r.x + r.width) * s).round().max(0.0) as usize).min(w as usize), (((r.y + r.height) * s).round().max(0.0) as usize).min(h as usize));
            let dados = m.data_mut();
            for y in y0..y1.max(y0) {
                dados[y * w as usize + x0..y * w as usize + x1.max(x0)].fill(255);
            }
            clips[n - 1].mask = Some(m);
            return clips[n - 1].mask.as_ref();
        }
        // O retângulo cheio e, nos quadrados dos cantos, a cobertura do arco calculada pixel a
        // pixel: rasterizar cada contorno na máscara inteira custava a tela toda por recorte.
        let (x0, y0) = (((r.x * s).round().max(0.0) as usize).min(w as usize), ((r.y * s).round().max(0.0) as usize).min(h as usize));
        let (x1, y1) = ((((r.x + r.width) * s).round().max(0.0) as usize).min(w as usize), (((r.y + r.height) * s).round().max(0.0) as usize).min(h as usize));
        let largura = w as usize;
        let dados = m.data_mut();
        for y in y0..y1.max(y0) {
            dados[y * largura + x0..y * largura + x1.max(x0)].fill(255);
        }
        for k in &clips[n - 1].cantos {
            let q = k.quadrado;
            let (qx0, qy0) = (((q.x * s).floor().max(x0 as f32) as usize), ((q.y * s).floor().max(y0 as f32) as usize));
            let (qx1, qy1) = ((((q.x + q.width) * s).ceil() as usize).min(x1), (((q.y + q.height) * s).ceil() as usize).min(y1));
            let (cx, cy, raio) = (k.centro.x * s, k.centro.y * s, k.raio * s);
            for y in qy0..qy1.max(qy0) {
                for x in qx0..qx1.max(qx0) {
                    let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                    let cob = (raio - d + 0.5).clamp(0.0, 1.0);
                    let px = &mut dados[y * largura + x];
                    *px = (*px as f32 * cob) as u8;
                }
            }
        }
        clips[n - 1].mask = Some(m);
    }
    clips[n - 1].mask.as_ref()
}

pub struct Renderer {
    pub pixmap: Pixmap,
    scale: f32,
    fonts: FontSystem,
    swash: SwashCache,
    texts: Cache<TextKey, Buffer>,
    /// Buffers montados por quem chama (texto rico), pela chave que ele dá.
    proprios: Cache<u64, Buffer>,
    /// Quantas de [`load_font`] já estão no banco.
    fontes_carregadas: usize,
    /// [`crate::Settings::text_cache`]: os textos ficam de um quadro para o outro.
    pub(crate) cache_textos: bool,
    svgs: Cache<SvgKey, Arc<Pixmap>>,
    shadows: Cache<ShadowKey, Pixmap>,
    imagens: Cache<(u64, u32, u32), Pixmap>,
    clips: Vec<Clip>,
    /// Num quadro parcial: onde o `pixmap` (pequeno) começa, em lógico e em pixels, e o
    /// quadro cheio que espera a região redesenhada.
    origem: Point,
    desloc: (i32, i32),
    parcial: Option<Parcial>,
    /// Os vidros (área lógica e raio) do quadro anterior: só perto deles o quadro parcial
    /// precisa redesenhar uma margem a mais.
    vidros: Vec<(Rect, f32)>,
    /// Gravando: os desenhos só anotam área e impressão digital (veja [`Renderer::record`]).
    gravando: bool,
    lista: Vec<(Rect, u64)>,
    /// O que o quadro que está no `pixmap` desenhou.
    anterior: Vec<(Rect, u64)>,
    /// As linhas (pixels) que o último quadro mudou: `None` = todas.
    mudou: Option<(u32, u32)>,
    /// O alfa que multiplica tudo que for desenhado ([`push_opacity`](Self::push_opacity)), e os de fora.
    opacidade: f32,
    opacidades: Vec<f32>,
    camadas: Vec<Camada>,
    /// As de [`Renderer::push_transform`], já compostas.
    transformacoes: Vec<vetor::Transform>,
    /// Pintando para a GPU ([`Renderer::begin_gpu`]): os desenhos vão para a lista, não para o `pixmap`.
    pintando_gpu: bool,
    pub(crate) gpu: gpu::Lista,
    /// O primeiro desenho do quadro que a GPU ainda não sabe fazer: aí o quadro todo sai na CPU.
    pub(crate) falta_gpu: Option<&'static str>,
    /// O tamanho (pixels) do último quadro desta janela: na GPU o `pixmap` fica para trás.
    tela: (u32, u32),
}

/// O que um quadro deixa para o próximo. Com várias janelas, cada uma guarda o seu e troca com
/// o renderizador na hora de desenhar: as fontes e os caches ficam um só.
pub(crate) struct Quadro {
    pixmap: Pixmap,
    scale: f32,
    anterior: Vec<(Rect, u64)>,
    vidros: Vec<(Rect, f32)>,
    mudou: Option<(u32, u32)>,
    tela: (u32, u32),
}

impl Default for Quadro {
    fn default() -> Self {
        Quadro { pixmap: Pixmap::new(1, 1).expect("1x1"), scale: 1.0, anterior: Vec::new(), vidros: Vec::new(), mudou: None, tela: (1, 1) }
    }
}

/// Uma camada aberta ([`Renderer::push_layer`]).
struct Camada {
    /// A área que ela cobre na tela já transformada (lógica): gravando, os desenhos de dentro anotam esta.
    caixa: Rect,
    /// A impressão digital da transformação: mudou a escala, muda tudo o que está dentro.
    chave: u64,
    escala: f32,
    rotacao: f32,
    alfa: f32,
    /// Pintando: onde o pixmap dela começa no de fora (pixels) e o que estava sendo desenhado antes.
    canto: (i32, i32),
    /// O centro da área (pixels do de fora): em volta dele gira e escala.
    centro: (f32, f32),
    fora: Option<Fora>,
}

struct Fora {
    pixmap: Pixmap,
    origem: Point,
    desloc: (i32, i32),
    clips: Vec<Clip>,
    opacidade: f32,
    opacidades: Vec<f32>,
}

struct Parcial {
    cheio: Pixmap,
    /// A parte que vale de verdade (a margem em volta só serve ao desfoque), em pixels.
    dentro: (i32, i32, i32, i32),
}

/// As fontes do sistema. O fontdb chama de monoespaçada a "Courier New", que no Linux cai na
/// Courier fina: troca pela primeira monoespaçada moderna instalada.
fn fontes() -> FontSystem {
    let mut f = FontSystem::new();
    let db = f.db_mut();
    let tem = |nome: &str| db.faces().any(|face| face.families.iter().any(|(n, _)| n == nome));
    if let Some(nome) = ["Cascadia Mono", "Consolas", "SF Mono", "Menlo", "DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono"].into_iter().find(|n| tem(n)) {
        db.set_monospace_family(nome);
    }
    f
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            pixmap: Pixmap::new(1, 1).expect("1x1"),
            scale: 1.0,
            fonts: fontes(),
            swash: SwashCache::new(),
            texts: Cache::default(),
            proprios: Cache::default(),
            fontes_carregadas: 0,
            cache_textos: false,
            svgs: Cache::default(),
            shadows: Cache::default(),
            imagens: Cache::default(),
            clips: Vec::new(),
            origem: Point::ORIGIN,
            desloc: (0, 0),
            parcial: None,
            vidros: Vec::new(),
            gravando: false,
            lista: Vec::new(),
            anterior: Vec::new(),
            mudou: None,
            opacidade: 1.0,
            opacidades: Vec::new(),
            camadas: Vec::new(),
            transformacoes: Vec::new(),
            pintando_gpu: false,
            gpu: gpu::Lista::default(),
            falta_gpu: None,
            tela: (1, 1),
        }
    }

    /// Troca o quadro guardado pelo que está aqui (veja [`Quadro`]).
    pub(crate) fn trocar(&mut self, q: &mut Quadro) {
        self.end();
        // Antes do layout: medir já com a fonte nova.
        self.carregar_fontes();
        // A janela troca no começo e no fim de cada quadro: cada quadro é uma geração, cheio ou
        // parcial. Senão, rolando (só quadros parciais), tudo parecia do quadro atual e o cache
        // de textos crescia sem fim.
        let manter = if self.cache_textos { 8 } else { 0 };
        self.texts.esquecer_velhos(manter);
        self.proprios.esquecer_velhos(manter);
        self.texts.geracao += 1;
        self.proprios.geracao += 1;
        self.svgs.geracao += 1;
        self.shadows.geracao += 1;
        self.imagens.geracao += 1;
        std::mem::swap(&mut self.pixmap, &mut q.pixmap);
        std::mem::swap(&mut self.anterior, &mut q.anterior);
        std::mem::swap(&mut self.vidros, &mut q.vidros);
        std::mem::swap(&mut self.mudou, &mut q.mudou);
        std::mem::swap(&mut self.tela, &mut q.tela);
        // A escala vale do quadro: trocar não limpa os caches (o `begin` limpa se mudar de verdade).
        std::mem::swap(&mut self.scale, &mut q.scale);
    }

    /// Prepara um quadro novo de `size` lógico na escala `scale`, limpo com `fundo`.
    pub fn begin(&mut self, size: Size, scale: f32, fundo: Color) {
        let (w, h) = self.preparar(size, scale);
        if self.pixmap.width() != w || self.pixmap.height() != h {
            self.pixmap = Pixmap::new(w, h).expect("tamanho válido");
        }
        self.pixmap.fill(skia_color(fundo));
    }

    /// Como o [`begin`](Self::begin), mas os desenhos vão para a lista da GPU ([`gpu::Lista`]),
    /// que a janela entrega ao [`gpu::Pintor`]. O `pixmap` não muda.
    pub(crate) fn begin_gpu(&mut self, size: Size, scale: f32, fundo: Color) {
        let (w, h) = self.preparar(size, scale);
        self.gpu.limpar(w, h, fundo);
        self.pintando_gpu = true;
    }

    /// O comum aos quadros cheios: caches, escala, recortes. Devolve o tamanho em pixels.
    fn preparar(&mut self, size: Size, scale: f32) -> (u32, u32) {
        self.end();
        self.carregar_fontes();
        let (w, h) = (((size.width * scale).round() as u32).max(1), ((size.height * scale).round() as u32).max(1));
        self.tela = (w, h);
        if self.scale != scale {
            self.texts.clear();
            self.proprios.clear();
            self.svgs.clear();
        }
        // Um quadro cheio desenha tudo o que está à mostra: o que nem ele nem o layout usaram
        // (outro tamanho de janela no meio de um resize, um menu que fechou) sai do cache.
        let antes = self.svgs.bytes() + self.shadows.bytes() + self.imagens.bytes();
        self.texts.virar();
        self.proprios.virar();
        self.svgs.virar();
        self.shadows.virar();
        self.imagens.virar();
        if antes - (self.svgs.bytes() + self.shadows.bytes() + self.imagens.bytes()) > 4 << 20 {
            devolver_memoria();
        }
        self.scale = scale;
        self.clips.clear();
        self.vidros.clear();
        self.mudou = None;
        self.sem_opacidade();
        (w, h)
    }

    /// Redesenha só `sujo` (lógico) sobre o quadro anterior: tudo que for desenhado até o
    /// [`end`](Self::end) cai num pixmap do tamanho da região. `false` se não dá (o tamanho
    /// ou a escala mudou, ou não há quadro anterior): aí use [`begin`](Self::begin).
    pub fn begin_parcial(&mut self, size: Size, scale: f32, fundo: Color, sujo: Rect) -> bool {
        self.end();
        let (w, h) = (((size.width * scale).round() as i32).max(1), ((size.height * scale).round() as i32).max(1));
        if self.pixmap.width() as i32 != w || self.pixmap.height() as i32 != h || self.scale != scale {
            return false;
        }
        let (x0, y0) = (((sujo.x * scale).floor() as i32).max(0), ((sujo.y * scale).floor() as i32).max(0));
        let (x1, y1) = ((((sujo.x + sujo.width) * scale).ceil() as i32).min(w), (((sujo.y + sujo.height) * scale).ceil() as i32).min(h));
        if x0 >= x1 || y0 >= y1 {
            return false;
        }
        // O desfoque lê o entorno: perto de um vidro, a região cresce o raio dele.
        let raio = self.vidros.iter().filter(|(v, r)| cruza(*v, crescer(sujo, *r))).fold(0.0f32, |a, (_, r)| a.max(*r));
        let m = if raio > 0.0 { ((raio + 12.0) * scale).ceil() as i32 } else { 0 };
        let (ex0, ey0) = (((x0 - m).max(0) / 12) * 12, ((y0 - m).max(0) / 12) * 12);
        let (ex1, ey1) = ((x1 + m).min(w), (y1 + m).min(h));
        let mut peq = Pixmap::new((ex1 - ex0) as u32, (ey1 - ey0) as u32).expect("região não vazia");
        peq.fill(skia_color(fundo));
        let cheio = std::mem::replace(&mut self.pixmap, peq);
        self.parcial = Some(Parcial { cheio, dentro: (x0, y0, x1, y1) });
        self.origem = Point::new(ex0 as f32 / scale, ey0 as f32 / scale);
        self.desloc = (ex0, ey0);
        self.clips.clear();
        self.sem_opacidade();
        true
    }

    /// Fecha um quadro parcial, colando a região no quadro cheio (sem efeito num quadro cheio).
    pub fn end(&mut self) {
        self.pintando_gpu = false;
        // Uma camada esquecida aberta ainda tem o pixmap de fora guardado.
        while !self.camadas.is_empty() {
            self.pop_layer();
        }
        let Some(Parcial { mut cheio, dentro: (x0, y0, x1, y1) }) = self.parcial.take() else { return };
        let (ex, ey) = self.desloc;
        let (pw, cw) = (self.pixmap.width() as usize * 4, cheio.width() as usize * 4);
        let (src, dst) = (self.pixmap.data(), cheio.data_mut());
        for y in y0..y1 {
            let s0 = (y - ey) as usize * pw + (x0 - ex) as usize * 4;
            let d0 = y as usize * cw + x0 as usize * 4;
            let n = (x1 - x0) as usize * 4;
            dst[d0..d0 + n].copy_from_slice(&src[s0..s0 + n]);
        }
        self.pixmap = cheio;
        // Várias regiões no mesmo quadro: as linhas mudadas são a união delas.
        self.mudou = Some(self.mudou.map_or((y0 as u32, y1 as u32), |(a, b)| (a.min(y0 as u32), b.max(y1 as u32))));
        self.origem = Point::ORIGIN;
        self.desloc = (0, 0);
    }

    /// Tamanho lógico da janela inteira (mesmo durante um quadro parcial).
    pub fn window_size(&self) -> Size {
        Size::new(self.tela.0 as f32 / self.scale, self.tela.1 as f32 / self.scale)
    }

    /// As linhas que o último quadro mudou (`None` = o quadro todo): só elas precisam subir para a GPU.
    pub fn changed_rows(&self) -> Option<(u32, u32)> {
        self.mudou
    }

    /// Para achar onde a RAM vai (`PELICANO_MEMORIA=1`): o tamanho de cada cache e do quadro.
    pub(crate) fn memoria(&self) -> String {
        let mib = |b: usize| b as f64 / 1048576.0;
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let heap = {
            #[repr(C)]
            struct Mallinfo2 { arena: usize, ordblks: usize, smblks: usize, hblks: usize, hblkhd: usize, usmblks: usize, fsmblks: usize, uordblks: usize, fordblks: usize, keepcost: usize }
            extern "C" {
                fn mallinfo2() -> Mallinfo2;
            }
            // SAFETY: função do glibc sem pré-condições.
            let m = unsafe { mallinfo2() };
            format!("heap em uso {:.1} MiB, livre guardado {:.1} MiB, mmap {:.1} MiB · ", mib(m.uordblks), mib(m.fordblks), mib(m.hblkhd))
        };
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        let heap = String::new();
        format!("{heap}quadro {:.1} MiB · textos {} + {} · svgs {} ({:.1} MiB) · sombras {} ({:.1} MiB) · imagens {} ({:.1} MiB) · glifos {} · fontes {}",
            mib(self.pixmap.data().len()), self.texts.mapa.len(), self.proprios.mapa.len(),
            self.svgs.mapa.len(), mib(self.svgs.bytes()), self.shadows.mapa.len(), mib(self.shadows.bytes()),
            self.imagens.mapa.len(), mib(self.imagens.bytes()), self.swash.image_cache.len(), self.fonts.db().len())
    }

    /// Começa a gravar o quadro: até o [`changes`](Self::changes), cada desenho só anota a
    /// área que pinta e uma impressão digital de tudo que muda o resultado. Comparar com o
    /// quadro anterior diz o que redesenhar (nada, se o mouse andou sem mudar o hover).
    pub fn record(&mut self) {
        self.end();
        self.gravando = true;
        self.mudou = Some((u32::MAX, 0));
        self.lista.clear();
        self.vidros.clear();
        self.clips.clear();
        self.falta_gpu = None;
        self.sem_opacidade();
    }

    /// Este quadro tem um desenho que a GPU ainda não faz: sai todo na CPU.
    fn falta(&mut self, motivo: &'static str) {
        self.falta_gpu.get_or_insert(motivo);
    }

    fn sem_opacidade(&mut self) {
        self.opacidade = 1.0;
        self.opacidades.clear();
    }

    /// Daqui até o [`pop_opacity`](Self::pop_opacity), tudo sai com o alfa multiplicado por `a`
    /// (o `opacity` do CSS; dentro de outro, multiplica).
    /// shortcut: cada desenho fica translúcido sozinho, não o grupo: onde dois se sobrepõem o de baixo aparece um pouco; uma camada à parte resolve se incomodar.
    pub fn push_opacity(&mut self, a: f32) {
        self.opacidades.push(self.opacidade);
        self.opacidade *= a.clamp(0.0, 1.0);
    }

    pub fn pop_opacity(&mut self) {
        self.opacidade = self.opacidades.pop().unwrap_or(1.0);
    }

    /// Daqui até o [`pop_layer`](Self::pop_layer) tudo cai numa camada à parte do tamanho de
    /// `area`, que no fim vai para a tela escalada por `scale` e girada `rotate` graus em volta
    /// do centro de `area`, com opacidade `opacity` (o `transform` e o `opacity` do CSS, para o grupo).
    /// shortcut: um vidro dentro da camada não vê o que está atrás dela (desfoca transparente).
    pub fn push_layer(&mut self, area: Rect, scale: f32, rotate: f32, opacity: f32) {
        self.falta("camada");
        let (sen, cos) = rotate.to_radians().sin_cos();
        let (hw, hh) = (area.width * scale / 2.0, area.height * scale / 2.0);
        let (bw, bh) = (hw * cos.abs() + hh * sen.abs(), hw * sen.abs() + hh * cos.abs());
        let (mx, my) = (area.x + area.width / 2.0, area.y + area.height / 2.0);
        let caixa = Rect { x: mx - bw - 2.0, y: my - bh - 2.0, width: 2.0 * bw + 4.0, height: 2.0 * bh + 4.0 };
        let mut k = Chave::new(8);
        k.r(area).f(scale).f(rotate).f(opacity);
        let mut c = Camada { caixa, chave: k.0.finish(), escala: scale, rotacao: rotate, alfa: opacity * self.opacidade, canto: (0, 0), centro: (0.0, 0.0), fora: None };
        if self.gravando {
            self.camadas.push(c);
            return;
        }
        let (s, l) = (self.scale, self.local(area));
        let (x0, y0) = ((l.x * s).floor() as i32, (l.y * s).floor() as i32);
        let (x1, y1) = (((l.x + l.width) * s).ceil() as i32, ((l.y + l.height) * s).ceil() as i32);
        c.centro = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
        // Só a parte da camada que, girada e escalada, cai no pixmap de fora: o resto nunca aparece.
        let (cx, cy) = c.centro;
        let volta = Transform::from_translate(cx, cy).pre_rotate(rotate).pre_scale(scale, scale).pre_translate(-cx, -cy).invert();
        let vista = volta.and_then(|v| tiny_skia::Rect::from_xywh(0.0, 0.0, self.pixmap.width() as f32, self.pixmap.height() as f32)?.transform(v));
        let (x0, y0, x1, y1) = vista.map_or((0, 0, 0, 0), |b| (x0.max(b.left().floor() as i32 - 1), y0.max(b.top().floor() as i32 - 1), x1.min(b.right().ceil() as i32 + 1), y1.min(b.bottom().ceil() as i32 + 1)));
        let (w, h) = ((x1 - x0).max(1), (y1 - y0).max(1));
        // Nada à mostra, ou grande demais ainda (algo enorme encolhido): pinta num 1x1 que não volta.
        let novo = if x1 > x0 && y1 > y0 && w as u64 * h as u64 <= 8192 * 8192 { Pixmap::new(w as u32, h as u32) } else { None };
        let novo = match novo {
            Some(p) => p,
            None => {
                c.alfa = 0.0;
                Pixmap::new(1, 1).expect("1x1")
            }
        };
        c.canto = (x0, y0);
        c.fora = Some(Fora {
            pixmap: std::mem::replace(&mut self.pixmap, novo),
            origem: self.origem,
            desloc: self.desloc,
            clips: std::mem::take(&mut self.clips),
            opacidade: self.opacidade,
            opacidades: std::mem::take(&mut self.opacidades),
        });
        self.origem = Point::new(self.origem.x + x0 as f32 / s, self.origem.y + y0 as f32 / s);
        self.desloc = (self.desloc.0 + x0, self.desloc.1 + y0);
        self.opacidade = 1.0;
        self.camadas.push(c);
    }

    pub fn pop_layer(&mut self) {
        let Some(c) = self.camadas.pop() else { return };
        let Some(f) = c.fora else { return };
        let camada = std::mem::replace(&mut self.pixmap, f.pixmap);
        self.origem = f.origem;
        self.desloc = f.desloc;
        self.clips = f.clips;
        self.opacidade = f.opacidade;
        self.opacidades = f.opacidades;
        if c.alfa <= 0.0 {
            return;
        }
        let (cx, cy) = c.centro;
        let t = Transform::from_translate(cx, cy).pre_rotate(c.rotacao).pre_scale(c.escala, c.escala).pre_translate(c.canto.0 as f32 - cx, c.canto.1 as f32 - cy);
        let tinta = PixmapPaint { opacity: c.alfa.min(1.0), quality: FilterQuality::Bilinear, ..Default::default() };
        let (pw, ph, s) = (self.pixmap.width(), self.pixmap.height(), self.scale);
        let mask = clip_mask(&mut self.clips, pw, ph, s, None);
        self.pixmap.draw_pixmap(0, 0, camada.as_ref(), &tinta, t, mask);
    }

    fn alfa(&self) -> u32 {
        (self.opacidade * 255.0).round() as u32
    }

    fn esmaecer(&self, c: Color) -> Color {
        c.alpha(c.a * self.opacidade)
    }

    /// Fecha a gravação e compara com o que está no `pixmap`.
    /// shortcut: o `pixmap` mexido direto (num canvas) não entra na comparação; use os métodos de desenho.
    pub fn changes(&mut self) -> Changes {
        self.gravando = false;
        self.clips.clear();
        self.camadas.clear();
        let novo = std::mem::take(&mut self.lista);
        let c = comparar(&self.anterior, &novo, &self.vidros);
        self.lista = std::mem::replace(&mut self.anterior, novo);
        c
    }

    /// Gravando, anota o desenho (área lógica, já cortada pelo recorte, e a chave) e diz
    /// para não pintar.
    fn gravou(&mut self, area: Rect, mut k: Chave) -> bool {
        if !self.gravando {
            return false;
        }
        let mut area = crescer(area, 2.0);
        // Fonte nova muda as letras de todas as janelas, sem mudar o que elas desenham.
        k.f(self.opacidade).h(self.fontes_carregadas);
        // Dentro de uma camada o desenho aparece transformado: vale a área dela toda.
        if let Some(c) = self.camadas.first() {
            area = c.caixa;
        }
        for c in &self.camadas {
            k.h(c.chave);
        }
        for c in &self.clips {
            k.r(c.own).h(c.radius.map(f32::to_bits));
        }
        if let Some(c) = self.clips.last() {
            area = area.intersect(&c.rect);
        }
        if area.width > 0.0 && area.height > 0.0 {
            self.lista.push((area, k.0.finish()));
        }
        true
    }

    fn local(&self, r: Rect) -> Rect {
        Rect { x: r.x - self.origem.x, y: r.y - self.origem.y, ..r }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    fn buffer(&mut self, text: &str, size: f32, font: Font, max_width: Option<f32>, lh: f32) -> &Buffer {
        let s = self.scale;
        let key = TextKey { text: text.to_owned(), size: (size * s * 64.0) as u32, font, width: max_width.map(|w| (w * s) as u32), line_height: lh.to_bits() };
        self.texts.aparar(self.texts.len() > 4000);
        let fonts = &mut self.fonts;
        self.texts.pegar(key, || {
            let mut b = Buffer::new(fonts, Metrics::new(size * s, (size * lh * s).ceil()));
            b.set_size(fonts, max_width.map(|w| w * s), None);
            let attrs = attrs(font);
            b.set_text(fonts, text, attrs, Shaping::Advanced);
            b
        })
    }

    /// Tamanho lógico do texto; com `max_width` quebra em linhas.
    pub fn measure(&mut self, text: &str, size: f32, font: Font, max_width: Option<f32>) -> Size {
        self.measure_lh(text, size, font, max_width, ALTURA_LINHA)
    }

    /// [`measure`](Self::measure) com a altura da linha em múltiplos do tamanho (o padrão é 1.3).
    pub fn measure_lh(&mut self, text: &str, size: f32, font: Font, max_width: Option<f32>, lh: f32) -> Size {
        let s = self.scale;
        let b = self.buffer(text, size, font, max_width, lh);
        let (mut w, mut h) = (0.0f32, 0.0f32);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            h = run.line_top + run.line_height;
        }
        Size::new((w / s).ceil(), (h / s).ceil())
    }

    /// Início e fim (x lógico) de cada caractere da primeira linha, com o índice em bytes: o cursor do campo de texto.
    pub fn carets(&mut self, text: &str, size: f32, font: Font) -> Vec<(usize, f32, f32)> {
        let s = self.scale;
        let b = self.buffer(text, size, font, None, ALTURA_LINHA);
        b.layout_runs().take(1).flat_map(|r| r.glyphs.iter().map(|g| (g.start, g.x / s, (g.x + g.w) / s)).collect::<Vec<_>>()).collect()
    }

    pub fn draw_text(&mut self, text: &str, size: f32, font: Font, color: Color, at: Point, max_width: Option<f32>) {
        self.draw_text_lh(text, size, font, color, at, max_width, ALTURA_LINHA)
    }

    /// [`draw_text`](Self::draw_text) com a altura da linha em múltiplos do tamanho.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_lh(&mut self, text: &str, size: f32, font: Font, color: Color, at: Point, max_width: Option<f32>, lh: f32) {
        if self.gravando {
            let m = self.measure_lh(text, size, font, max_width, lh);
            let mut k = Chave::new(1);
            k.h(text).f(size).h(font).c(color).f(at.x).f(at.y).h(max_width.map(f32::to_bits)).f(lh);
            self.gravou(crescer(Rect::new(at, m), sobra_glifos(size, lh)), k);
            return;
        }
        self.buffer(text, size, font, max_width, lh);
        let key = TextKey { text: text.to_owned(), size: (size * self.scale * 64.0) as u32, font, width: max_width.map(|w| (w * self.scale) as u32), line_height: lh.to_bits() };
        let b = self.texts.get(&key).expect("criado pelo buffer");
        let at = Point::new(at.x - self.origem.x, at.y - self.origem.y);
        let color = color.alpha(color.a * self.opacidade);
        if self.pintando_gpu {
            let rec = self.clips.last().map_or(gpu::SEM_RECORTE, |c| c.gpu);
            if !gpu::glifos(&mut self.gpu, b, &mut self.fonts, &mut self.swash, &rec, self.scale, at, color, self.opacidade) {
                self.falta("atlas de glifos cheio");
            }
            return;
        }
        pintar_glifos(b, &mut self.fonts, &mut self.swash, &mut self.pixmap, &mut self.clips, self.scale, at, color, self.opacidade);
    }

    /// Um `Buffer` montado por `montar(fontes, escala)` e guardado pela `chave` (que já conta
    /// o conteúdo e a largura; a escala entra aqui).
    pub(crate) fn proprio(&mut self, chave: u64, montar: impl FnOnce(&mut FontSystem, f32) -> Buffer) -> &Buffer {
        self.proprios.aparar(self.proprios.len() > 2000);
        let (fonts, s) = (&mut self.fonts, self.scale);
        self.proprios.pegar(chave ^ s.to_bits() as u64, || montar(fonts, s))
    }

    /// Desenha o buffer guardado com [`Renderer::proprio`] (montado antes neste quadro).
    pub(crate) fn draw_proprio(&mut self, chave: u64, at: Point, color: Color, area: Rect) {
        let mut k = Chave::new(8);
        k.h(chave).f(at.x).f(at.y).c(color);
        if self.gravou(area, k) {
            return;
        }
        let at = Point::new(at.x - self.origem.x, at.y - self.origem.y);
        let color = self.esmaecer(color);
        let Some(b) = self.proprios.get(&(chave ^ self.scale.to_bits() as u64)) else { return };
        if self.pintando_gpu {
            let rec = self.clips.last().map_or(gpu::SEM_RECORTE, |c| c.gpu);
            if !gpu::glifos(&mut self.gpu, b, &mut self.fonts, &mut self.swash, &rec, self.scale, at, color, self.opacidade) {
                self.falta("atlas de glifos cheio");
            }
            return;
        }
        pintar_glifos(b, &mut self.fonts, &mut self.swash, &mut self.pixmap, &mut self.clips, self.scale, at, color, self.opacidade);
    }

    /// Põe no banco as fontes de [`load_font`] que ainda não estão; com fonte nova, os textos
    /// medidos com a antiga (que caiu numa substituta) saem do cache.
    fn carregar_fontes(&mut self) {
        let extra = FONTES_EXTRA.lock().unwrap_or_else(|e| e.into_inner());
        if extra.len() == self.fontes_carregadas {
            return;
        }
        for f in &extra[self.fontes_carregadas..] {
            self.fonts.db_mut().load_font_source(cosmic_text::fontdb::Source::Binary(f.clone()));
        }
        self.fontes_carregadas = extra.len();
        self.texts.clear();
        self.proprios.clear();
    }

    /// As fontes, para quem monta o próprio `Buffer` (o editor de texto).
    pub(crate) fn fonts(&mut self) -> &mut FontSystem {
        &mut self.fonts
    }

    /// Os glifos de um `Buffer` já montado (em pixels físicos), a partir de `at` lógico.
    /// Gravando, anota `area` com a impressão digital `chave`, que o dono do buffer calcula
    /// (o conteúdo dele não está aqui para entrar no hash).
    pub(crate) fn draw_buffer(&mut self, b: &Buffer, at: Point, color: Color, area: Rect, chave: u64) {
        let mut k = Chave::new(7);
        k.h(chave).f(at.x).f(at.y).c(color);
        if self.gravou(area, k) {
            return;
        }
        let at = Point::new(at.x - self.origem.x, at.y - self.origem.y);
        let color = self.esmaecer(color);
        if self.pintando_gpu {
            let rec = self.clips.last().map_or(gpu::SEM_RECORTE, |c| c.gpu);
            if !gpu::glifos(&mut self.gpu, b, &mut self.fonts, &mut self.swash, &rec, self.scale, at, color, self.opacidade) {
                self.falta("atlas de glifos cheio");
            }
            return;
        }
        pintar_glifos(b, &mut self.fonts, &mut self.swash, &mut self.pixmap, &mut self.clips, self.scale, at, color, self.opacidade);
    }
}

#[allow(clippy::too_many_arguments)]
/// `color` já vem esmaecida; `opac` esmaece os glifos de cor própria (`color_opt`).
fn pintar_glifos(b: &Buffer, fonts: &mut FontSystem, swash: &mut SwashCache, pixmap: &mut Pixmap, clips: &mut [Clip], s: f32, at: Point, color: Color, opac: f32) {
    let (ox, oy) = ((at.x * s).round() as i32, (at.y * s).round() as i32);
    let clip = clips.last().map(|c| c.rect);
    let cor = cosmic_text::Color::rgba((color.r * 255.0) as u8, (color.g * 255.0) as u8, (color.b * 255.0) as u8, (color.a * 255.0) as u8);
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let (cx0, cy0, cx1, cy1) = match clip {
        Some(r) => ((r.x * s).floor() as i32, (r.y * s).floor() as i32, ((r.x + r.width) * s).ceil() as i32, ((r.y + r.height) * s).ceil() as i32),
        None => (0, 0, w, h),
    };
    let (pw, ph) = (pixmap.width(), pixmap.height());
    let precisa = clips.last().is_some_and(|c| c.rounded);
    let mask = if precisa { clip_mask(clips, pw, ph, s, None).map(|m| m.data()) } else { None };
    let px = pixmap.pixels_mut();
    // O laço do `Buffer::draw`, mas pulando glifos de cor transparente (o Markdown esconde
    // sinais assim) e linhas fora do recorte.
    for run in b.layout_runs() {
        let topo = oy + run.line_top as i32;
        if topo > cy1 || topo + run.line_height.ceil() as i32 + 1 < cy0 {
            continue;
        }
        for g in run.glyphs {
            let cor_g = g.color_opt.unwrap_or(cor);
            // O swash devolve só a cobertura no alfa: o alfa da cor (e a opacidade) entra aqui.
            let fator = if g.color_opt.is_some() { (cor_g.a() as f32 * opac).round() as u32 } else { cor_g.a() as u32 };
            if fator == 0 {
                continue;
            }
            let fis = g.physical((0., 0.), 1.0);
            swash.with_pixels(fonts, fis.cache_key, cor_g, |x, y, c| {
                let a = c.a() as u32 * fator / 255;
                let (xx, yy) = (ox + fis.x + x, oy + run.line_y as i32 + fis.y + y);
                if a == 0 || xx < cx0.max(0) || xx >= cx1.min(w) || yy < cy0.max(0) || yy >= cy1.min(h) {
                    return;
                }
                let i = (yy * w + xx) as usize;
                let a = match mask {
                    Some(m) => a * m[i] as u32 / 255,
                    None => a,
                };
                blend(&mut px[i], c.r(), c.g(), c.b(), a);
            });
        }
    }
}

impl Renderer {
    /// Retângulo com fundo, borda e sombra; o raio vale para os três.
    pub fn fill_quad(&mut self, r: Rect, bg: Option<&Background>, border: Border, shadow: Option<Shadow>) {
        if matches!(bg, Some(Background::Linear { stops, .. }) if stops.len() > 4) {
            self.falta("degradê com mais de 4 paradas");
        }
        if self.gravando {
            let mut k = Chave::new(2);
            k.r(r).borda(border);
            match bg {
                None => k.h(0u8),
                Some(Background::Color(c)) => k.h(1u8).c(*c),
                Some(Background::Linear { angle, stops }) => {
                    k.h(2u8).f(*angle);
                    for (t, c) in stops.iter() {
                        k.f(*t).c(*c);
                    }
                    &mut k
                }
            };
            let mut area = r;
            if let Some(sh) = shadow.filter(|sh| sh.color.a > 0.0) {
                k.c(sh.color).f(sh.offset.x).f(sh.offset.y).f(sh.blur);
                area = uniao(area, crescer(Rect { x: r.x + sh.offset.x, y: r.y + sh.offset.y, ..r }, sh.blur));
            }
            self.gravou(area, k);
            return;
        }
        if self.pintando_gpu {
            return self.quad_gpu(r, bg, border, shadow);
        }
        let r = self.local(r);
        let s = self.scale;
        let esmaecido;
        let (bg, border) = if self.opacidade < 1.0 {
            esmaecido = bg.map(|b| b.faded(self.opacidade));
            (esmaecido.as_ref(), Border { color: self.esmaecer(border.color), ..border })
        } else {
            (bg, border)
        };
        if let Some(sh) = shadow.filter(|sh| sh.color.a > 0.0) {
            self.shadow(r, border.radius, sh);
        }
        let Some(path) = rounded_path(r, border.radius, s) else { return };
        if let Some(bg) = bg.filter(|_| self.preencher(r, bg, border.radius)) {
            let p = match bg {
                Background::Color(c) => paint(*c),
                Background::Linear { angle, stops } => {
                    let (cx, cy, dx, dy) = ((r.x + r.width / 2.0) * s, (r.y + r.height / 2.0) * s, angle.cos(), angle.sin());
                    let meio = (r.width * dx.abs() + r.height * dy.abs()) / 2.0 * s;
                    let paradas = stops.iter().map(|(t, c)| GradientStop::new(*t, skia_color(*c))).collect();
                    let shader = LinearGradient::new((cx - dx * meio, cy - dy * meio).into(), (cx + dx * meio, cy + dy * meio).into(), paradas, SpreadMode::Pad, Transform::identity());
                    match shader {
                        Some(shader) => Paint { shader, anti_alias: true, ..Default::default() },
                        None => paint(stops.first().map(|s| s.1).unwrap_or_default()),
                    }
                }
            };
            self.preencher_caminho(&path, &p);
        }
        if border.width > 0.0 && border.color.a > 0.0 {
            // A borda fica do lado de dentro: o anel entre o contorno e o contorno encolhido.
            let w = border.width;
            let dentro = Rect { x: r.x + w, y: r.y + w, width: r.width - 2.0 * w, height: r.height - 2.0 * w };
            let mut pb = PathBuilder::new();
            push_rounded_path(&mut pb, r, border.radius, s);
            push_rounded_path(&mut pb, dentro, border.radius.map(|v| (v - w).max(0.0)), s);
            if let Some(anel) = pb.finish() {
                let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
                let mask = clip_mask(&mut self.clips, pw, ph, s, Some(anel.bounds()));
                self.pixmap.fill_path(&anel, &paint(border.color), FillRule::EvenOdd, Transform::identity(), mask);
            }
        }
    }

    /// Fundo liso ou degradê na horizontal/vertical pintado direto em inteiros (o caso comum,
    /// e o tiny-skia é lento nele em área grande). `false` se já pintou; senão fica pro tiny-skia.
    fn preencher(&mut self, r: Rect, bg: Option<&Background>, cantos: [f32; 4]) -> bool {
        let s = self.scale;
        let Some(f) = tiny_skia::Rect::from_xywh(r.x * s, r.y * s, r.width * s, r.height * s) else { return false };
        let pre = |c: Color| {
            let a = c.a.clamp(0.0, 1.0);
            [c.r, c.g, c.b].map(|v| (v.clamp(0.0, 1.0) * a * 255.0 + 0.5) as u8).into_iter().chain([(a * 255.0 + 0.5) as u8]).collect::<Vec<_>>().try_into().unwrap_or([0; 4])
        };
        // Cor por posição ao longo do eixo do degradê (uma por linha ou coluna).
        let (vertical, cores): (bool, Vec<[u8; 4]>) = match bg {
            None => return true,
            Some(Background::Color(c)) => (true, vec![pre(*c)]),
            Some(Background::Linear { angle, stops }) if !stops.is_empty() => {
                let (dx, dy) = (angle.cos(), angle.sin());
                let vertical = dx.abs() < 1e-4;
                if !vertical && dy.abs() >= 1e-4 {
                    return true;
                }
                let n = if vertical { f.height() } else { f.width() }.ceil().max(1.0) as usize;
                let inverte = if vertical { dy < 0.0 } else { dx < 0.0 };
                let cor = |t: f32| {
                    let t = if inverte { 1.0 - t } else { t };
                    let i = stops.iter().position(|(p, _)| *p >= t).unwrap_or(stops.len());
                    match i {
                        0 => stops[0].1,
                        i if i == stops.len() => stops[i - 1].1,
                        i => {
                            let ((p0, c0), (p1, c1)) = (stops[i - 1], stops[i]);
                            let k = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
                            Color { r: c0.r + (c1.r - c0.r) * k, g: c0.g + (c1.g - c0.g) * k, b: c0.b + (c1.b - c0.b) * k, a: c0.a + (c1.a - c0.a) * k }
                        }
                    }
                };
                (vertical, (0..n).map(|i| pre(cor((i as f32 + 0.5) / n as f32))).collect())
            }
            Some(_) => return true,
        };
        let (x, y) = (f.left().floor() as i32, f.top().floor() as i32);
        let tam = ((f.right().ceil() as i32 - x), (f.bottom().ceil() as i32 - y));
        let ultima = cores.len() - 1;
        let amostra = |px: usize, py: usize| cores[if vertical { py } else { px }.min(ultima)];
        compor(&mut self.pixmap, &mut self.clips, s, x, y, tam, Some((f, cantos.map(|v| v * s))), false, false, self.parcial.is_none(), 255, vertical, amostra);
        false
    }

    fn preencher_caminho(&mut self, p: &tiny_skia::Path, paint: &Paint) {
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, self.scale, Some(p.bounds()));
        self.pixmap.fill_path(p, paint, FillRule::Winding, Transform::identity(), mask);
    }

    /// Sombra desfocada de verdade, guardada em cache pelo tamanho, raio, desfoque e cor.
    fn shadow(&mut self, r: Rect, radius: [f32; 4], sh: Shadow) {
        let s = self.scale;
        let m = (sh.blur * s).ceil();
        let (bw, bh) = ((r.width * s).ceil(), (r.height * s).ceil());
        if bw < 1.0 || bh < 1.0 {
            return;
        }
        // Caixa grande: basta a sombra de uma caixa pequena com os mesmos cantos, esticada
        // pelo meio (nine-slice); o miolo é constante. Assim o cache não depende do tamanho.
        let mr = radius.iter().fold(0.0f32, |a, &v| a.max(v * s)).ceil();
        let lado = 2.0 * (mr + m) + 2.0;
        let nove = bw >= lado && bh >= lado;
        let (bw, bh) = if nove { (lado, lado) } else { (bw, bh) };
        let (w, h) = (bw + 2.0 * m, bh + 2.0 * m);
        let cor = skia_color(sh.color).to_color_u8();
        let key = ShadowKey { w: w as u32, h: h as u32, radius: radius.map(|v| (v * s) as u32), blur: m as u32, color: [cor.red(), cor.green(), cor.blue(), cor.alpha()] };
        self.shadows.aparar(self.shadows.len() > 256 || self.shadows.bytes() > 64 << 20);
        let pm = self.shadows.pegar(key, || {
            let mut pm = Pixmap::new(w as u32, h as u32).expect("w, h >= 1");
            let caixa = Rect { x: m / s, y: m / s, width: bw / s, height: bh / s };
            if let Some(p) = rounded_path(caixa, radius, s) {
                pm.fill_path(&p, &paint(sh.color), FillRule::Winding, Transform::identity(), None);
            }
            if m < 1.0 {
                return pm;
            }
            let px: Vec<[u32; 4]> = pm.pixels().iter().map(|p| [p.red() as u32, p.green() as u32, p.blue() as u32, p.alpha() as u32]).collect();
            let (pq, d) = desfocar(px, w as usize, h as usize, (m / 3.0).round().max(1.0) as usize);
            if d > 1.0 { escalar(&pq, w as u32, h as u32, None) } else { pq }
        });
        let (x, y) = (((r.x + sh.offset.x) * s - m).round() as i32, ((r.y + sh.offset.y) * s - m).round() as i32);
        let caixa = tiny_skia::Rect::from_xywh(r.x * s, r.y * s, r.width * s, r.height * s).map(|f| (f, radius.map(|v| v * s)));
        let fatia = |alvo: usize, n: usize| -> Vec<usize> {
            let c = n / 2;
            (0..alvo).map(|i| if i < c { i } else if i >= alvo - c { i + n - alvo } else { c }).collect()
        };
        let (xs, ys);
        let mapa = if nove {
            xs = fatia((r.width * s).ceil() as usize + 2 * m as usize, w as usize);
            ys = fatia((r.height * s).ceil() as usize + 2 * m as usize, h as usize);
            Some((&xs[..], &ys[..]))
        } else {
            None
        };
        let (tam, amostra) = de(pm, mapa);
        // A sombra fica no cache com a cor cheia: a opacidade entra só na hora de compor.
        let alfa = (self.opacidade * 255.0).round() as u32;
        compor(&mut self.pixmap, &mut self.clips, s, x, y, tam, caixa, true, false, self.parcial.is_none(), alfa, false, amostra);
    }

    /// Imagem esticada em `r`, já recortada pelos cantos `radius`.
    pub fn draw_image(&mut self, img: &Image, r: Rect, radius: [f32; 4]) {
        // shortcut: transformada, a imagem perde os cantos arredondados; recortar quando um slide pedir.
        if !self.transformacoes.last().is_none_or(|t| t.is_identity()) {
            return self.imagem_transformada(img, r, FilterQuality::Bilinear);
        }
        if img.0.width().max(img.0.height()) > self.gpu.max_textura {
            self.falta("imagem maior que a textura");
        }
        let mut k = Chave::new(3);
        k.h(img.1).r(r).h(radius.map(f32::to_bits));
        if self.gravou(r, k) {
            return;
        }
        if self.pintando_gpu {
            let s = self.scale;
            let (x, y, w, h) = ((r.x * s).round(), (r.y * s).round(), (r.width * s).round(), (r.height * s).round());
            return self.imagem_gpu(img.1, &img.0, [x, y, w, h], radius.map(|v| v * s));
        }
        let tela = r;
        let r = self.local(r);
        let s = self.scale;
        let (x, y, w, h) = ((r.x * s).round() as i32, (r.y * s).round() as i32, (r.width * s).round() as u32, (r.height * s).round() as u32);
        if w == 0 || h == 0 {
            return;
        }
        if w as u64 * h as u64 > 1 << 24 {
            // Esticada além de 4096² (zoom): o tiny-skia pinta só a parte à mostra, sem cópia.
            let cantos = radius.iter().any(|v| *v > 0.0);
            if cantos {
                self.push_clip(tela, radius);
            }
            self.imagem_transformada(img, tela, FilterQuality::Bilinear);
            if cantos {
                self.pop_clip();
            }
            return;
        }
        self.imagens.aparar(self.imagens.len() > 32 || self.imagens.bytes() > 32 << 20);
        let pm: &Pixmap = if (w, h) == (img.0.width(), img.0.height()) { &img.0 } else { self.imagens.pegar((img.1, w, h), || escalar(&img.0, w, h, None)) };
        let forma = tiny_skia::Rect::from_xywh(x as f32, y as f32, w as f32, h as f32).map(|f| (f, radius.map(|v| v * s)));
        let (tam, amostra) = de(pm, None);
        let alfa = (self.opacidade * 255.0).round() as u32;
        compor(&mut self.pixmap, &mut self.clips, s, x, y, tam, forma, false, false, self.parcial.is_none(), alfa, false, amostra);
    }

    /// Imagem sem suavizar: cada pixel dela vira um bloco (página de PDF 1:1, pixel art, zoom).
    pub fn draw_image_nearest(&mut self, img: &Image, r: Rect) {
        self.imagem_transformada(img, r, FilterQuality::Nearest);
    }

    /// A imagem esticada em `r` pela transformação inteira (a do usuário, a escala, a camada).
    fn imagem_transformada(&mut self, img: &Image, r: Rect, filtro: FilterQuality) {
        self.falta("imagem transformada ou sem suavizar");
        if self.gravando {
            let mut k = Chave::new(12);
            k.h(img.1).r(r).h(matches!(filtro, FilterQuality::Nearest));
            let t = self.transformacoes.last().copied().unwrap_or_default();
            let tt = t.0;
            k.f(tt.sx).f(tt.ky).f(tt.kx).f(tt.sy).f(tt.tx).f(tt.ty);
            let area = vetor::Path::rect(r).and_then(|p| p.transform(t)).map_or(r, |p| p.bounds());
            self.gravou(area, k);
            return;
        }
        let (iw, ih) = (img.0.width() as f32, img.0.height() as f32);
        let s = self.scale;
        let usuario = self.transformacoes.last().copied().unwrap_or_default().0;
        let t = Transform::from_row(r.width / iw, 0.0, 0.0, r.height / ih, r.x, r.y).post_concat(usuario).post_translate(-self.origem.x, -self.origem.y).post_scale(s, s);
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let caixa = tiny_skia::Rect::from_xywh(0.0, 0.0, iw, ih).and_then(|c| c.transform(t));
        let mask = clip_mask(&mut self.clips, pw, ph, s, caixa);
        let pp = PixmapPaint { opacity: self.opacidade, quality: filtro, ..Default::default() };
        self.pixmap.draw_pixmap(0, 0, img.0.as_ref().as_ref(), &pp, t, mask);
    }

    /// SVG em `r`; com `color`, troca a cor de tudo mantendo o alfa (ícones de um traço só).
    pub fn draw_svg(&mut self, svg: &Svg, r: Rect, color: Option<Color>) {
        let s = self.scale;
        if (r.width * s).round() as u64 * (r.height * s).round() as u64 > 1 << 22 {
            self.falta("svg maior que 2048²");
        }
        let mut k = Chave::new(4);
        k.h(svg.1).r(r).h(color.map(|c| [c.r, c.g, c.b, c.a].map(f32::to_bits)));
        if self.gravou(r, k) {
            return;
        }
        let r = self.local(r);
        let (w, h) = ((r.width * s).round() as u32, (r.height * s).round() as u32);
        if w == 0 || h == 0 {
            return;
        }
        let cor = color.map(|c| skia_color(c).to_color_u8()).map(|c| [c.red(), c.green(), c.blue(), c.alpha()]);
        let t = svg.0.size();
        let escala = Transform::from_scale(w as f32 / t.width(), h as f32 / t.height());
        let rasterizar = |pw: u32, ph: u32, t: Transform| {
            let mut pm = Pixmap::new(pw, ph)?;
            resvg::render(&svg.0, t, &mut pm.as_mut());
            if let Some([cr, cg, cb, ca]) = cor {
                for p in pm.pixels_mut() {
                    let a = p.alpha() as u32 * ca as u32 / 255;
                    let pre = |c: u8| (c as u32 * a / 255) as u8;
                    *p = tiny_skia::PremultipliedColorU8::from_rgba(pre(cr), pre(cg), pre(cb), a as u8).expect("pré-multiplicado");
                }
            }
            Some(pm)
        };
        let (x, y) = ((r.x * s).round() as i32, (r.y * s).round() as i32);
        let parte;
        let (pm, x, y) = if w as u64 * h as u64 <= 1 << 22 {
            let key = SvgKey { tree: svg.1, w, h, color: cor };
            // A mesma chave da textura: o bit de cima não deixa colidir com o número de uma imagem.
            let id = { let mut hs = std::collections::hash_map::DefaultHasher::new(); key.hash(&mut hs); hs.finish() | 1 << 63 };
            self.svgs.aparar(self.svgs.len() > 512 || self.svgs.bytes() > 64 << 20);
            let pm = self.svgs.pegar(key, || Arc::new(rasterizar(w, h, escala).expect("tamanho > 0")));
            if self.pintando_gpu {
                let pm = pm.clone();
                return self.imagem_gpu(id, &pm, [x as f32, y as f32, w as f32, h as f32], [0.0; 4]);
            }
            (&**pm, x, y)
        } else {
            // Maior que 2048² (zoom): só a parte que cai no pixmap, sem cache.
            let (x0, y0) = (x.max(0), y.max(0));
            let x1 = (x as i64 + w as i64).min(self.pixmap.width() as i64) as i32;
            let y1 = (y as i64 + h as i64).min(self.pixmap.height() as i64) as i32;
            if x1 <= x0 || y1 <= y0 || self.pintando_gpu {
                return;
            }
            let Some(p) = rasterizar((x1 - x0) as u32, (y1 - y0) as u32, escala.post_translate((x - x0) as f32, (y - y0) as f32)) else { return };
            parte = p;
            (&parte, x0, y0)
        };
        let (w, h) = (pm.width(), pm.height());
        let area = tiny_skia::Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, s, area);
        let tinta = PixmapPaint { opacity: self.opacidade, ..Default::default() };
        self.pixmap.draw_pixmap(x, y, pm.as_ref(), &tinta, Transform::identity(), mask);
    }

    /// Linha passando pelos `pontos`, com pontas e junções redondas.
    pub fn stroke(&mut self, pontos: &[Point], largura: f32, cor: Color) {
        if cor.a * self.opacidade < 1.0 && pontos.len() > 2 {
            self.falta("traço translúcido");
        }
        if self.gravando {
            let mut k = Chave::new(5);
            k.f(largura).c(cor);
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in pontos {
                k.f(p.x).f(p.y);
                (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
            }
            if !pontos.is_empty() {
                self.gravou(crescer(Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }, largura / 2.0), k);
            }
            return;
        }
        if self.pintando_gpu {
            let cor = self.esmaecer(cor);
            return self.traco_gpu(pontos.windows(2).map(|w| (w[0], w[1])), largura, cor);
        }
        let s = self.scale;
        let mut pb = PathBuilder::new();
        for (i, p) in pontos.iter().enumerate() {
            let p = &Point::new(p.x - self.origem.x, p.y - self.origem.y);
            if i == 0 {
                pb.move_to(p.x * s, p.y * s);
            } else {
                pb.line_to(p.x * s, p.y * s);
            }
        }
        let Some(path) = pb.finish() else { return };
        let traco = tiny_skia::Stroke { width: largura * s, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        let area = path.bounds();
        let meia = traco.width / 2.0;
        let area = tiny_skia::Rect::from_ltrb(area.left() - meia, area.top() - meia, area.right() + meia, area.bottom() + meia);
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, s, area);
        let cor = cor.alpha(cor.a * self.opacidade);
        self.pixmap.stroke_path(&path, &paint(cor), &traco, Transform::identity(), mask);
    }

    /// Desfoca o que já está desenhado em `r` (vidro), respeitando os cantos.
    pub fn blur(&mut self, r: Rect, raio: f32, cantos: [f32; 4]) {
        self.falta("vidro");
        // Num quadro parcial os vidros já vêm da gravação (e servem às outras regiões).
        if self.parcial.is_none() {
            self.vidros.push((r, raio));
        }
        let mut k = Chave::new(6);
        k.r(r).f(raio).h(cantos.map(f32::to_bits));
        if self.gravou(r, k) {
            return;
        }
        let r = self.local(r);
        let s = self.scale;
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        // Começa num múltiplo de 12 na janela inteira: o desfoque reduzido cai na mesma grade
        // no quadro cheio e no parcial (`begin_parcial`), e não aparece emenda.
        let grade = |v: f32, o: i32| (v.floor() as i32 + o).div_euclid(12) * 12 - o;
        let x0 = grade(r.x * s, self.desloc.0).clamp(0, pw);
        let y0 = grade(r.y * s, self.desloc.1).clamp(0, ph);
        let x1 = (((r.x + r.width) * s).ceil() as i32).clamp(0, pw);
        let y1 = (((r.y + r.height) * s).ceil() as i32).clamp(0, ph);
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        if w == 0 || h == 0 {
            return;
        }
        let mut px: Vec<[u32; 4]> = Vec::with_capacity(w * h);
        let src = self.pixmap.pixels();
        for y in y0..y1 {
            for x in x0..x1 {
                let p = src[(y * pw + x) as usize];
                px.push([p.red() as u32, p.green() as u32, p.blue() as u32, p.alpha() as u32]);
            }
        }
        let (pq, d) = desfocar(px, w, h, ((raio * s) / 3.0).round().max(1.0) as usize);
        let pm = if d > 1.0 { escalar(&pq, w as u32, h as u32, Some(d as i64)) } else { pq };
        let forma = tiny_skia::Rect::from_xywh(r.x * s, r.y * s, r.width * s, r.height * s).map(|f| (f, cantos.map(|v| v * s)));
        let (tam, amostra) = de(&pm, None);
        let alfa = self.alfa();
        compor(&mut self.pixmap, &mut self.clips, s, x0, y0, tam, forma, false, true, self.parcial.is_none(), alfa, false, amostra);
    }

    /// Daqui até o `pop_clip`, nada sai de `r` (nem dos cantos arredondados).
    pub fn push_clip(&mut self, r: Rect, radius: [f32; 4]) {
        let r = self.local(r);
        let rect = self.clips.last().map_or(r, |c| c.rect.intersect(&r));
        let toca = |q: &Rect| q.x < rect.x + rect.width && rect.x < q.x + q.width && q.y < rect.y + rect.height && rect.y < q.y + q.height;
        // A célula de uma tabela num painel arredondado fica longe dos cantos: não herda a máscara.
        let mut cantos: Vec<Canto> = self.clips.last().map_or(Vec::new(), |c| c.cantos.iter().filter(|q| toca(&q.quadrado)).copied().collect());
        let max = (r.width.min(r.height) / 2.0).max(0.0);
        let [a, b, c, d] = radius.map(|v| v.clamp(0.0, max));
        let fim = (r.x + r.width, r.y + r.height);
        for (x, y, k, cx, cy) in [(r.x, r.y, a, r.x + a, r.y + a), (fim.0 - b, r.y, b, fim.0 - b, r.y + b), (fim.0 - c, fim.1 - c, c, fim.0 - c, fim.1 - c), (r.x, fim.1 - d, d, r.x + d, fim.1 - d)] {
            if k > 0.0 {
                cantos.push(Canto { quadrado: Rect { x, y, width: k, height: k }, centro: Point::new(cx, cy), raio: k });
            }
        }
        let gpu = gpu::recorte(self.clips.last().map_or(&gpu::SEM_RECORTE, |c| &c.gpu), rect, r, radius, self.scale);
        self.clips.push(Clip { rect, own: r, radius, rounded: !cantos.is_empty(), cantos, mask: None, gpu });
    }

    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// `r` (lógico, com `folga` em volta para sombras) cai todo fora do recorte atual: quem
    /// desenha pode pular. Com camada ou transformação aberta, nunca (a área não bate).
    pub fn clipped_out(&self, r: Rect, folga: f32) -> bool {
        if !self.camadas.is_empty() || !self.transformacoes.is_empty() {
            return false;
        }
        let Some(c) = self.clips.last() else { return false };
        let r = self.local(r);
        let c = c.rect;
        r.x - folga >= c.x + c.width || r.y - folga >= c.y + c.height || r.x + r.width + folga <= c.x || r.y + r.height + folga <= c.y
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

/// Média móvel de largura `2k+1` numa direção, com a borda repetida.
/// Um cache que lembra em que geração (quadro) cada item foi usado; `virar` (a cada quadro
/// cheio, ou quando passa do limite) esquece o que ficou sem uso nela.
struct Cache<K, V> {
    mapa: HashMap<K, (V, u64)>,
    geracao: u64,
    /// Quantos sobraram na última virada.
    vivos: usize,
}

impl<K, V> Default for Cache<K, V> {
    fn default() -> Self {
        Cache { mapa: HashMap::new(), geracao: 0, vivos: 0 }
    }
}

impl<K: Hash + Eq, V> Cache<K, V> {
    fn pegar(&mut self, k: K, novo: impl FnOnce() -> V) -> &mut V {
        let g = self.geracao;
        let e = self.mapa.entry(k).or_insert_with(|| (novo(), g));
        e.1 = g;
        &mut e.0
    }
    fn get(&self, k: &K) -> Option<&V> {
        self.mapa.get(k).map(|e| &e.0)
    }
    fn len(&self) -> usize {
        self.mapa.len()
    }
    fn clear(&mut self) {
        self.mapa.clear();
    }
    /// Fora o que ficou `manter` gerações sem uso (2 trocas por quadro: 8 = 4 quadros, com folga
    /// para outra janela desenhando no meio; 0 = só o quadro atual): a RAM não sobe e desce.
    fn esquecer_velhos(&mut self, manter: u64) {
        let g = self.geracao;
        self.mapa.retain(|_, e| e.1 + manter >= g);
    }
    fn virar(&mut self) {
        let g = self.geracao;
        self.mapa.retain(|_, e| e.1 == g);
        self.geracao += 1;
        self.vivos = self.mapa.len();
    }
    /// `cheio`: vira no meio do quadro (os quadros parciais não viram no `begin`). Só de novo
    /// quando dobrar: o que o quadro está usando fica, mesmo passando do limite.
    fn aparar(&mut self, cheio: bool) {
        if cheio && self.mapa.len() > 2 * self.vivos {
            self.virar();
        }
    }
}

impl<K, P: Borrow<Pixmap>> Cache<K, P> {
    fn bytes(&self) -> usize {
        self.mapa.values().map(|e| e.0.borrow().data().len()).sum()
    }
}

use std::borrow::Borrow;

/// O glibc guarda a memória dos pixmaps soltos para reusar, e o processo segue com ela:
/// depois de largar vários MB, devolve ao sistema.
fn devolver_memoria() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        // SAFETY: função do glibc sem pré-condições.
        unsafe { malloc_trim(0) };
    }
}

/// Bilinear em inteiros (8 bits de fração): `src` esticado para `w`×`h`.
/// Com `razao`, cada pixel de origem vale exatamente `razao` de destino (não depende de `w`, `h`).
fn escalar(src: &Pixmap, w: u32, h: u32, razao: Option<i64>) -> Pixmap {
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    // Para cada coordenada de destino: os dois vizinhos na origem e o peso do segundo.
    let eixo = |n: u32, sn: i64| -> Vec<(usize, usize, u32)> {
        let (num, den) = razao.map_or((sn, n as i64), |d| (1, d));
        (0..n as i64)
            .map(|i| {
                let f = (((2 * i + 1) * num << 8) / (2 * den) - 128).clamp(0, (sn - 1) << 8);
                ((f >> 8) as usize, ((f >> 8) + 1).min(sn - 1) as usize, (f & 255) as u32)
            })
            .collect()
    };
    let (xs, ys) = (eixo(w, sw), eixo(h, sh));
    let mut out = Pixmap::new(w, h).expect("w, h > 0");
    let (sd, sw) = (src.data(), sw as usize);
    // Faixas de linhas em paralelo: no resize a foto de fundo é refeita a cada quadro.
    let linhas = (h as usize).div_ceil(std::thread::available_parallelism().map_or(1, |n| n.get()).min(4));
    std::thread::scope(|e| {
        for (i, faixa) in out.data_mut().chunks_mut(linhas * w as usize * 4).enumerate() {
            let (xs, ys) = (&xs, &ys[i * linhas..]);
            e.spawn(move || {
                for (linha, &(y0, y1, fy)) in faixa.chunks_mut(w as usize * 4).zip(ys) {
                    let (l0, l1) = (&sd[y0 * sw * 4..], &sd[y1 * sw * 4..]);
                    for (p, &(x0, x1, fx)) in linha.chunks_mut(4).zip(xs) {
                        for c in 0..4 {
                            let cima = l0[x0 * 4 + c] as u32 * (256 - fx) + l0[x1 * 4 + c] as u32 * fx;
                            let baixo = l1[x0 * 4 + c] as u32 * (256 - fx) + l1[x1 * 4 + c] as u32 * fx;
                            p[c] = ((cima * (256 - fy) + baixo * fy) >> 16) as u8;
                        }
                    }
                }
            });
        }
    });
    out
}

/// Quanto (0..=255) do pixel `(px, py)` cai no retângulo `r` com cantos `c` (tudo em pixels).
fn cobertura(r: tiny_skia::Rect, c: [f32; 4], px: i32, py: i32) -> u32 {
    let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
    let max = r.width().min(r.height()) / 2.0;
    let c = c.map(|v| v.clamp(0.0, max));
    let fx = (cx + 0.5 - r.left()).min(r.right() - cx + 0.5).clamp(0.0, 1.0);
    let fy = (cy + 0.5 - r.top()).min(r.bottom() - cy + 0.5).clamp(0.0, 1.0);
    let mut cov = fx * fy;
    let canto = if cx < r.left() + c[0] && cy < r.top() + c[0] {
        Some((c[0], r.left() + c[0], r.top() + c[0]))
    } else if cx > r.right() - c[1] && cy < r.top() + c[1] {
        Some((c[1], r.right() - c[1], r.top() + c[1]))
    } else if cx > r.right() - c[2] && cy > r.bottom() - c[2] {
        Some((c[2], r.right() - c[2], r.bottom() - c[2]))
    } else if cx < r.left() + c[3] && cy > r.bottom() - c[3] {
        Some((c[3], r.left() + c[3], r.bottom() - c[3]))
    } else {
        None
    };
    if let Some((k, ox, oy)) = canto {
        cov *= (k - ((cx - ox).powi(2) + (cy - oy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
    }
    (cov * 255.0 + 0.5) as u32
}

/// Pixels de `pm`, opcionalmente esticados pelos índices de `mapa` (nine-slice).
fn de<'a>(pm: &'a Pixmap, mapa: Option<(&'a [usize], &'a [usize])>) -> ((i32, i32), impl Fn(usize, usize) -> [u8; 4] + Sync + 'a) {
    let (d, w) = (pm.data(), pm.width() as usize);
    let tam = mapa.map_or((pm.width() as i32, pm.height() as i32), |(xs, ys)| (xs.len() as i32, ys.len() as i32));
    (tam, move |x, y| {
        let (x, y) = mapa.map_or((x, y), |(xs, ys)| (xs[x], ys[y]));
        let i = (y * w + x) * 4;
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    })
}

/// Escreve em `(x, y)` uma área `sw`×`sh` cujos pixels (pré-multiplicados) vêm de `amostra`,
/// em inteiros, sem o pipeline de pattern do tiny-skia, que é lento em área grande. `forma` limita a um retângulo arredondado;
/// `fora` inverte a forma (sombra só do lado de fora da caixa, como no CSS);
/// `fonte` substitui o que havia em vez de sobrepor; `alfa` (0..255) é a opacidade do todo.
#[allow(clippy::too_many_arguments)]
/// `linha_igual`: a `amostra` não depende de x (cor sólida, degradê vertical): o miolo de cada linha vai de uma vez.
fn compor(dst: &mut Pixmap, clips: &mut [Clip], s: f32, x: i32, y: i32, (sw, sh): (i32, i32), forma: Option<(tiny_skia::Rect, [f32; 4])>, fora: bool, fonte: bool, paralelo: bool, alfa: u32, linha_igual: bool, amostra: impl Fn(usize, usize) -> [u8; 4] + Sync) {
    let (pw, ph) = (dst.width() as i32, dst.height() as i32);
    let area = tiny_skia::Rect::from_xywh(x as f32, y as f32, sw as f32, sh as f32);
    let mask = clip_mask(clips, pw as u32, ph as u32, s, area).map(|m| m.data());
    let (ya, yb) = (y.max(0), (y + sh).min(ph));
    if ya >= yb {
        return;
    }
    // Área grande de um quadro cheio vai em faixas paralelas (o resize precisa da latência);
    // no hover (quadro parcial) criar as threads gasta mais CPU do que economiza.
    let n = if paralelo && (yb - ya) * sw > 1 << 16 { std::thread::available_parallelism().map_or(1, |n| n.get()).min(4) } else { 1 };
    let faixa = (yb - ya + n as i32 - 1) / n as i32;
    let linhas = &mut dst.data_mut()[(ya * pw * 4) as usize..(yb * pw * 4) as usize];
    let amostra = &amostra;
    std::thread::scope(|e| {
        for (i, dd) in linhas.chunks_mut((faixa * pw * 4) as usize).enumerate() {
            let ya = ya + i as i32 * faixa;
            let mut trabalho = move || compor_linhas(dd, ya, pw, x, y, sw, mask, forma, fora, fonte, alfa, linha_igual, amostra);
            if n == 1 {
                trabalho();
            } else {
                e.spawn(trabalho);
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn compor_linhas(dd: &mut [u8], ya: i32, pw: i32, x: i32, y: i32, sw: i32, mask: Option<&[u8]>, forma: Option<(tiny_skia::Rect, [f32; 4])>, fora: bool, fonte: bool, alfa: u32, linha_igual: bool, amostra: &(impl Fn(usize, usize) -> [u8; 4] + Sync)) {
    for py in ya..ya + (dd.len() / (pw as usize * 4)) as i32 {
        // Linhas longe dos cantos e das bordas ficam inteiras dentro da forma.
        let meio = forma.is_some_and(|(r, c)| py as f32 >= r.top() + c[0].max(c[1]) + 1.0 && py as f32 + 1.0 <= r.bottom() - c[2].max(c[3]) - 1.0);
        let (px, fim) = (x.max(0), (x + sw).min(pw));
        // O miolo da linha (cobertura cheia, sem máscara): os fundos dos painéis são quase tudo isso.
        // Nas linhas dos cantos, o miolo fica entre os arcos.
        let dentro_v = forma.is_none_or(|(r, _)| py as f32 >= r.top() + 1.0 && py as f32 + 1.0 <= r.bottom() - 1.0);
        if linha_igual && mask.is_none() && alfa == 255 && !fora && dentro_v {
            let (a, b) = match forma {
                Some((r, c)) => {
                    let (e, d) = if meio { (0.0, 0.0) } else { (c[0].max(c[3]), c[1].max(c[2])) };
                    (((r.left() + e + 1.0).ceil() as i32).max(px), ((r.right() - d - 2.0).floor() as i32 + 1).min(fim))
                }
                None => (px, fim),
            };
            if a < b {
                let sp = amostra(0, (py - y) as usize);
                let linha = &mut dd[(((py - ya) * pw + a) * 4) as usize..(((py - ya) * pw + b) * 4) as usize];
                if fonte || sp[3] == 255 {
                    for d in linha.chunks_exact_mut(4) {
                        d.copy_from_slice(&sp);
                    }
                } else if sp[3] > 0 {
                    let k = 255 - sp[3] as u32;
                    for d in linha.chunks_exact_mut(4) {
                        for c in 0..4 {
                            d[c] = (sp[c] as u32 + d[c] as u32 * k / 255) as u8;
                        }
                    }
                }
                compor_trecho(dd, ya, pw, x, y, py, px, a, mask, forma, fora, fonte, alfa, meio, amostra);
                compor_trecho(dd, ya, pw, x, y, py, b, fim, mask, forma, fora, fonte, alfa, meio, amostra);
                continue;
            }
        }
        compor_trecho(dd, ya, pw, x, y, py, px, fim, mask, forma, fora, fonte, alfa, meio, amostra);
    }
}

/// Os pixels `px..fim` da linha `py`, um a um (bordas, cantos, máscara, imagens).
#[allow(clippy::too_many_arguments)]
fn compor_trecho(dd: &mut [u8], ya: i32, pw: i32, x: i32, y: i32, py: i32, mut px: i32, fim: i32, mask: Option<&[u8]>, forma: Option<(tiny_skia::Rect, [f32; 4])>, fora: bool, fonte: bool, alfa: u32, meio: bool, amostra: &(impl Fn(usize, usize) -> [u8; 4] + Sync)) {
    {
        while px < fim {
            let mut cov = match forma {
                Some((r, _)) if meio && px as f32 >= r.left() + 1.0 && px as f32 + 1.0 <= r.right() - 1.0 => {
                    if fora {
                        px = px.max((r.right() - 1.0).floor() as i32);
                        continue;
                    }
                    255
                }
                Some((r, c)) if fora => 255 - cobertura(r, c, px, py),
                Some((r, c)) => cobertura(r, c, px, py),
                None => 255,
            };
            px += 1;
            let px = px - 1;
            if let Some(m) = mask {
                cov = cov * m[(py * pw + px) as usize] as u32 / 255;
            }
            if alfa < 255 {
                cov = cov * alfa / 255;
            }
            if cov == 0 {
                continue;
            }
            let sp = amostra((px - x) as usize, (py - y) as usize);
            let di = (((py - ya) * pw + px) * 4) as usize;
            if cov == 255 && (fonte || sp[3] == 255) {
                dd[di..di + 4].copy_from_slice(&sp);
            } else if fonte {
                for c in 0..4 {
                    dd[di + c] = ((sp[c] as u32 * cov + dd[di + c] as u32 * (255 - cov)) / 255) as u8;
                }
            } else {
                let a = sp[3] as u32 * cov / 255;
                for c in 0..4 {
                    dd[di + c] = (sp[c] as u32 * cov / 255 + dd[di + c] as u32 * (255 - a) / 255) as u8;
                }
            }
        }
    }
}

/// Desfoque gaussiano (três caixas) feito em resolução reduzida: devolve o resultado e
/// o fator para esticá-lo de volta. Num desfoque largo a perda de detalhe não aparece.
fn desfocar(px: Vec<[u32; 4]>, w: usize, h: usize, k: usize) -> (Pixmap, f32) {
    let d = (k / 2).clamp(1, 4);
    let (sw, sh) = (w.div_ceil(d), h.div_ceil(d));
    let mut pq = vec![[0u32; 4]; sw * sh];
    if d == 1 {
        pq = px;
    } else {
        for y in 0..sh {
            for x in 0..sw {
                let (mut soma, mut n) = ([0u32; 4], 0);
                for yy in y * d..((y + 1) * d).min(h) {
                    for xx in x * d..((x + 1) * d).min(w) {
                        let v = px[yy * w + xx];
                        (0..4).for_each(|c| soma[c] += v[c]);
                        n += 1;
                    }
                }
                pq[y * sw + x] = soma.map(|v| v / n);
            }
        }
    }
    let k = (k / d).max(1);
    for _ in 0..3 {
        box_blur(&mut pq, sw, sh, k, true);
        box_blur(&mut pq, sw, sh, k, false);
    }
    let bytes: Vec<u8> = pq.iter().flat_map(|p| p.map(|c| c.min(255) as u8)).collect();
    (Pixmap::from_vec(bytes, tiny_skia::IntSize::from_wh(sw as u32, sh as u32).expect("w, h > 0")).expect("mesmo tamanho"), d as f32)
}

fn box_blur(px: &mut [[u32; 4]], w: usize, h: usize, k: usize, horizontal: bool) {
    let (linhas, n) = if horizontal { (h, w) } else { (w, h) };
    let idx = |l: usize, i: usize| if horizontal { l * w + i } else { i * w + l };
    let mut linha = vec![[0u32; 4]; n];
    let div = (2 * k + 1) as u32;
    for l in 0..linhas {
        for (i, v) in linha.iter_mut().enumerate() {
            *v = px[idx(l, i)];
        }
        let mut soma = [0u32; 4];
        for j in 0..=2 * k {
            let v = linha[j.saturating_sub(k).min(n - 1)];
            (0..4).for_each(|c| soma[c] += v[c]);
        }
        for i in 0..n {
            px[idx(l, i)] = soma.map(|v| v / div);
            let sai = linha[i.saturating_sub(k)];
            let entra = linha[(i + k + 1).min(n - 1)];
            (0..4).for_each(|c| soma[c] = soma[c] + entra[c] - sai[c]);
        }
    }
}

fn blend(p:&mut tiny_skia::PremultipliedColorU8, r: u8, g: u8, b: u8, a: u32) {
    let inv = 255 - a;
    let m = |src: u8, dst: u8| ((src as u32 * a + dst as u32 * inv) / 255) as u8;
    let alpha = (a + p.alpha() as u32 * inv / 255) as u8;
    *p = tiny_skia::PremultipliedColorU8::from_rgba(m(r, p.red()).min(alpha), m(g, p.green()).min(alpha), m(b, p.blue()).min(alpha), alpha).unwrap_or(*p);
}

fn skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), c.b.clamp(0.0, 1.0), c.a.clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint { anti_alias: true, ..Default::default() };
    p.set_color(skia_color(c));
    p
}

fn rounded_path(r: Rect, radius: [f32; 4], s: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    push_rounded_path(&mut pb, r, radius, s);
    pb.finish()
}

fn push_rounded_path(pb: &mut PathBuilder, r: Rect, radius: [f32; 4], s: f32) {
    if r.width <= 0.0 || r.height <= 0.0 {
        return;
    }
    let (x, y, w, h) = (r.x * s, r.y * s, r.width * s, r.height * s);
    let max = w.min(h) / 2.0;
    let [tl, tr, br, bl] = radius.map(|v| (v * s).clamp(0.0, max));
    // Constante da aproximação de um quarto de círculo por cúbica.
    const K: f32 = 0.447_715;
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.cubic_to(x + w - tr * K, y, x + w, y + tr * K, x + w, y + tr);
    pb.line_to(x + w, y + h - br);
    pb.cubic_to(x + w, y + h - br * K, x + w - br * K, y + h, x + w - br, y + h);
    pb.line_to(x + bl, y + h);
    pb.cubic_to(x + bl * K, y + h, x, y + h - bl * K, x, y + h - bl);
    pb.line_to(x, y + tl);
    pb.cubic_to(x, y + tl * K, x + tl * K, y, x + tl, y);
    pb.close();
}

/// O que mudou de um quadro para o outro.
#[derive(Debug, Clone, PartialEq)]
pub enum Changes {
    /// Nada: o quadro na tela continua valendo.
    None,
    /// Redesenhar tudo (primeiro quadro, ou a ordem dos desenhos mudou).
    All,
    /// Só estas regiões (lógicas).
    Regions(Vec<Rect>),
}

/// Impressão digital de um desenho: tudo que muda o resultado entra no hash.
struct Chave(std::collections::hash_map::DefaultHasher);

impl Chave {
    fn new(tipo: u8) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&tipo, &mut h);
        Chave(h)
    }
    fn h(&mut self, v: impl std::hash::Hash) -> &mut Self {
        v.hash(&mut self.0);
        self
    }
    fn f(&mut self, v: f32) -> &mut Self {
        self.h(v.to_bits())
    }
    fn r(&mut self, r: Rect) -> &mut Self {
        self.f(r.x).f(r.y).f(r.width).f(r.height)
    }
    fn c(&mut self, c: Color) -> &mut Self {
        self.f(c.r).f(c.g).f(c.b).f(c.a)
    }
    fn borda(&mut self, b: Border) -> &mut Self {
        self.c(b.color).f(b.width).h(b.radius.map(f32::to_bits))
    }
}

use std::hash::{Hash, Hasher};

/// Regiões a redesenhar: as áreas dos desenhos que só um dos dois quadros tem. Perto de um
/// vidro a mudança se espalha pelo raio do desfoque, dentro dele.
fn comparar(velho: &[(Rect, u64)], novo: &[(Rect, u64)], vidros: &[(Rect, f32)]) -> Changes {
    if velho.is_empty() {
        return Changes::All;
    }
    // Contagem por chave: a mesma chave é o mesmo desenho, na mesma área.
    let mut conta: HashMap<u64, (i32, Rect)> = HashMap::new();
    for (r, k) in velho {
        conta.entry(*k).or_insert((0, *r)).0 += 1;
    }
    for (r, k) in novo {
        conta.entry(*k).or_insert((0, *r)).0 -= 1;
    }
    let mut sujos: Vec<Rect> = conta.values().filter(|(n, _)| *n != 0).map(|(_, r)| *r).collect();
    if sujos.is_empty() {
        // Os mesmos desenhos em outra ordem (camadas trocadas) mudam o resultado.
        return if velho.iter().map(|d| d.1).eq(novo.iter().map(|d| d.1)) { Changes::None } else { Changes::All };
    }
    let espalhados: Vec<Rect> = vidros.iter().flat_map(|(v, raio)| sujos.iter().filter(|s| cruza(**s, *v)).map(move |s| crescer(*s, *raio).intersect(v))).collect();
    sujos.extend(espalhados);
    // Junta as que se encostam; muitas pedaços viram uma só.
    let mut juntou = true;
    while juntou {
        juntou = false;
        'fora: for i in 0..sujos.len() {
            for j in i + 1..sujos.len() {
                if cruza(crescer(sujos[i], 8.0), sujos[j]) {
                    sujos[i] = uniao(sujos[i], sujos[j]);
                    sujos.swap_remove(j);
                    juntou = true;
                    break 'fora;
                }
            }
        }
    }
    if sujos.len() > 4 {
        let u = sujos.iter().copied().reduce(uniao).expect("não vazio");
        sujos = vec![u];
    }
    Changes::Regions(sujos)
}

/// Quanto as letras passam da caixa medida: com a linha baixa (`lh` < 1,4), acentos e
/// pernas saem por cima e por baixo.
pub(crate) fn sobra_glifos(size: f32, lh: f32) -> f32 {
    size * ((1.4 - lh).max(0.0) / 2.0 + 0.1)
}

pub(crate) fn crescer(r: Rect, m: f32) -> Rect {
    Rect { x: r.x - m, y: r.y - m, width: r.width + 2.0 * m, height: r.height + 2.0 * m }
}

fn cruza(a: Rect, b: Rect) -> bool {
    let i = a.intersect(&b);
    i.width > 0.0 && i.height > 0.0
}

fn uniao(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect { x, y, width: (a.x + a.width).max(b.x + b.width) - x, height: (a.y + a.height).max(b.y + b.height) - y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corte_so_fora_do_recorte_e_sem_transformacao() {
        let mut r = Renderer::new();
        r.begin(Size::new(200.0, 200.0), 1.0, Color::BLACK);
        let longe = Rect::new(Point::new(0.0, 500.0), Size::new(50.0, 50.0));
        assert!(!r.clipped_out(longe, 0.0), "sem recorte nada é cortado");
        r.push_clip(Rect::new(Point::ORIGIN, Size::new(200.0, 200.0)), [0.0; 4]);
        assert!(r.clipped_out(longe, 48.0));
        assert!(!r.clipped_out(Rect::new(Point::new(0.0, 230.0), Size::new(50.0, 50.0)), 48.0), "a folga guarda a sombra");
        r.push_transform(vetor::Transform::rotate(90.0));
        assert!(!r.clipped_out(longe, 0.0));
        r.pop_transform();
        r.pop_clip();
    }

    #[test]
    fn resize_nao_acumula_cache() {
        // Foto esticada na janela, caixa com sombra e texto: cada largura gera cópias novas.
        let img = Image::from_rgba(64, 64, vec![200; 64 * 64 * 4]).expect("tamanho confere");
        let mut r = Renderer::new();
        for i in 0..30 {
            let w = 600.0 + i as f32 * 7.0;
            r.begin(Size::new(w, 400.0), 1.0, Color::BLACK);
            r.draw_image(&img, Rect { x: 0.0, y: 0.0, width: w, height: 400.0 }, [0.0; 4]);
            let caixa = Rect { x: 20.0, y: 20.0, width: w - 40.0, height: 60.0 + i as f32 };
            r.fill_quad(caixa, None, Border::default(), Some(Shadow { color: Color::BLACK, offset: Point::new(0.0, 4.0), blur: 12.0 }));
            r.draw_text(&format!("largura {w}"), 14.0, Font::default(), Color::WHITE, Point::new(30.0, 30.0), Some(w - 60.0));
        }
        // Só o quadro anterior e o atual ficam.
        assert!(r.imagens.len() <= 2, "{} imagens", r.imagens.len());
        assert!(r.texts.len() <= 2, "{} textos", r.texts.len());
        assert!(r.shadows.len() <= 2, "{} sombras", r.shadows.len());
    }

    #[test]
    fn rolar_so_com_quadros_parciais_nao_acumula_textos() {
        // Como a janela: troca, quadro parcial, troca. Cada quadro mede um texto novo (uma linha
        // que entrou rolando); antes o cache passava de 9000 e seguia.
        for (cache, max) in [(false, 1), (true, 5)] {
            let mut r = Renderer { cache_textos: cache, ..Renderer::new() };
            let mut q = Quadro::default();
            let tela = Size::new(100.0, 100.0);
            r.trocar(&mut q);
            r.begin(tela, 1.0, Color::BLACK);
            r.trocar(&mut q);
            for i in 0..9000 {
                r.trocar(&mut q);
                r.measure(&format!("linha {i}"), 12.0, Font::DEFAULT, None);
                r.measure(&format!("linha {i}"), 12.0, Font::DEFAULT, None);
                assert!(r.begin_parcial(tela, 1.0, Color::BLACK, Rect::new(Point::ORIGIN, Size::new(10.0, 10.0))));
                r.end();
                r.trocar(&mut q);
            }
            assert!(r.texts.len() <= max, "cache {cache}: {} textos", r.texts.len());
        }
    }

    #[test]
    fn cache_cheio_no_meio_do_quadro_guarda_o_que_esta_em_uso() {
        let mut r = Renderer::new();
        r.begin(Size::new(100.0, 100.0), 1.0, Color::BLACK);
        for i in 0..4100 {
            r.measure(&format!("t{i}"), 12.0, Font::DEFAULT, None);
        }
        assert!(r.texts.len() > 4000, "{} textos: o quadro usa todos", r.texts.len());
        // Só quadros parciais (nenhum `begin`): SVGs de 4 MB cada não passam muito do limite.
        let svg = Svg::from_bytes(br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="#fff"/></svg>"##).unwrap();
        let tela = Size::new(1000.0, 1000.0);
        r.begin(tela, 1.0, Color::BLACK);
        for i in 0..45 {
            assert!(r.begin_parcial(tela, 1.0, Color::BLACK, Rect::new(Point::ORIGIN, tela)));
            r.draw_svg(&svg, Rect { x: 0.0, y: 0.0, width: 1000.0 - i as f32, height: 1000.0 }, None);
            r.end();
        }
        assert!(r.svgs.bytes() < 150_000_000, "{} bytes de SVG", r.svgs.bytes());
    }
}
