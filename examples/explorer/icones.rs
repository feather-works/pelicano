//! Ícones de traço no estilo lucide (24×24), analisados uma vez só.
use pelicano::Svg;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ARQUIVO: &str = r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/>"#;

fn corpo(nome: &str) -> String {
    let s = match nome {
        "logo" => r#"<path fill="black" stroke="none" d="M12 1.5c-.9 2.3-1.5 3.8-2.5 6 .6.7 1.4 1.4 2.6 2.3-1.3-.5-2.2-1.1-2.9-1.7C7.7 11.5 5.4 15.9 1.8 22.5c2.7-1.6 4.8-2.5 6.7-2.9-.1-.4-.1-.8-.1-1.2.1-1.9 1.7-3.4 3.6-3.3 1.9.1 3.4 1.7 3.4 3.6 0 .3 0 .6-.1.9 1.9.4 4 1.3 6.9 2.9-.6-1.1-1.1-2-1.5-2.8-.8-.6-1.6-1.3-3.1-2 1.1.3 1.9.6 2.5 1C17.6 13 15.6 8.9 12 1.5z"/>"#,
        "casa" => r#"<path fill="black" d="M3 10a2 2 0 0 1 .7-1.5l7-6a2 2 0 0 1 2.6 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
        "pasta" => r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#,
        "pasta_cheia" => r#"<path fill="black" stroke-width="1" d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#,
        "grade" => r#"<rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/>"#,
        "engrenagem" => r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#,
        "monitor" => r#"<rect width="20" height="14" x="2" y="3" rx="2"/><path d="M7 21h10"/><path d="M8 13h8"/>"#,
        "terminal" => r#"<path d="m4 17 6-6-6-6"/><path d="M12 19h8"/>"#,
        "grafico" => r#"<path d="M3 3v18h18"/><path d="m6 15 4-6 4 5 6-8"/>"#,
        "pulso" => r#"<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>"#,
        "marcador" => r#"<path d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/>"#,
        "arquivo" => ARQUIVO,
        "texto" => return format!(r#"{ARQUIVO}<path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>"#),
        "codigo" => return format!(r#"{ARQUIVO}<path d="m10 13-2 2 2 2"/><path d="m14 17 2-2-2-2"/>"#),
        "foto" => return format!(r#"{ARQUIVO}<circle cx="10" cy="12" r="2"/><path d="m20 17-1.3-1.3a2.4 2.4 0 0 0-3.4 0L9 22"/>"#),
        "md" => return format!(r#"{ARQUIVO}<path d="M8 18v-5l2 2 2-2v5"/><path d="m14 16 1.5 1.5L17 16"/>"#),
        "download" => r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>"#,
        "imagem" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/>"#,
        "documento" => return format!(r#"{ARQUIVO}<path d="M16 13H8"/><path d="M16 17H8"/>"#),
        "video" => r#"<path d="m16 13 5.2 3.5a.5.5 0 0 0 .8-.4V7.9a.5.5 0 0 0-.8-.4L16 10.5"/><rect x="2" y="6" width="14" height="12" rx="2"/>"#,
        "musica" => r#"<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>"#,
        "lixeira" => r#"<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/>"#,
        "x" => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
        "mais" => r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#,
        "esquerda" => r#"<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>"#,
        "direita" => r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#,
        "cima" => r#"<path d="m5 11 7-7 7 7"/><path d="M12 4v17"/><path d="M8 21v-6"/><path d="M16 21v-6"/>"#,
        "busca" => r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#,
        "lista" => r#"<path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M3 6h.01"/><path d="M3 12h.01"/><path d="M3 18h.01"/>"#,
        "reticencias" => r#"<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#,
        "ajustes" => r#"<path d="M21 4h-7"/><path d="M10 4H3"/><path d="M21 12h-9"/><path d="M8 12H3"/><path d="M21 20h-5"/><path d="M12 20H3"/><path d="M14 2v4"/><path d="M8 10v4"/><path d="M16 18v4"/>"#,
        "cpu" => r#"<rect width="16" height="16" x="4" y="4" rx="2"/><rect width="6" height="6" x="9" y="9" rx="1"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/>"#,
        "memoria" => r#"<rect width="20" height="12" x="2" y="6" rx="2"/><path d="M6 10v4"/><path d="M10 10v4"/><path d="M14 10v4"/><path d="M18 10v4"/>"#,
        "disco" => r#"<path d="M2 12h20"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><path d="M6 16h.01"/><path d="M10 16h.01"/>"#,
        "rede" => r#"<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>"#,
        "minimizar" => r#"<path d="M5 12h14"/>"#,
        "maximizar" => r#"<rect width="16" height="16" x="4" y="4" rx="1"/>"#,
        _ => r#"<circle cx="12" cy="12" r="10"/>"#,
    };
    s.to_string()
}

/// O ícone pelo nome; a cor vem do `svg(..).color(..)` de quem desenha.
pub fn icone(nome: &'static str) -> Svg {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Svg>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    c.entry(nome)
        .or_insert_with(|| {
            let fonte = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
                corpo(nome)
            );
            Svg::from_bytes(fonte.as_bytes()).expect("ícone válido")
        })
        .clone()
}
