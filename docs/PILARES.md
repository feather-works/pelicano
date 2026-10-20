# Pilares de um motor de GUI: onde o pelicano está

Checklist dos seis pilares de um motor de interface, com o estado do pelicano verificado no código de `src/` (em 2026-10-20). Legenda: ✅ feito, 🟡 parcial, ❌ falta. Onde o item não tem código, a coluna "Onde no código" aponta a ausência.

## 1. SO / Janela

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Janela e loop de eventos (winit 0.30) | ✅ `ApplicationHandler` próprio | `src/app.rs` (`run`, `impl ApplicationHandler`); `Cargo.toml` | — |
| Janela sem moldura, arrastar e redimensionar | ✅ `WindowCmd::Drag` e `DragResize` | `src/app.rs` (`WindowCmd`) | — |
| Janela transparente e desfoque do desktop | ✅ `Settings { transparent, blur }`; wgpu só apresenta o quadro com alfa | `src/app.rs` (`Settings`, `present`) | GNOME ignora o desfoque (limite do SO) |
| HiDPI / escala | ✅ eventos em pixels lógicos; quadro na escala física; cache de texto zera ao mudar a escala | `src/app.rs` (`logical_size`, `window_event`); `src/render.rs` (`begin`) | `ScaleFactorChanged` só pede redesenho; sem teste com monitores de escalas diferentes |
| Ícone do cursor | ✅ `CursorIcon` por widget, aplicado só quando muda | `src/widget.rs` (`DrawCx.cursor_icon`); `src/app.rs` (`redraw`) | — |
| Múltiplos monitores | ❌ nenhuma API de monitor usada | — | Listar monitores e escolher onde abrir a janela |
| Múltiplas janelas | ❌ `Runtime` guarda uma `window: Option` | `src/app.rs` (`Runtime`) | Modelo de várias janelas no `App` |
| Área de transferência | ❌ | — | Crate de clipboard (ex.: arboard) ligado ao `text_input` |
| Diálogos nativos de arquivo | ❌ `pickers.rs` são widgets internos (data e cor), não diálogo do SO | `src/pickers.rs` | Crate de diálogo (ex.: rfd) como `Task` |
| Menus nativos, bandeja, arrastar e soltar | ❌ menus são widgets internos; sem `DroppedFile` | `src/controls.rs` (`PickList`) | Tudo que é nativo |

## 2. Árvore de estado

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Modelo Elm (`update` e `view`) | ✅ trait `App` com `update`, `view`, `theme` e `on_event` | `src/app.rs` (`trait App`) | — |
| Tarefas assíncronas | ✅ `Task::none/done/perform/run/window/batch/map/abortable` | `src/app.rs` (`Task`) | — |
| Reconstrução da view | ✅ `view()` é chamado a cada evento e a cada quadro | `src/app.rs` (`event`, `redraw`) | — |
| Reconciliação com diff | ❌ não há diff: a árvore inteira é refeita e relida | — | Diff, ou IDs estáveis para widgets que mudam de posição |
| Estado dos widgets | ✅ `States` (`Box<dyn Any>`) chaveado pelo hash do caminho na árvore | `src/lib.rs` (`States`); `src/widget.rs` (`child_key`, `LayoutCx::key`) | Estado segue a posição, não a identidade: trocar a ordem dos filhos troca o estado |
| Foco como estado global | ✅ `States.focus: Option<u64>` | `src/lib.rs` (`States`) | — |
| Widget customizado público | ✅ trait `Widget<M>` pública; `canvas` para desenho livre | `src/widget.rs` (`trait Widget`, `canvas`) | Boilerplate de mensagens: cada ação exige variante em `Msg` |

## 3. Layout

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Duas passagens (restrições descem, tamanho sobe) | ✅ `Limits` e `Node`; `LayoutCx::child` mede e `place` posiciona | `src/widget.rs` (`Limits`, `Node`, `LayoutCx`) | Cache de layout entre quadros |
| Flex row e column | ✅ `row!`/`column!` com espaçamento, alinhamento e padding | `src/widget.rs` (`Flex`, `row`, `column`) | — |
| Flex com quebra de linha | 🟡 `.wrap()` só para linha (row); coluna não quebra | `src/widget.rs` (`Flex::wrap`, `layout_wrap`) | Quebra para coluna |
| Stack (camadas) | ✅ `stack!`, último filho por cima | `src/widget.rs` (`Stack`) | — |
| Padding e margin | ✅ em `container`, `Flex` e `Button` | `src/widget.rs` (`Container::padding/margin`); `src/lib.rs` (`Padding`) | — |
| Clip com cantos arredondados | ✅ `container.clip(true)` e `Scrollable` cortam pelo raio | `src/widget.rs` (`Container::clip`, `Scrollable::draw`); `src/render.rs` (`push_clip`, `clip_mask`) | — |
| Rolagem com recorte | 🟡 `scrollable` vertical (roda e polegar); sem rolagem horizontal | `src/widget.rs` (`Scrollable`, `Event::Wheel { dy }`) | Eixo horizontal |
| Grid | ❌ | — | Trilhas, áreas e spans |
| Tamanho e alinhamento | ✅ `Length::{Fixed, Fill, Shrink}`, `Align` | `src/lib.rs` (`Length`, `Align`) | — |
| Inspetor de layout | ✅ mostra margin, padding e conteúdo; F12 ou `PELICANO_DEBUG=1` | `src/widget.rs` (`inspect`); `src/app.rs` (F12, `PELICANO_DEBUG`) | Só liga com `Settings { debug: true }` |

