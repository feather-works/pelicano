//! Xadrez pixelado do pack de jogos, sozinho numa janela; o jogo está em `pack/xadrez.rs`.
//! `cargo run --release --example xadrez_pixel`; `-- --capturar x.png` só gera o PNG.
#[path = "pack/estilo.rs"]
mod estilo;
#[path = "pack/xadrez.rs"]
mod xadrez;

fn main() -> Result<(), pelicano::Error> {
    xadrez::principal()
}
