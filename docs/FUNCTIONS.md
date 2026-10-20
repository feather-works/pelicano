# Pelicano: referência de funções

Referência de toda a API pública, com o que cada função recebe e devolve.
Escrita para que uma pessoa ou outra IA consiga usar a biblioteca sem ler o código.
Importe com `use pelicano::*;` **e** `use pelicano::column;`: a biblioteca padrão também tem um `column!`, e sem a linha explícita o compilador acusa ambiguidade. `row!` e `stack!` vêm com o `*`.

Convenções que valem para tudo:

- **Unidades:** tamanhos e posições em pixels lógicos (`f32`). A escala da tela é aplicada só no desenho.
- **`M`:** o tipo das mensagens do app (o `App::Msg`). Widgets que mandam mensagens pedem `M: Clone`.
- **Construtores:** cada widget tem uma função construtora (`button(...)`, `text(...)`) e métodos encadeáveis
  (`.width(...).padding(...)`). Os métodos tomam `self` e devolvem `Self`.
- **`Element`:** qualquer widget vira `Element<'a, M>` com `.into()`. `&str` e `String` viram `text` direto.
- **Tamanhos numéricos:** onde aparece `impl Into<Num>`, servem `14`, `14.0` ou `14u16`.
- **Estilos:** são closures. `Fn(&Theme) -> Style` (contêiner) ou `Fn(&Theme, Status) -> Style` (o que reage ao mouse).

---

## 1. Aplicativo e janela (`app.rs`)

### `trait App`

| Item | Assinatura | O que faz |
|---|---|---|
| `type Msg` | `Clone + Send + 'static` | As mensagens do app. |
| `update` | `fn update(&mut self, msg: Self::Msg) -> Task<Self::Msg>` | Muda o estado. Devolve trabalho a fazer (`Task::none()` se não houver). |
| `view` | `fn view(&self) -> Element<'_, Self::Msg>` | Monta a tela a partir do estado. Roda a cada quadro que precisa ser redesenhado. |
| `theme` | `fn theme(&self) -> Theme` | Opcional. Padrão: `Theme::dark()`. |
| `on_event` | `fn on_event(&self, ev: &Event) -> Option<Self::Msg>` | Opcional. Vê todo evento da janela (atalhos, fechar, redimensionar). `Some(msg)` manda a mensagem. |
| `view_window` | `fn view_window(&self, id: WindowId) -> Element<'_, Self::Msg>` | Opcional. O que aparece em cada janela aberta com `Task::open_window`. Padrão: `view()`. |
| `on_window_event` | `fn on_window_event(&self, id: WindowId, ev: &Event) -> Option<Self::Msg>` | Opcional. Os eventos de cada janela. Padrão: na principal chama `on_event`, nas outras nada. |

### Várias janelas

`WindowId(u64)` identifica cada janela; o app escolhe o número. A principal é `WindowId::MAIN` (o `0`).
`Task::open_window(id, settings)` abre outra, que mostra o `view_window(id)`. Todas dividem o mesmo
renderizador (fontes e caches) e a mesma placa de vídeo: uma janela a mais custa só a superfície e o quadro dela.
Cada janela tem o próprio estado de widgets (foco, rolagem, hover).

### `run`

```rust
pub fn run<A: App>(settings: Settings, init: impl FnOnce() -> (A, Task<A::Msg>)) -> Result<(), Error>
```

- **Entrada:** as configurações da janela e uma função que cria o estado inicial e a primeira tarefa.
- **Saída:** só retorna quando a janela fecha. `Err(Error)` se a janela ou a GPU não abrirem.
- **`Error`:** `EventLoop(winit::error::EventLoopError)`, `Window(String)` ou `Gpu(String)`. Implementa `Display`.

```rust
use pelicano::column;
use pelicano::*;

struct Contador { n: i32 }
#[derive(Clone)] enum Msg { Mais }

impl App for Contador {
    type Msg = Msg;
    fn update(&mut self, m: Msg) -> Task<Msg> { match m { Msg::Mais => self.n += 1 } Task::none() }
    fn view(&self) -> Element<'_, Msg> {
        column![text(self.n.to_string()).size(32), button("+1").on_press(Msg::Mais)].spacing(8).into()
    }
}

fn main() -> Result<(), pelicano::Error> {
    pelicano::run(Settings { title: "Contador".into(), ..Default::default() }, || (Contador { n: 0 }, Task::none()))
}
```

### `Settings`

