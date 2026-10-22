//! Demo tool to test Spanish hyphenation on any word.
//!
//! Run with no arguments to see sample words:
//! ```text
//! cargo run -p designcraft-compose --example hyph_demo
//! ```
//!
//! Or pass words as arguments:
//! ```text
//! cargo run -p designcraft-compose --example hyph_demo constelación extraordinario ciberseguridad
//! ```

use designcraft_compose::hyphen::{Limits, es};

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let lim = Limits::default();

    if words.is_empty() {
        println!("Guionado silábico en español (Reglas RAE & Límites Tipográficos):");
        println!("{:-<50}", "");
        let sample = [
            "declaración",
            "construcción",
            "estrella",
            "cuidado",
            "farmacia",
            "teatro",
            "poesía",
            "países",
            "aéreo",
            "alcohol",
            "alhaja",
            "desarmar",
            "deshacer",
            "inútil",
            "sublunar",
            "transporte",
            "ciberseguridad",
            "geolocalización",
            "escríbemelo",
            "diciéndoselo",
            "interactivo",
            "superestructura",
            "transatlántico",
            "pingüino",
            "averigüéis",
        ];
        for w in sample {
            let pts = es::hyphen_points(w, &lim);
            let breaks = es::word_breaks(w);
            let hy = es::hyphenate_word(w, &lim);
            println!("{w:18} → {hy:24} (cortes: {breaks:?} | permitidos: {pts:?})");
        }
    } else {
        for w in &words {
            let pts = es::hyphen_points(w, &lim);
            let breaks = es::word_breaks(w);
            let hy = es::hyphenate_word(w, &lim);
            println!("{w:18} → {hy:24} (cortes: {breaks:?} | permitidos: {pts:?})");
        }
    }
}
