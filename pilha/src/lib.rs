//! Motor de empilhar fatias (sprite stacking) sobre o pelicano: o objeto 2.5D é uma pilha de fatias
//! horizontais que a GPU abre e desenha de baixo para cima, giradas pelo giro da câmera, achatadas
//! pela inclinação e subindo um pouco a cada uma, com a luz presa à tela (3 tons por material).
//!
//! [`Volume`]: material e normal por voxel, montados de uma função de forma (em segundo plano, se
//! quiser). [`Modelo`]: o volume virado atlas do pelicano, com a paleta e o contorno.
//! [`Instancia`], [`Camera`] e [`desenhar`]: descarta o que está fora da tela, ordena de trás para a
//! frente e manda cada pilha para o `Renderer::draw_stacks`.
//!
//! Fica de fora: chão, mapa, animação por quadros (pose diferente = outro volume) e as linhas de
//! contorno por dentro (uma parte na frente da outra).
mod cena;
mod volume;

pub use cena::{desenhar, Camera, Instancia, Modelo, LUZ};
pub use volume::Volume;