| Campo | Tipo | Padrão | O que faz |
|---|---|---|---|
| `title` | `String` | `"Pelicano"` | Título da janela. |
| `size` | `Size` | `1024 × 720` | Tamanho inicial. |
| `min_size` | `Option<Size>` | `None` | Tamanho mínimo. |
| `decorations` | `bool` | `true` | `false`: sem barra do sistema. O app desenha a própria e usa `WindowCmd::Drag`. |
| `transparent` | `bool` | `false` | Janela com alfa (cantos arredondados, fundo translúcido). |
| `blur` | `bool` | `false` | Pede ao compositor para desfocar o que está atrás (KDE, Windows, macOS; os outros ignoram). |
| `resizable` | `bool` | `true` | Permite redimensionar. |
| `close_on_request` | `bool` | `true` | `false`: o "fechar" só chega como `Event::CloseRequested` e o app decide. |
| `app_id` | `Option<String>` | `None` | Nome da janela para o sistema (no Linux, casa com o `.desktop`). |
| `icon` | `Option<Icon>` | `None` | Ícone da janela (Windows e X11; no Wayland vem do `.desktop`). |
| `position` | `Option<Point>` | `None` | Onde a janela abre (canto de cima à esquerda). O Wayland ignora. |
| `skip_taskbar` | `bool` | `false` | Fora da barra de tarefas e da dock (Windows e X11; o Wayland não deixa). |
| `x11` | `bool` | `false` | Linux: usa o X11 (XWayland) em vez do Wayland, para posição, `Level` e `skip_taskbar` funcionarem. Vale para o app todo; sem X, cai no padrão. |
| `run_in_background` | `bool` | `false` | Fechar a principal (ou a última janela) não acaba o app; só o `WindowCmd::Exit`. |
| `start_hidden` | `bool` | `false` | A principal não abre sozinha; só com `Task::open_window(WindowId::MAIN, …)`. |
| `debug` | `bool` | `false` | Libera o inspetor de layout no F12. `PELICANO_DEBUG=1` também libera, já ligado. |

Variáveis de ambiente: `PELICANO_DEBUG=1` (inspetor ligado) e `PELICANO_DANO=1` (imprime as regiões redesenhadas).

### `Task<M>`

O trabalho que o `update` devolve. Futuros e fluxos rodam fora da thread da interface. Cada resultado volta como mensagem.

| Função | Entrada | Saída / efeito |
|---|---|---|
| `Task::none()` | — | Nada a fazer. |
| `Task::done(m)` | uma mensagem | A mensagem chega no próximo giro do laço. |
| `Task::perform(fut, f)` | `Future<Output = T>` e `FnOnce(T) -> M` | Roda o futuro e manda `f(resultado)`. |
| `Task::run(stream, f)` | `Stream<Item = T>` e `FnMut(T) -> M` | Cada item do fluxo vira uma mensagem (progresso de download, relógio). |
| `Task::window(cmd)` | `WindowCmd` | Uma ordem para a janela principal. |
| `Task::window_of(id, cmd)` | `WindowId`, `WindowCmd` | Uma ordem para outra janela. Antes de ela abrir, a ordem espera. |
| `Task::open_window(id, settings)` | `WindowId`, `Settings` | Abre outra janela (não abre duas com o mesmo `id`). |
| `Task::batch(tasks)` | `IntoIterator<Item = Task<M>>` | Junta várias tarefas em uma. |
| `task.map(f)` | `Fn(M) -> N` | Converte as mensagens (componente dentro de outro app). |
| `task.abortable()` | — | `(Task<M>, Handle)`. `handle.abort()` cancela (pausar download). |

`pelicano::futures` é reexportado para criar canais e fluxos.

### `WindowCmd`

Ordens para a janela, enviadas com `Task::window(...)` ou `Task::window_of(id, ...)`.
As que um widget manda (`Drag` no clique da barra) vão para a janela do clique.

| Variante | Efeito |
|---|---|
| `Drag` | Começa a arrastar a janela. Chame no clique da barra de título. |
| `DragResize(ResizeDirection)` | Começa a redimensionar por uma borda. Funciona sem moldura. |
| `Minimize` | Minimiza. |
| `ToggleMaximize` | Maximiza ou restaura. |
| `ToggleFullscreen` | Tela cheia sem moldura no monitor atual, ou volta. |
| `Close` | Fecha a janela. Sem `run_in_background`, fechar a principal (ou a última) acaba o app. |
| `Exit` | Acaba o app, com todas as janelas. |
| `Focus` | Traz para a frente e tira do minimizado (no Wayland pode só piscar na barra). |
| `Level(WindowLevel)` | `AlwaysOnTop`, `AlwaysOnBottom` (presa no fundo) ou `Normal`. O Wayland ignora. |
| `Resize(Size)` | Muda o tamanho. |
| `Title(String)` | Muda o título. |
| `Blur(bool)` | Liga ou desliga o desfoque do compositor atrás da janela. |

---

## 2. Tipos básicos (`lib.rs`)

### Geometria

| Tipo | Campos | Métodos |
|---|---|---|
| `Point` | `x, y: f32` | `Point::new(x, y)`, `Point::ORIGIN` |
| `Size` | `width, height: f32` | `Size::new(w, h)`, `Size::ZERO`, `Size::INFINITE` |
| `Rect` | `x, y, width, height: f32` | `Rect::new(Point, Size)`; `.size() -> Size`; `.contains(Point) -> bool`; `.intersect(&Rect) -> Rect`; `.shrink(Padding) -> Rect` |
| `Padding` | `top, right, bottom, left: f32` | `Padding::ZERO`; `.horizontal()` e `.vertical()` devolvem a soma dos lados |

