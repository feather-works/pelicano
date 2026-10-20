//! Ícones de traço no estilo lucide (24×24), analisados uma vez só.
use pelicano::Svg;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn corpo(nome: &str) -> &'static str {
    match nome {
        "pena" => r#"<path fill="black" fill-opacity="0.45" d="M12.67 19a2 2 0 0 0 1.416-.588l6.154-6.172a6 6 0 0 0-8.49-8.49L5.586 9.914A2 2 0 0 0 5 11.328V18a1 1 0 0 0 1 1z"/><path d="M16 8 2 22"/><path d="M17.5 15H9"/>"#,
        "mais" => r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#,
        "pasta" => r#"<path fill="black" stroke-width="1" d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#,
        "estrela" => r#"<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>"#,
        "estrela_cheia" => r#"<path fill="black" d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>"#,
        "relogio" => r#"<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>"#,
        "tarefa" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="m9 12 2 2 4-4"/>"#,
        "arquivo" => r#"<rect fill="black" width="20" height="5" x="2" y="3" rx="1"/><path fill="black" fill-opacity="0.85" d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path stroke-opacity="0" d="M10 12h4"/>"#,
        "lixeira" => r#"<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/>"#,
        "filtro" => r#"<path d="M3 6h18"/><path d="M7 12h10"/><path d="M10 18h4"/>"#,
        "busca" => r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#,
        "sol" => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#,
        "lua" => r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#,
        "sino" => r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>"#,
        "abaixo" => r#"<path d="m6 9 6 6 6-6"/>"#,
        "imagem" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>"#,
        "compartilhar" => r#"<circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><path d="m8.59 13.51 6.83 3.98"/><path d="m15.41 6.51-6.82 3.98"/>"#,
        "reticencias" => r#"<circle fill="black" cx="12" cy="12" r="1.2"/><circle fill="black" cx="19" cy="12" r="1.2"/><circle fill="black" cx="5" cy="12" r="1.2"/>"#,
        "calendario" => r#"<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/>"#,
        "salvo" => r#"<circle fill="black" fill-opacity="0.55" stroke="none" cx="12" cy="12" r="10"/><path d="m8 12 3 3 5-6"/>"#,
        "negrito" => r#"<path d="M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"/>"#,
        "italico" => r#"<path d="M19 4h-9"/><path d="M14 20H5"/><path d="M15 4 9 20"/>"#,
        "sublinhado" => r#"<path d="M6 4v6a6 6 0 0 0 12 0V4"/><path d="M4 20h16"/>"#,
        "tachado" => r#"<path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><path d="M4 12h16"/>"#,
        "codigo" => r#"<path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/>"#,
        "lista" => r#"<path d="M3 12h.01"/><path d="M3 18h.01"/><path d="M3 6h.01"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M8 6h13"/>"#,
        "numerada" => r#"<path d="M10 12h11"/><path d="M10 18h11"/><path d="M10 6h11"/><path d="M4 10h2"/><path d="M4 6h1v4"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/>"#,
        "link" => r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
        "tabela" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/><path d="M9 3v18"/><path d="M15 3v18"/>"#,
        "alvo" => r#"<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle fill="black" cx="12" cy="12" r="2"/>"#,
        "barras" => r#"<rect fill="black" stroke="none" x="3" y="13" width="4.5" height="8" rx="1"/><rect fill="black" stroke="none" x="9.75" y="8" width="4.5" height="13" rx="1"/><rect fill="black" stroke="none" x="16.5" y="3" width="4.5" height="18" rx="1"/>"#,
        "clipe" => r#"<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>"#,
        "pagina" => r#"<path fill="black" stroke="none" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/>"#,
        "pagina_linhas" => r#"<path d="M8 12h8"/><path d="M8 16h8"/><path d="M8 8h3"/>"#,
        "notas" => r#"<rect width="16" height="18" x="4" y="3" rx="2"/><path d="M8 8h8"/><path d="M8 12h8"/><path d="M8 16h5"/>"#,
        "expandir" => r#"<path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="M21 3l-7 7"/><path d="M3 21l7-7"/>"#,
        _ => r#"<circle cx="12" cy="12" r="10"/>"#,
    }
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
