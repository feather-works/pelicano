# Pendente no pelicano

## Próximo: texto que não cabe (pedido em 2026-10-20)

Regra: texto nunca quebra letra por letra. Quando não cabe, ele cresce na vertical (só onde a caixa
pode crescer, nunca num cabeçalho), vira "…" ou some.

1. ~~Texto de botão não quebra por padrão~~ — feito: botão `Shrink` mede o rótulo sem limite de largura.
2. ~~`row`/`column` nunca apertam um filho abaixo do mínimo do conteúdo~~ — feito: o cosmic-text
   quebra só entre palavras (`Wrap::Word`) e `text` `Shrink` nunca fica mais estreito que a maior
   palavra; o que não cabe passa da caixa (não é cortado, falta o pai cortar).
3. ~~`Text::ellipsis()`~~ — feito.
4. Botão com ícone e texto encolhe para só o ícone quando falta espaço. (falta)
5. Quebra em várias linhas só quando o app pede. (falta: `text` continua com `wrap: true`; trocar
   o padrão quebraria parágrafos do feathernotes, do featherlauncher e do markdown.)

Causa hoje: `text` nasce com `wrap: true`, a `row` dá a um filho `Shrink` menos que a largura
natural dele, e o cosmic-text quebra dentro da palavra. Já existem `responsive(|tamanho| ...)` e
`row(...).wrap()`.

Antes de mudar os padrões: conferir o featherlauncher, o feathernotes e os exemplos.

## Migração da suíte para o pelicano (planejada em 2026-10-20)

Ordem: FeatherCode → FeatherDocs → FeatherPDF → FeatherSlide → FeatherSheet → FeatherCAD.

| App | Interface hoje | Linhas de interface | Canvas |
|---|---|---|---|
| Docs | Iced + feather-ui | 7,2 mil (12 arquivos) | 1 |
| Code | Iced + feather-ui + iced_term (o pelicano já tem terminal) | 8,4 mil (10) | 1 |
| PDF | Iced + feather-ui | 12,7 mil (42) | 9 |
| Slide | Iced + feather-ui | 22 mil (41) | 6 |
| Sheet | Iced + feather-ui | 67,5 mil (142); o `feathersheet-gui` tem 81 mil | 9 |
| CAD | Tauri + web (11 mil linhas de JS/TS) | reescrever a tela em Rust | — |

- O Sheet não é troca rápida: o motor (core/io/vba) fica, mas a interface é a maior da lista,
  e a grade desenhada à mão precisa continuar rápida em planilha enorme.
- Antes do Slide, do Sheet e do CAD: `fill_path` e curvas na GPU (ver abaixo).
- O feather-ui (ribbon, tema, painéis) será substituído depois, "quando tudo tiver certo".
  Decidir, ao começar pelo Code, de onde os apps tiram a aparência Feather.

## GPU (continuação da 0.1.7)

- Feito (21/10): `fill_path` e `stroke_path` de qualquer tipo vão à GPU rasterizados só na caixa
  deles e guardados como textura (cache `caminhos`; mover por pixels inteiros reaproveita).
  Falta: a rasterização ainda é na CPU numa thread; com muitos objetos mudando de forma (giro,
  posição fracionária com zoom) cada quadro rasteriza de novo. Medir e, se pesar, lyon na GPU.
- Ainda caem na CPU: vidro/desfoque, camadas com rotação ou escala, caminho maior que 2048²,
  imagem sem suavizar.
- Sombra só aproximada; só o recorte arredondado mais de dentro corta os cantos.
- Testar à mão rolar e redimensionar na GPU.
- Feito (12/11, #7): rodada cuja instância do wgpu não tem backend (Intel Ivy Bridge no Linux: sem
  Vulkan e com o contexto OpenGL recusado) é pulada antes do `create_surface`, que entrava em
  pânico; a janela abre pela CPU. Conferido só simulando a instância vazia; falta a confirmação
  de quem tem a placa, e publicar a versão para o FeatherSheet e o Launcher.

## Desempenho

- Feito (21/10): sem GPU, a `gpu::Lista` é pintada em faixas, uma thread por faixa
  (`render/faixas.rs`). Numa thread o pintor ainda é ~2× mais lento que o tiny-skia; o ganho na
  tabela cheia foi 2,4→1,9 ms (escala 1) e 7,8→3,3 ms (escala 2). Falta testar janela real com
  `PELICANO_GPU=nenhuma`.
- O preparo da lista (gravar, medir texto, rasterizar caminhos) é numa thread só: é o que pesa
  ao arrastar muitos objetos no FeatherSlide (quadros de 300–450 ms). Perfilar com `perf`.

## Para os apps

- Um fluxo "a cada quadro", como o `window::frames()` do Iced, e a taxa do monitor.
- Saber quando um quadro chegou à tela, para contar FPS.
- Caneta como eventos `Pen*`, trazida do feather-ui.
- Lista virtual, cabeçalho fixo de tabela, fonte padrão do app, estilo de hover, SVG
  multicolorido, peso 500 caindo em serifa, `Padding` de `[t, r, b, l]`.
- Cor de link no texto rico sem preparar o texto de novo a cada clique.

## Faltas achadas na porta do FeatherSlide (2026-10-21)

Cada uma está marcada com `shortcut:` em `featherslide/crates/featherslide-pelicano`.

- ~~`pick_list` sem estilo nem padding~~ — feito: `.style(Fn(&Theme, Status))` e `.padding(..)` (a seta continua na cor `muted`).
- ~~`max_height` em contêiner e `scrollable`~~ — feito.
- ~~Barra de rolagem fina~~ — feito: `scrollbar_width`, `scrollbar_spacing` (trilho ao lado) e `scrollbar_color`.
- ~~`text_input` com cor do texto de exemplo e da seleção~~ — feito: `placeholder_color`, `selection_color`; o campo ganhou seleção (Shift+setas, Ctrl+A, arrastar).
- ~~Texto que não cabe quebra no meio da palavra ("Substitui/r" na faixa)~~ — feito.
- Roda do mouse: a rolagem suave reparte um passo em vários eventos; quem conta passos (trocar
  de slide) precisa acumular. Dar o passo inteiro também, ou um evento "passo".
- Medir o Iced e o pelicano no mesmo teste (arrastar muitos objetos no FeatherSlide).

## Arrumação

- `map_de_assinatura_recusa_closure_que_captura` falha em release (testa um `debug_assert`).
- Exemplo csv: o "Δ ms" do topo faz a janela redesenhar sem parar com o mouse em cima.