`Padding` aceita `From`: `f32` ou `u16` (os quatro lados iguais) e `[v, h]` como `[u16; 2]` ou `[f32; 2]` (vertical, horizontal).

### `Length`

| Variante | Significado |
|---|---|
| `Shrink` | O tamanho do conteúdo. |
| `Fill(n)` | Ocupa a sobra. `n` é a proporção entre os irmãos que também enchem. `Length::FILL` = `Fill(1)`. |
| `Fixed(f32)` | Tamanho fixo. `f32` e `u16` viram `Fixed` com `.into()`. |

### `Align`

`Start` (padrão), `Center` e `End`. `align.offset(espaco, item) -> f32` dá o deslocamento de um item de tamanho `item` dentro de `espaco`.

### `Color`

sRGB com componentes de 0 a 1 e alfa não pré-multiplicado. Campos `r, g, b, a`.

| Função | Entrada | Saída |
|---|---|---|
| `Color::rgb(r, g, b)` / `Color::rgba(r, g, b, a)` | `f32` de 0 a 1 | `Color` |
| `Color::rgb8(r, g, b)` | `u8` | `Color` |
| `Color::hex(s)` | `"#rrggbb"` ou `"#rrggbbaa"` | `Option<Color>` (`None` se inválido) |
| `c.alpha(a)` | novo alfa | a mesma cor com esse alfa |
| `c.mix(o, t)` | outra cor e `t` de 0 a 1 | mistura (`0` = `c`, `1` = `o`). Base dos estados hover/pressionado. |
| Constantes | — | `Color::BLACK`, `Color::WHITE`, `Color::TRANSPARENT` |

Em `pickers.rs`: `to_hsv(c) -> (h 0..360, s 0..1, v 0..1)`, `from_hsv(h, s, v) -> Color` e `to_hex(c) -> String`.

### `Border`, `Shadow`, `Background`

| Tipo | Campos | Construção |
|---|---|---|
| `Border` | `color: Color`, `width: f32`, `radius: [f32; 4]` (sup. esq., sup. dir., inf. dir., inf. esq.) | `Border::rounded(r)`; `.color(cor, largura)` |
| `Shadow` | `color: Color`, `offset: Point`, `blur: f32` (quanto espalha) | literal de struct |
| `Background` | `Color(Color)` ou `Linear { angle: f32 (radianos, 0 = esquerda→direita), stops: Vec<(f32, Color)> }` | `Color` vira `Background` com `.into()` |

### `Theme`

A paleta padrão dos widgets. Campos (todos `Color`): `background`, `surface`, `text`, `muted`, `primary`, `on_primary`, `border`, `danger`.
Prontos: `Theme::dark()` e `Theme::light()`. Para mudar, use `Theme { primary: ..., ..Theme::dark() }`.

### `Style`

A caixa de qualquer widget:

| Campo | Tipo | O que faz |
|---|---|---|
| `background` | `Option<Background>` | Fundo. |
| `border` | `Border` | Borda e raio. O raio também recorta o fundo e a sombra. |
| `shadow` | `Option<Shadow>` | Sombra desfocada de verdade. |
| `text_color` | `Option<Color>` | Cor herdada pelos textos de dentro. |
| `backdrop_blur` | `f32` | Desfoca o que está atrás da caixa (o `backdrop-filter: blur()` do CSS). |
| `transition` | `Option<Transition>` | Mudanças de estilo andam em vez de pular (o `transition` do CSS). Veja a seção 5. |

Construtores: `Style::glass(raio)` (vidro pronto: fundo translúcido, fio de luz na borda, sombra e desfoque 18) e
os encadeáveis `.background(..)`, `.border(..)`, `.shadow(..)`, `.text_color(..)`, `.backdrop_blur(..)` e `.transition(..)`.
Os estilos padrão (`button_primary`, `button_text`, `input_default`) já vêm com uma transição de 120 ms.

### `Status`

O estado que as closures de estilo recebem: `Idle`, `Hovered`, `Pressed`, `Focused` ou `Disabled`.

### `Event`

Os eventos da janela, já em pixels lógicos:

| Variante | Quando |
|---|---|
| `CursorMoved(Point)` | O mouse andou. |
| `CursorLeft` | O mouse saiu da janela. |
| `MousePressed(MouseButton)` / `MouseReleased(MouseButton)` | Clique. |
| `Wheel { dx, dy, modifiers }` | Rolagem. Positivo = conteúdo desce. Com Ctrl, os apps costumam dar zoom. |
| `Key { key: Key, text: Option<String>, modifiers, pressed }` | Tecla. `text` é o que ela digita. |
| `Resized(Size)` | A janela mudou de tamanho. |
| `Moved(Point)` | A janela foi para outro lugar da tela (o Wayland nunca manda). |
| `CloseRequested` | Pediram para fechar. Com `close_on_request = false` a janela fica e o app decide. |
| `Focused(bool)` | A janela ganhou ou perdeu o foco. |