## 4. Renderização

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Desenho na CPU (tiny-skia) | ✅ todo o quadro sai do `Renderer` | `src/render.rs` (`Renderer`, `begin`) | Caminho de GPU para desenhar |
| Apresentação na GPU (wgpu) | 🟡 wgpu só sobe a textura pronta e desenha um triângulo | `src/app.rs` (`Gpu`, `present`) | Rasterizar na GPU |
| Retângulos arredondados | ✅ raio por canto, borda | `src/render.rs` (`fill_quad`, `rounded_path`) | — |
| Gradientes | 🟡 linear ✅ (`Background::Linear`); radial ❌ | `src/lib.rs` (`Background`); `src/render.rs` (`fill_quad`) | Gradiente radial |
| Sombra desfocada | ✅ com cache por tamanho, raio, desfoque e cor | `src/render.rs` (`shadow`, `ShadowKey`) | Cache é zerado inteiro ao passar de 256 entradas |
| Desfoque do fundo (backdrop) | ✅ `Style::backdrop_blur` e `Style::glass` | `src/render.rs` (`blur`); `src/widget.rs` (`Style`) | — |
| Imagens | ✅ `image()` com Fill, Contain, Cover e raio | `src/render.rs` (`draw_image`); `src/widget.rs` (`ImageView`) | — |
| SVG | ✅ resvg, com cache por tamanho e cor | `src/render.rs` (`draw_svg`); `src/widget.rs` (`SvgView`) | — |
| Bézier | 🟡 cúbicas só nos cantos; a API pública tem só `stroke` por polilinha | `src/render.rs` (`push_rounded_path`, `stroke`) | Caminhos e curvas públicos |
| Batching | ❌ cada `fill_quad` vai direto ao pixmap | `src/render.rs` | Agrupar desenhos |
| Damage tracking (retângulos sujos) | ❌ `begin` limpa o pixmap inteiro; todo evento redesenha o quadro | `src/render.rs` (`begin`); `src/app.rs` (`event`, `redraw`) | Redesenhar só a área suja |

## 5. Tipografia

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Shaping (cosmic-text) | ✅ `Shaping::Advanced` | `src/render.rs` (`buffer`) | — |
| Ligaduras e kerning | 🟡 vêm do cosmic-text; sem teste visual | `src/render.rs` | Teste com fonte que tenha ligaduras |
| Emoji | 🟡 depende de fonte com emoji no sistema; sem fallback explícito | `src/render.rs` | Fallback e teste |
| Quebra de linha | ✅ `text` com wrap e largura máxima; `no_wrap()` | `src/render.rs` (`measure`, `draw_text`); `src/widget.rs` (`Text`) | — |
| Métricas (altura de linha e linha de base) | 🟡 altura de linha fixa em 1.3 × tamanho; sem linha de base exposta | `src/render.rs` (`buffer`, `Metrics::new`) | `line_height` configurável e baseline |
| Clique para posicionar o cursor entre letras | ✅ no `text_input` | `src/render.rs` (`carets`); `src/widget.rs` (`TextInput::event`) | — |
| Seleção de texto | ❌ | — | Seleção por arrasto e duplo clique |
| Editor multilinha | ❌ `text_input` é de uma linha só | `src/widget.rs` (`TextInput`) | Linhas, caret vertical, seleção |
| Atlas de glifos | ❌ cache de glifos do `SwashCache` por buffer, sem atlas | `src/render.rs` (`Renderer.swash`) | Atlas (útil principalmente com GPU) |
| Fontes próprias | 🟡 `Font` aceita só nome de família do sistema | `src/render.rs` (`Font`) | Carregar fonte a partir de bytes |

## 6. Eventos, foco, IME e acessibilidade

