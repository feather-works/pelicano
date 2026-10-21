//! Paciência do pack de jogos, sozinha numa janela; o jogo está em `pack/paciencia.rs`.
//! `cargo run --release --example paciencia`; `-- --capturar x.png` só gera o PNG.
#[path = "pack/estilo.rs"]
mod estilo;
#[path = "pack/cartas.rs"]
mod cartas;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/paciencia.rs"]
mod paciencia;

fn main() -> Result<(), pelicano::Error> {
    paciencia::principal()
}