Reexportados do winit: `Key`, `NamedKey`, `Modifiers`, `MouseButton`, `Icon`, `ResizeDirection` e `CursorIcon`.

---

## 3. Layout e conteúdo (`widget.rs`)

### `text`

```rust
text(s: impl Into<Cow<str>>) -> Text
```

Métodos: `.size(Num)`, `.font(Font)`, `.color(Color)`, `.width(Length)`, `.height(Length)`, `.align_x(Align)` e `.no_wrap()`.
Quebra linha sozinho na largura disponível. Sem `.color`, usa a cor herdada do contêiner ou o `theme.text`.

### `Font`

Campos `family: Option<&'static str>` (`None` = sem serifa do sistema) e `weight: u16`.
Constantes: `Font::DEFAULT` (400), `MEDIUM` (500), `SEMIBOLD` (600), `BOLD` (700), `MONO` e `SERIF`.

### `container`

```rust
container(content: impl Into<Element>) -> Container
```

Uma caixa em volta de um filho. Métodos:

| Método | Entrada | Efeito |
|---|---|---|
| `.padding(p)` | `Into<Padding>` | Espaço interno. |
| `.margin(m)` | `Into<Padding>` | Espaço por fora do fundo e da borda (o `margin` do CSS). |
| `.width(l)` / `.height(l)` | `Into<Length>` | Tamanho. |
| `.max_width(w)` | `f32` | Largura máxima. |
| `.align_x(a)` / `.align_y(a)` | `Align` | Posição do filho dentro da caixa. |
| `.center()` | — | Centraliza nos dois eixos. |
| `.clip(bool)` | — | Recorta o filho pelo contorno, cantos arredondados inclusive. |
| `.style(f)` | `Fn(&Theme) -> Style` | Fundo, borda, sombra, vidro. |

### `row` / `column` (`Flex`)

```rust
row(children: impl IntoIterator<Item = Element>) -> Flex
column(children: impl IntoIterator<Item = Element>) -> Flex
row![a, b, c]        // cada item passa por Element::from
column![a, b, c]
```

`Row` e `Column` são apelidos de `Flex`. Métodos:

| Método | Efeito |
|---|---|
| `.push(e)` | Acrescenta um filho. |
| `.push_maybe(Option<e>)` | Acrescenta só se for `Some`. |
| `.spacing(Num)` | Espaço entre os filhos. |
| `.padding(p)` | Espaço interno. |
| `.width(l)` / `.height(l)` | Tamanho. |
| `.align(Align)` | Alinhamento no eixo cruzado (vertical numa linha, horizontal numa coluna). |
| `.wrap()` | Só em `row`: quebra em várias linhas quando não cabe (grade de cartões). |

Filhos com `Length::Fill(n)` dividem a sobra na proporção `n`.

### `stack`

```rust
stack(children) -> Stack      // ou stack![a, b]
```

Camadas umas sobre as outras. A última fica por cima e recebe os eventos primeiro. Métodos: `.push(e)`, `.width(l)` e `.height(l)`.

### `opaque`

`opaque(content) -> Opaque`: bloqueia o mouse para o que está abaixo na pilha (fundo de diálogo, menu).

### `Space`

`Space::new(w, h)`, `horizontal_space()` (enche na horizontal) e `vertical_space()` (enche na vertical).

### `scrollable`

`scrollable(content) -> Scrollable`: rolagem vertical com barra arrastável. Padrão `Length::FILL` nos dois eixos. Métodos: `.width(l)` e `.height(l)`.
A posição da rolagem fica guardada sozinha entre quadros.

### `image` e `Image`

| Função | Entrada | Saída |
|---|---|---|
| `Image::from_rgba(w, h, rgba)` | pixels RGBA8 sem pré-multiplicar, linha a linha | `Option<Image>` (`None` se o tamanho não bate) |
| `Image::from_png(bytes)` | bytes de um PNG | `Option<Image>` |
| `img.size()` | — | `Size` em pixels |
| `image(img) -> ImageView` | `Image` | widget |

`ImageView`: `.width(l)`, `.height(l)`, `.content_fit(ContentFit)` e `.radius(f32)` (cantos arredondados).
`ContentFit`: `Fill` (estica sem manter a proporção), `Contain` (cabe inteira, com sobra) ou `Cover` (cobre tudo e corta o que passa).
`Image` é barata de clonar (`Arc`). Guarde no estado do app e passe `img.clone()` no `view`.

### `svg` e `Svg`

`Svg::from_bytes(bytes) -> Option<Svg>`, `svg.size() -> Size` e `svg(s) -> SvgView`.
`SvgView`: `.width(l)`, `.height(l)` e `.color(Color)` (pinta tudo nessa cor, mantendo o alfa; bom para ícones).
O SVG é analisado uma vez, e a rasterização fica em cache por tamanho e cor.

### `progress_bar`

`progress_bar(range: RangeInclusive<f32>, value: f32) -> ProgressBar`. Fora do intervalo, a barra fica vazia ou cheia.
Métodos: `.width(l)`, `.height(f32)` e `.color(Color)`.

### `tooltip`