| Item | Pelicano hoje | Onde no código | Falta |
|---|---|---|---|
| Hit-test de frente para trás | ✅ `opaque_at`; `Stack` e `opaque` bloqueiam o que está embaixo | `src/widget.rs` (`opaque_at`, `Stack`, `Opaque`) | — |
| Overlays desenhados por último | ✅ `DrawCx::overlay` | `src/widget.rs` (`draw`, `overlay`) | — |
| Propagação com parada | ✅ `Captured::Yes` interrompe a entrega | `src/lib.rs` (`Captured`); `src/widget.rs` (`dispatch`) | — |
| Fase de captura separada da de bolha | ❌ a entrega é uma única passagem | `src/widget.rs` (`dispatch`) | Duas fases, se houver necessidade |
| Foco | 🟡 `States.focus`; clique fora solta e Esc solta; só `text_input` recebe foco | `src/lib.rs` (`States`); `src/widget.rs` (`dispatch`, `TextInput::event`) | Foco em botões e outros widgets |
| Navegação por Tab | ❌ nenhuma tecla Tab é tratada | — | Ordem de foco pela árvore |
| Atalhos globais | ✅ `App::on_event` recebe todo evento | `src/app.rs` (`on_event`) | — |
| Teclado no campo de texto | 🟡 setas, Home, End, Backspace, Delete e Enter; Ctrl+C, V, X e A são ignorados | `src/widget.rs` (`TextInput::event`) | Atalhos de edição |
| Duplo clique e clique direito | ✅ `mouse_area` com `on_double_click` e `on_right_press` | `src/widget.rs` (`MouseArea`) | — |
| Roda do mouse | 🟡 vertical ✅; horizontal ❌ (`dx` é ignorado) | `src/app.rs` (`window_event`); `src/widget.rs` (`Scrollable::event`) | Eixo horizontal |
| IME (composição, ex.: ~ + a = ã) | ❌ `Event` não tem composição; sem `set_ime_allowed` | `src/lib.rs` (`Event`) | Eventos de pré-edição do winit, posição do candidato, campo que trata pré-edição |
| Acessibilidade (AccessKit, UIA, AT-SPI) | ✅ árvore montada no desenho, só com leitor ligado; clicar e focar pelo leitor | `src/access.rs`; `src/app.rs` (`acessibilidade`) | Seletores de data e cor, menus abertos e tooltips ainda não entram; slider sem incrementar pelo leitor |

## Onde Tk, Iced e Slint pecam e como o pelicano responde

| Engine | Paradigma | Renderização | Ponto forte | Calcanhar de Aquiles | Pelicano |
|---|---|---|---|---|---|
| Tk | Imperativo/retido | APIs nativas legadas/X11/GDI | Leveza, maturidade | Estética datada, sem GPU, estado manual | Elm (`update`/`view`) com estado por caminho, sem estado manual solto. Sombra, desfoque e transparência prontos. Mas é jovem (0.1.0), com um só app de referência, e sem Tab, IME, clipboard nem a11y. |
| Iced | Elm reativo | WGPU + Tiny-Skia | Rust puro, sem DSL | Boilerplate de mensagens, widgets custom difíceis, API instável | Mesmo modelo Elm, então o boilerplate de `Msg` continua. Widgets custom saem da trait pública `Widget`. A API ainda muda. Desenho na CPU: sem damage tracking nem batching, e o quadro inteiro é refeito a cada evento. Menus, bandeja e DnD ausentes, como no Iced. |
| Slint | DSL compilada | OpenGL/Vulkan/Skia/Software | Performance, embarcados | DSL própria, ecossistema pequeno, licença dual | Sem DSL: tudo é Rust. Sem a limitação da DSL, mas sem o ganho de performance do código compilado para embarcado. Licença MIT. Ecossistema é só a suíte FeatherWorks. |

## Próximos passos

Ordem de prioridade, do que mais pesa hoje para o que depende de outros itens:

1. **Custo por quadro e resize travando.** Cada evento refaz layout e view, e `begin` limpa o pixmap inteiro. Medir e cortar o trabalho repetido (por exemplo, não redesenhar quando o hover não muda, e reaproveitar o pixmap no resize).
2. **IME.** Precisa de eventos de composição em `Event` e de posicionar o candidato. Depende de o campo de texto tratar pré-edição.
3. **Editor multilinha.** Hoje o `text_input` é de uma linha. Exige seleção (arrasto e duplo clique) e caret vertical.
4. **AccessKit.** Árvore de papéis e rótulos gerada pelos widgets. Cada widget precisa de um rótulo antes.
5. **Renderizador na GPU.** Hoje a GPU só apresenta. Rasterizar na GPU tira o custo de CPU do quadro.
6. **Área de transferência.** Crate de clipboard ligado a Ctrl+C, V e X no `text_input`. Melhor depois da seleção.
7. **Foco com Tab.** Ordem de foco pela árvore. Hoje só o `text_input` recebe foco.
8. **Damage tracking.** Redesenhar só os retângulos sujos. Depende de caches estáveis e de o item 1 estar resolvido.
9. **Grid.** Trilhas e spans no layout. Não bloqueia nada, mas faltará para telas com alinhamento em duas dimensões.

Observação: o README, na seção "Ainda falta", cita seletor de data, mas ele existe em `src/pickers.rs` (`date_picker`, `date_range_picker`). Vale atualizar.
