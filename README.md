# Pelicano

Biblioteca de interface da suíte FeatherWorks, em Rust, **sem Iced**. O modelo é o
mesmo que a suíte já usa (estado, `update`, `view`), mas com tudo aberto para
personalizar — a ideia é chegar perto do que HTML + CSS permitem.

![vitrine](docs/vitrine.png)

## O que ele faz que o Iced 0.13 travava

- **Janela sem moldura de verdade**: arrasta (`WindowCmd::Drag`) e redimensiona pela borda (`WindowCmd::DragResize`).
- **Janela translúcida** com cantos arredondados, e desfoque do desktop atrás (`Settings { transparent, blur }`; KDE/Windows/macOS — o GNOME ignora o desfoque).
- **Vidro em qualquer caixa**: `Style::backdrop_blur` (o `backdrop-filter` do CSS) e o preset `Style::glass(raio)`.
- **Recorte pelo raio**: `container(...).clip(true)` corta imagem, lista e qualquer filho nos cantos arredondados.
- Raio por canto, borda, sombra desfocada e gradiente em todo contêiner; **padding e margin**.
- Hover que respeita camadas (o botão debaixo de um menu aberto não acende) e cursor do mouse que muda sozinho.
- **Inspetor de layout** como o do navegador — desligado por padrão: `Settings { debug: true }` libera o F12 (ou `PELICANO_DEBUG=1`). Margem em laranja, padding em verde, conteúdo em azul, com o tamanho.
- **Captura sem janela**: `capture(&app.view(), tamanho, escala, &tema, &mut Renderer::new(), None)` devolve o PNG — para testes e miniaturas.
- `Widget` é um trait pequeno e público, e todo estilo é uma closure `Fn(&Theme, Status) -> Style`; `canvas` para desenho livre.

![inspetor](docs/inspetor.png)

## Widgets

`text`, `button`, `container`, `row!`/`column!` (com `.wrap()`), `stack!`, `Space`,
`mouse_area` (com duplo clique), `opaque`, `scrollable` (roda e polegar arrastável),
`text_input` (com modo senha), `checkbox`, `toggler`, `pick_list`, `slider`,
`progress_bar`, `tooltip`, `image` (Fill/Contain/Cover), `svg` (com cor), `horizontal_rule`/`vertical_rule`, `canvas`.

## Exemplo

```sh
cargo run --example vitrine                      # abre a janela (F12 = inspetor)
cargo run --example vitrine -- --capturar a.png  # só gera o PNG
cargo run --example xadrez                       # xadrez com temas (Clássico, Verde, Vidro)
```

![xadrez](docs/xadrez-vidro.png)

## Como funciona

winit abre a janela; o quadro é desenhado na CPU com tiny-skia (texto com
cosmic-text, SVG com resvg) e o wgpu só leva a imagem pronta para a tela com alfa
pré-multiplicado — é isso que dá a transparência no Wayland.

## Ainda falta

Seletor de data, área de texto multilinha, seleção e área de transferência no
`text_input`, assinaturas de tempo (`every`), executor tokio (para `reqwest`),
acessibilidade (AccessKit) e redesenho parcial.

Licença GPL-3.0-only.