`tooltip(content, dica: impl Into<Cow<str>>, TooltipPosition::Top | Bottom) -> Tooltip`: mostra a dica com o mouse em cima.

### `canvas`

```rust
canvas(width, height, paint: impl Fn(&mut Renderer, Rect, &Theme)) -> Canvas
```

Um retângulo onde você desenha à mão com o `Renderer` (gráficos, réguas, prévias). `paint` recebe a área do canvas em coordenadas absolutas.

`Renderer::push_opacity(a)` / `pop_opacity()`: tudo desenhado no meio sai com o alfa multiplicado por `a`.
Cada desenho fica translúcido por conta própria, não o grupo: onde dois se sobrepõem, o de baixo aparece um pouco.
`Renderer::push_layer(area, escala, graus, opacidade)` / `pop_layer()`: o que for desenhado no meio cai numa camada
do tamanho de `area` e vai para a tela transformado em volta do centro dela, com a opacidade do grupo.

---

## 5. Transições, animações e acessibilidade (`anim.rs`, `access.rs`)

### Transições

```rust
button("Salvar").style(|t, s| button_primary(t, s).transition(Transition::new(200).easing(Easing::EaseOut)))
```

| Tipo | O que é |
|---|---|
| `Transition::new(ms)` | Duração. `.easing(Easing)` e `.delay(ms)` encadeiam. |
| `Easing` | `Linear`, `Ease` (padrão), `EaseIn`, `EaseOut`, `EaseInOut`, `CubicBezier(x1, y1, x2, y2)`, as curvas do CSS. `apply(t)` dá o progresso. |
| `Lerp` | O que sabe ir de um valor a outro: `f32`, `Point`, `Color`, `Border`, `Shadow`, `Background`, `Style`, `Keyframe`, `Option<T>`. |

Com o estilo tendo `transition`, qualquer mudança dele anda: hover, foco, ou o app trocar a cor (cartão escolhido).
Fundo e sombra que surgem ou somem vêm do transparente; degradês com número diferente de paradas trocam no meio.

Num widget próprio: `cx.transition(node.key, alvo, Transition)` em `draw` devolve o valor de agora a caminho de
`alvo` (cada tipo de valor anda separado). `cx.styled(node.key, style)` aplica a transição do próprio estilo.

### `animate`

```rust
animate(content) -> Animate
```

| Método | O que faz |
|---|---|
| `.opacity(f32)` / `.offset(x, y)` / `.scale(k)` / `.rotate(graus)` | O alvo. Mudou, anima até lá. Nada disso mexe no layout (o `transform` do CSS); o clique segue o deslocamento (não a escala nem a rotação). |
| `.transition(Transition)` | Padrão: 250 ms, `Ease`. |
| `.hover(Keyframe)` / `.pressed(Keyframe)` | O alvo com o mouse em cima / com o botão apertado em cima (o `:hover` e o `:active`). |
| `.enter(Keyframe)` | Como ele aparece: ao entrar na tela anima dali até o alvo (o `@starting-style`). Sumir de um quadro e voltar conta como entrar. |
| `.show(bool)` / `.exit(Keyframe)` | `show(false)` anima até a saída (padrão: só some), depois para de desenhar e de pegar clique. O lugar no layout continua ocupado (numa pilha não se nota). |
| `.repeat(Repeat::Forever \| Repeat::Alternate)` | Laço sem fim da entrada (ou do normal) até o alvo: pulsar, piscar, girar (`.rotate(360.0)` com `Easing::Linear`). |

`Keyframe::default().opacity(0.0).offset(0.0, 24.0).scale(0.9).rotate(15.0)` monta um quadro-chave (o padrão é
opacidade 1, no lugar, tamanho 1, sem giro). Escala e rotação desenham o grupo numa camada (`Renderer::push_layer`),
que também deixa a opacidade certa para o grupo; um vidro dentro dela não vê o que está atrás.

Os widgets prontos já animam: menus, calendário e seletor de cor surgem crescendo um pouco (o menu também se apaga
ao fechar); a dica aparece depois de 350 ms em cima; checkbox enche de cor, rádio engrossa, toggler desliza, a bolinha
do slider cresce, a barra de progresso escorrega até o valor novo e as abas trocam de cor andando.

Como roda: o relógio é o do quadro (`States::tick`). Enquanto alguma transição não acabou, a janela pede o próximo
quadro no ritmo do monitor e redesenha só a área que mudou; parado, não roda nada. Sem `tick` (capturas com
`capture`, testes), tudo já está no fim. Para testar o meio de uma animação: `states.tick(instante)` antes do `draw`,
e `states.animating()` diz se ainda falta.

### Acessibilidade (AccessKit)

A janela conversa sozinha com o leitor de tela do sistema (Orca, NVDA, Narrador, VoiceOver). A árvore sai do
próprio desenho e só é montada quando há um leitor ligado.

| Widget | Para o leitor |
|---|---|
| `text` | Rótulo. Dentro de um botão, vira o nome do botão. |
| `button`, `mouse_area` clicável | Botão (desabilitado sem `on_press`). |
| `text_input` | Campo (senha sem o valor), com o placeholder. |
| `text_editor` | Campo de várias linhas, com o texto. |
| `checkbox`, `toggler`, `radio` | Caixa, chave ou rádio, marcado ou não. |
| `pick_list` | Caixa de seleção com o valor e se está aberta. |
| `slider`, `progress_bar` | Valor, mínimo e máximo. |
| `scrollable` | Área de rolagem com o conteúdo dentro. |

`Element::access_label("Fechar")`: o nome que o leitor fala (o `aria-label`): botão só de ícone, imagem.
Clicar e focar pelo leitor viram um clique no meio do widget.

Num widget próprio: `cx.access(key, rect, || accesskit::Node::new(Role::..))` em `draw`, ou `access_begin` / `access_end`
para nós com filhos; `cx.access_text` para texto. `pelicano::accesskit` é o crate. `access_tree(..)` devolve a árvore
de um quadro (para testes).

---

## 4. Controles (`widget.rs`, `controls.rs`)

### `button`

```rust
button(content: impl Into<Element>) -> Button      // M: Clone
```

| Método | Efeito |
|---|---|
| `.on_press(m)` | Manda `m` ao clicar. Sem isso, o botão fica desabilitado. |
| `.on_press_maybe(Option<m>)` | `None` = desabilitado. |
| `.padding(p)`, `.width(l)`, `.height(l)` | Tamanho. |
| `.style(f)` | `Fn(&Theme, Status) -> Style`. |

Estilos prontos: `button_primary` (padrão: cor primária, mais clara no hover e mais escura apertado) e `button_text` (sem fundo, só acende no hover).

### `mouse_area`

`mouse_area(content) -> MouseArea`: dá cliques a qualquer coisa.
Métodos: `.on_press(m)`, `.on_release(m)`, `.on_right_press(m)`, `.on_double_click(m)` (até 400 ms; use para maximizar pela barra de título) e `.cursor_icon(CursorIcon)`.

### `text_input`

```rust
text_input(placeholder, value) -> TextInput      // M: Clone
```

O campo não guarda o texto: o app guarda e passa de volta em `value` a cada `view`.

| Método | Efeito |
|---|---|
| `.on_input(f)` | `Fn(String) -> M`: recebe o texto novo inteiro a cada mudança. |
| `.on_submit(m)` | Manda `m` no Enter. |
| `.autofocus(pedido: u64)` | Pede o foco. Mude o número para pedir de novo. |
| `.secure(bool)` | Senha: mostra bolinhas e não copia. |
| `.size(Num)`, `.font(Font)`, `.padding(p)`, `.width(l)` | Aparência. |
| `.style(f)` | `Fn(&Theme, Status) -> Style`. Padrão: `input_default`. |

Teclas: setas, Home, End, Backspace, Delete, Enter (submit), Esc (tira o foco) e Ctrl+C/X/V (copia e recorta o campo todo). Clicar fora tira o foco.

### `text_editor` e `Content`

```rust
text_editor(content: &Content) -> TextEditor      // M: Clone
```

Editor de várias linhas. O `Content` fica no estado do app, e o editor edita o conteúdo direto (não há `on_input`).

`TextEditor`:

| Método | Efeito |
|---|---|
| `.on_change(m)` | Manda `m` a cada edição (para marcar "não salvo"). |
| `.size(f32)`, `.font(Font)`, `.padding(p)` | Aparência. |
| `.wrap(bool)` | Quebra de linha automática (ligada por padrão). |
| `.line_numbers(bool)` | Números de linha. |
| `.invisibles(bool)` | Mostra espaço `·`, tab `→` e fim de linha `↵`, e marca os caracteres ocultos. |
| `.markdown(bool)` | Realce de Markdown leve no próprio texto (títulos, negrito, código, listas, tabelas). |
| `.autofocus(u64)` | Como no `text_input`. |
| `.style(f)` | `Fn(&Theme, Status) -> Style`. |

Teclas: setas (com Ctrl, anda por palavra), Home e End (com Ctrl, início e fim do texto), PageUp/PageDown, Tab, Enter e Ctrl+A/C/X/V/Z/Y.

`Content` (todos os métodos tomam `&self`; dá para chamar no `update`):

| Método | Entrada | Saída |
|---|---|---|
| `Content::new()` / `Content::with_text(&str)` | — | um conteúdo novo |
| `.text()` | — | `String` com o texto inteiro |
| `.version()` | — | `u64` que muda a cada edição (não com cursor ou rolagem) |
| `.cursor()` | — | `(linha, coluna)`, contando de 1 |
| `.line_count()` | — | `usize` |
| `.can_undo()` / `.can_redo()` | — | `bool` |
| `.undo()` / `.redo()` | — | — |
| `.go_to_line(n)` | linha, contando de 1 | leva o cursor ao começo dela |
| `.wrap_selection(antes, depois, inicio)` | `&str`, `&str`, `Fn(&str) -> usize` | põe em volta da seleção. Se já estão lá (mesmo por fora de outros, como em `***~~oi~~***`), tira. `inicio` diz onde o texto da linha começa (depois de um `## `): a seleção nunca começa antes. |
| `.edit_lines(f)` | `Fn(&str) -> (bytes_a_tirar, novo_inicio)` | troca o começo de cada linha da seleção (listas, citações) |
| `.map_lines(f)` | `Fn(&str) -> String` | troca o texto de cada linha, num passo só do desfazer |
| `.char_at_cursor()` | — | `Option<char>` logo depois do cursor |
| `.count_chars(f)` | `Fn(char) -> bool` | quantos caracteres passam |
| `.find_char(f)` | `Fn(char) -> bool` | seleciona o próximo que passa. `false` se nenhum. |
| `.find_next(termo, maiusculas)` | `&str`, `bool` | seleciona a próxima ocorrência. `false` se nenhuma. |
| `.replace_next(termo, novo, maiusculas)` | — | troca a seleção atual e seleciona a próxima. `bool` |
| `.replace_all(termo, novo, maiusculas)` | — | `usize`: quantas trocou (um passo só no desfazer) |

`is_invisible(c: char) -> bool`: caractere que não aparece ou parece um espaço comum (controles, larguras zero, BOM, NBSP). Tab e quebra de linha não contam.

### `checkbox`, `toggler`, `radio` (`Toggle`)

```rust
checkbox(label, checked: bool) -> Toggle
toggler(label, on: bool) -> Toggle
radio(label, value: V, selected: Option<V>, on_select: impl Fn(V) -> M) -> Toggle
```

`checkbox` e `toggler`: `.on_toggle(Fn(bool) -> M)` recebe o novo valor.
`radio`: aparece marcado quando `selected == Some(value)`, e o clique manda `on_select(value)`.
Todos: `.size(Num)` e `.color(Color)` (a cor de quando está marcado).

### `pick_list`

```rust
pick_list(options: &[T], selected: Option<T>, on_select: impl Fn(T) -> M) -> PickList
// T: Clone + PartialEq + ToString
```

Lista suspensa. Mostra `selected.to_string()` e abre as outras opções embaixo. Métodos: `.placeholder(..)`, `.width(l)` e `.size(Num)`.

### `slider`

`slider(range: RangeInclusive<f32>, value, on_change: impl Fn(f32) -> M) -> Slider`.
Métodos: `.step(f32)` (arredonda para múltiplos; `0` = contínuo), `.width(l)` e `.color(Color)`.

### `number_input`

`number_input(value: f64, range: RangeInclusive<f64>, step: f64, on_change: impl Fn(f64) -> M) -> Element`: número com botões − e +, preso no intervalo.

### `tabs`

`tabs(labels: &[&str], active: usize, on_select: impl Fn(usize) -> M) -> Element`: abas com sublinhado na ativa. `on_select` recebe o índice.

### `horizontal_rule` / `vertical_rule`

`horizontal_rule(espessura: f32) -> Rule` (e a vertical): linha divisória na cor da borda do tema. `.color(Color)` muda a cor.

### `date_picker` / `date_range_picker` e `Date`

```rust
date_picker(value: Option<Date>, on_select: impl Fn(Date) -> M) -> DatePicker
date_range_picker(start: Option<Date>, end: Option<Date>, on_select: impl Fn(Date, Date) -> M) -> DatePicker
```

Um campo que abre um calendário. No intervalo, o primeiro clique marca o início e o segundo o fim (a ordem se acerta sozinha).
Métodos: `.placeholder(&str)` e `.width(l)`.

`Date { year: i32, month: u8, day: u8 }`:

| Função | Saída |
|---|---|
| `Date::new(y, m, d)` | `Option<Date>` (`None` se o dia não existe) |
| `Date::today()` | hoje, em UTC (perto da meia-noite pode marcar o dia vizinho) |
| `d.weekday()` | `u8`, `0` = domingo |
| `d.to_string()` | texto da data (`Display`) |
| `days_in_month(ano, mes)` | `u8` |

### `color_picker`

`color_picker(value: Color, on_change: impl Fn(Color) -> M) -> ColorPicker`: amostra da cor que abre o seletor (quadro de saturação/brilho, barra de matiz e cores prontas). `.width(l)`.

---

## 5. Sem janela: testes, capturas e o laço manual (`widget.rs`)

### `capture`

```rust
capture(el: &Element<M>, size: Size, scale: f32, theme: &Theme, renderer: &mut Renderer, inspect_at: Option<Point>) -> Vec<u8>
```

- **Entrada:** a árvore, o tamanho lógico, a escala (`1.0`, `2.0` para HiDPI), o tema, um `Renderer::new()` e, opcionalmente, um ponto onde ligar o inspetor.
- **Saída:** os bytes de um PNG. Não precisa de janela nem GPU. Serve para testes, capturas de tela e miniaturas.

```rust
let png = capture(&app.view(), Size::new(800.0, 600.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
std::fs::write("tela.png", png)?;
```

### O laço à mão

Para testes de interação, faça o que o `run` faz por dentro:

| Função | Entrada | Saída |
|---|---|---|
| `layout(el, size, renderer, states, theme)` | árvore e tamanho | `Node`: onde cada widget ficou (`rect`, `children`, `padding`, `margin`, `key`) |
| `draw(el, node, renderer, states, theme, cursor, debug)` | árvore com layout | `CursorIcon` pedido. Desenha em `renderer.pixmap`. |
| `dispatch(el, node, ev, cx: &mut EventCx<M>)` | um `Event` | `Captured::Yes` ou `No`. As mensagens vão para `cx.messages`. |
| `inspect(node, renderer, cursor)` | — | desenha o inspetor (margem laranja, padding verde, conteúdo azul) |

`States` guarda o que sobrevive entre quadros (rolagem, cursor do campo), pela chave do widget. Crie um com `States::default()`. `states.focus` é o widget com foco.
`EventCx` tem os campos `renderer`, `states`, `cursor`, `messages: &mut Vec<M>`, `window: &mut Vec<WindowCmd>`, `redraw` e `clipboard: &mut dyn Clipboard`.
`MemoryClipboard` é uma área de transferência em memória para testes (implementa o trait `Clipboard`, com `read() -> Option<String>` e `write(String)`).

---

## 6. Desenho direto (`Renderer`, `render.rs`)

Usado dentro de um `canvas` ou de um widget próprio. Tudo em pixels lógicos.

| Método | Entrada | Efeito |
|---|---|---|
| `Renderer::new()` | — | um renderer vazio |
| `begin(size, scale, fundo)` | tamanho, escala e cor | começa um quadro limpo |
| `begin_parcial(size, scale, fundo, sujo)` | + a região a refazer | redesenha só `sujo` sobre o quadro anterior. `false` se não dá (aí use `begin`). |
| `end()` | — | fecha um quadro parcial |
| `record()` / `changes() -> Changes` | — | grava o quadro e compara com o anterior. `Changes::None`, `All` ou `Regions(Vec<Rect>)`. |
| `window_size() -> Size` / `scale() -> f32` | — | tamanho lógico e escala atuais |
| `changed_rows() -> Option<(u32, u32)>` | — | linhas que o último quadro mudou (`None` = todas) |
| `measure(text, size, font, max_width) -> Size` | texto | o tamanho que ele ocupa. Com `max_width`, quebra linha. |
| `carets(text, size, font) -> Vec<(byte, x_ini, x_fim)>` | texto | posição de cada caractere da primeira linha |
| `draw_text(text, size, font, color, at, max_width)` | — | escreve |
| `fill_quad(rect, Option<&Background>, Border, Option<Shadow>)` | — | caixa com fundo, borda e sombra |
| `draw_image(&Image, rect, radius: [f32; 4])` | — | imagem esticada e recortada pelos cantos |
| `draw_svg(&Svg, rect, Option<Color>)` | — | SVG, opcionalmente numa cor só |
| `stroke(pontos: &[Point], largura, cor)` | — | linha com pontas e junções redondas |
| `blur(rect, raio, cantos)` | — | desfoca o que já está desenhado ali (vidro) |
| `push_clip(rect, radius)` / `pop_clip()` | — | nada sai de `rect` até o `pop_clip` |
| campo `pixmap` | — | o `tiny_skia::Pixmap` do quadro. Desenho direto nele não entra no `changes()`. |

---

## 7. Widget próprio (`trait Widget<M>`)

```rust
pub trait Widget<M> {
    fn size(&self) -> (Length, Length) { (Length::Shrink, Length::Shrink) }
    fn layout(&self, cx: &mut LayoutCx, limits: Limits) -> Node;
    fn draw(&self, node: &Node, cx: &mut DrawCx);
    fn event(&self, node: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured { Captured::No }
    fn opaque_at(&self, node: &Node, p: Point) -> bool { false }
}
```

| Método | O que faz |
|---|---|
| `size` | Como o widget quer crescer, para o pai distribuir o espaço. |
| `layout` | Recebe `limits.max` e devolve um `Node` (use `Node::leaf(cx.key(), tamanho)` para folhas e `cx.child(i, &filho, limits)` para filhos; `node.place(p)` posiciona o filho relativo ao pai). `Limits::resolve(len, max, conteudo)` resolve um lado. |
| `draw` | Desenha com `cx.renderer`. Também tem `cx.theme`, `cx.states`, `cx.cursor`, `cx.text_color`, `cx.hovered(&rect)`, `cx.overlay(f)` (desenha por cima de tudo) e `cx.cursor_icon`. |
| `event` | Reage. `cx.publish(m)` manda uma mensagem, `cx.redraw = true` pede redesenho sem mensagem, e `cx.states.get_mut::<T>(node.key)` dá estado próprio. Devolva `Captured::Yes` para não deixar o evento descer. |
| `opaque_at` | `true` se o widget cobre `p` para quem está embaixo numa pilha. |

Para virar `Element`, use `Element::new(meu_widget)`. `element.map(f)` converte as mensagens de um componente para as do app.
