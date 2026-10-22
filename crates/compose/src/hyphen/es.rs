//! Spanish hyphenation based on RAE orthographic syllabification rules.
//!
//! Spanish word division at line breaks is fundamentally phonological and rule-based
//! (Ortografía de la Real Academia Española, Cap. V):
//! 1. **Vocalic nuclei**:
//!    - Open vowels (*a, e, o, á, é, ó*) vs closed unaccented (*i, u, ü*) vs closed tonic (*í, ú*).
//!    - Diphthongs and triphthongs are indivisible (*cau-sa*, *via-je*, *cui-da-do*, *buey*, *a-ve-ri-güéis*).
//!    - Hiatuses are divisible (*te-a-tro*, *le-er*, *pa-ís*, *bú-ho*, *o-í-do*).
//! 2. **Consonant clusters**:
//!    - Inseparable digraphs: `ch`, `ll`, `rr` (*co-che*, *ca-lle*, *tie-rra*).
//!    - Inseparable onsets (plosive/fricative + liquid): `br`, `cr`, `dr`, `fr`, `gr`, `kr`, `pr`, `tr`,
//!      `bl`, `cl`, `fl`, `gl`, `kl`, `pl` (*ha-blar*, *a-brir*, *cua-dro*, *pla-ta*).
//!    - Sequences of 1, 2, 3 and 4 consonants (*me-sa*, *ac-ción*, *cons-tan-te*, *cons-truc-ción*).
//!    - Silent `qu` and `gu` before `e`/`i` treated as single consonant phonemes.
//! 3. **Editorial & Typographic constraints**:
//!    - Orphan vowels prohibited at line breaks (e.g. `a-mor` -> no break; `ú-nico` -> `úni-co`;
//!      `aé-reo` -> `aé-reo`). Enforced by [`Limits::after_first`] >= 2 and [`Limits::before_last`] >= 2.
//!    - Discretionary hyphen override and user dictionary exceptions.

use super::Limits;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VowelKind {
    Open,             // a, e, o, á, é, ó
    ClosedUnaccented, // i, u, ü
    ClosedTonic,      // í, ú
}

fn vowel_kind(c: char) -> Option<VowelKind> {
    match c {
        'a' | 'e' | 'o' | 'á' | 'é' | 'ó' => Some(VowelKind::Open),
        'i' | 'u' | 'ü' => Some(VowelKind::ClosedUnaccented),
        'í' | 'ú' => Some(VowelKind::ClosedTonic),
        _ => None,
    }
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | 'ʼ')
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || is_apostrophe(c)
}

/// Whether char `c` is treated as a vowel in context at index `i` in `core`.
fn is_vowel_at(core: &[char], i: usize) -> bool {
    let c = core[i];
    if c == 'y' || c == 'Y' {
        // 'y' acts as a vowel /i/ at the end of a word or before a consonant when preceded by a vowel
        // (e.g. "buey", "rey", "guay"). At onset or intervocalic it's a consonant ("ya", "reyes").
        let prev_vowel = i > 0 && vowel_kind(core[i - 1]).is_some();
        let next_is_vowel = i + 1 < core.len() && vowel_kind(core[i + 1]).is_some();
        return prev_vowel && !next_is_vowel;
    }
    if c == 'u' || c == 'U' {
        // In "qu" ("queso", "química") or "gu" before e/i without dieresis ("guerra", "guiso"),
        // 'u' is a mute orthographic digraph component, not a vowel.
        if i > 0 && (core[i - 1] == 'q' || core[i - 1] == 'Q') {
            return false;
        }
        if i > 0
            && (core[i - 1] == 'g' || core[i - 1] == 'G')
            && i + 1 < core.len()
            && matches!(core[i + 1], 'e' | 'é' | 'E' | 'É' | 'i' | 'í' | 'I' | 'Í')
        {
            return false;
        }
    }
    vowel_kind(c).is_some()
}

fn vowel_kind_at(core: &[char], i: usize) -> Option<VowelKind> {
    let c = core[i];
    if (c == 'y' || c == 'Y') && is_vowel_at(core, i) {
        return Some(VowelKind::ClosedUnaccented);
    }
    if !is_vowel_at(core, i) {
        return None;
    }
    vowel_kind(c)
}

/// Whether two vowels at `i` and `j` form a diphthong.
fn is_diphthong(core: &[char], i: usize, j: usize) -> bool {
    let (Some(k1), Some(k2)) = (vowel_kind_at(core, i), vowel_kind_at(core, j)) else {
        return false;
    };
    let c1 = core[i];
    let c2 = core[j];
    // Identical vowels form a hiatus (e.g. "leer", "chiita", "cooperar").
    if c1 == c2 {
        return false;
    }
    match (k1, k2) {
        // Open + closed unaccented ("pausa", "peine", "sauce", "caimán")
        (VowelKind::Open, VowelKind::ClosedUnaccented) => true,
        // Closed unaccented + open ("viaje", "hielo", "cuota", "canción", "después")
        (VowelKind::ClosedUnaccented, VowelKind::Open) => true,
        // Two different closed unaccented ("ciudad", "cuidado", "triunfo")
        (VowelKind::ClosedUnaccented, VowelKind::ClosedUnaccented) => true,
        // Closed unaccented + closed tonic ("construído", "lingüística")
        (VowelKind::ClosedUnaccented, VowelKind::ClosedTonic) => true,
        // Hiatuses: Open + Open, Open + Closed tonic, Closed tonic + Open
        _ => false,
    }
}

/// Whether three vowels at `i`, `j`, `k` form a triphthong.
fn is_triphthong(core: &[char], i: usize, j: usize, k: usize) -> bool {
    let (Some(k1), Some(k2), Some(k3)) = (vowel_kind_at(core, i), vowel_kind_at(core, j), vowel_kind_at(core, k)) else {
        return false;
    };
    // Triphthong: closed unaccented + open + closed unaccented ("averigüéis", "limpiáis", "buey")
    k1 == VowelKind::ClosedUnaccented && k2 == VowelKind::Open && k3 == VowelKind::ClosedUnaccented
}

/// Inseparable consonant pair in Spanish.
fn is_inseparable_cluster(a: char, b: char) -> bool {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    matches!(
        (a, b),
        ('c', 'h') | ('l', 'l') | ('r', 'r') | ('b' | 'c' | 'd' | 'f' | 'g' | 'k' | 'p' | 't', 'r') | ('b' | 'c' | 'f' | 'g' | 'k' | 'p', 'l')
    )
}

/// Finds the contiguous vocalic nuclei ranges `start..end` in `core`.
fn find_vocalic_nuclei(core: &[char]) -> Vec<std::ops::Range<usize>> {
    let mut nuclei = Vec::new();
    let n = core.len();
    let mut i = 0;
    while i < n {
        if !is_vowel_at(core, i) {
            i += 1;
            continue;
        }
        // Check for triphthong: i, i+1, i+2
        if i + 2 < n && is_vowel_at(core, i + 1) && is_vowel_at(core, i + 2) && is_triphthong(core, i, i + 1, i + 2) {
            nuclei.push(i..i + 3);
            i += 3;
            continue;
        }
        // Check for diphthong with silent 'h': e.g. "prohibir", "ahumar"
        if i + 2 < n && (core[i + 1] == 'h' || core[i + 1] == 'H') && is_vowel_at(core, i + 2) && is_diphthong(core, i, i + 2) {
            nuclei.push(i..i + 3);
            i += 3;
            continue;
        }
        // Check for normal diphthong: i, i+1
        if i + 1 < n && is_vowel_at(core, i + 1) && is_diphthong(core, i, i + 1) {
            nuclei.push(i..i + 2);
            i += 2;
            continue;
        }
        // Single vowel nucleus
        nuclei.push(i..i + 1);
        i += 1;
    }
    nuclei
}

/// Unrestricted syllabic break points of one lowercase Spanish core (char indices).
pub fn core_breaks(core: &[char]) -> Vec<usize> {
    let n = core.len();
    if n < 2 {
        return vec![];
    }
    let nuclei = find_vocalic_nuclei(core);
    if nuclei.len() < 2 {
        return vec![];
    }

    let mut breaks = Vec::with_capacity(nuclei.len());

    for w in nuclei.windows(2) {
        let n1_end = w[0].end;
        let n2_start = w[1].start;
        let c_len = n2_start - n1_end;

        let cut = match c_len {
            // Case 0: Hiatus without consonants ("te-atro", "le-er", "pa-ís")
            0 => n1_end,
            // Case 1: Single consonant or 'h' between vowels ("me-sa", "a-ho-ra", "bú-ho")
            1 => {
                // Consonant goes to the next syllable
                n1_end
            }
            // Case 2: Two consonants between vowels
            2 => {
                let c0 = core[n1_end];
                let c1 = core[n1_end + 1];
                if (c1 == 'h' || c1 == 'H') && c0 != 'c' && c0 != 'C' {
                    // Preceding consonant + 'h' divides before 'h' ("al-haja", "des-hora", "in-halar")
                    n1_end + 1
                } else if is_inseparable_cluster(c0, c1) {
                    // Inseparable onset cluster ("ha-blar", "a-brir", "co-che", "ca-lle", "tie-rra")
                    n1_end
                } else {
                    // Regular pair divides between them ("ac-ción", "gim-nas-ta", "al-to", "ob-je-to")
                    n1_end + 1
                }
            }
            // Case 3: Three consonants between vowels
            3 => {
                let c1 = core[n1_end + 1];
                let c2 = core[n1_end + 2];
                if (c2 == 'h' || c2 == 'H') && c1 != 'c' && c1 != 'C' {
                    n1_end + 2
                } else if is_inseparable_cluster(c1, c2) {
                    // C1 | C2 C3 ("in-flar", "com-prar", "es-tre-lla", "des-tro-zo", "ex-pli-car")
                    n1_end + 1
                } else {
                    // C1 C2 | C3 ("ins-ti-tu-to", "cons-tan-te", "pers-pi-caz")
                    n1_end + 2
                }
            }
            // Case 4: Four consonants between vowels ("cons-truc-ción", "obs-truc-ción")
            4 => n1_end + 2,
            // Case >= 5: Rare/foreign clusters
            _ => {
                let last1 = core[n2_start - 2];
                let last2 = core[n2_start - 1];
                if is_inseparable_cluster(last1, last2) { n2_start - 2 } else { n2_start - 1 }
            }
        };

        if cut > 0 && cut < n && !breaks.contains(&cut) {
            breaks.push(cut);
        }
    }

    // Morphological prefix awareness (RAE Ortografía Cap. V)
    // Allows division at prefix boundary when followed by vowel or liquid.
    add_prefix_breaks(core, &mut breaks);

    breaks.sort_unstable();
    breaks.dedup();

    // Ensure every syllable chunk between breaks contains at least one vowel.
    let mut valid = Vec::with_capacity(breaks.len());
    let mut last = 0;
    for &b in &breaks {
        let has_vowel = (last..b).any(|idx| is_vowel_at(core, idx));
        if has_vowel {
            valid.push(b);
            last = b;
        }
    }
    if let Some(&last_b) = valid.last() {
        let tail_has_vowel = (last_b..core.len()).any(|idx| is_vowel_at(core, idx));
        if !tail_has_vowel {
            valid.pop();
        }
    }
    valid
}

/// Checks productive Spanish prefixes to add morphological break points.
fn add_prefix_breaks(core: &[char], breaks: &mut Vec<usize>) {
    let s: String = core.iter().collect();
    // "des-" + vowel or 'h'
    if s.starts_with("des") && s.len() > 4 {
        let next = core[3];
        if is_vowel_at(core, 3) || next == 'h' {
            breaks.retain(|&b| b != 2);
            breaks.push(3);
        }
    }
    // "sub-" + vowel or 'l' or 'r'
    if s.starts_with("sub") && s.len() > 4 {
        let next = core[3];
        if is_vowel_at(core, 3) || matches!(next, 'l' | 'r') {
            breaks.retain(|&b| b != 2);
            breaks.push(3);
        }
    }
    // "in-" + vowel
    if s.starts_with("in") && s.len() > 3 && is_vowel_at(core, 2) {
        breaks.retain(|&b| b != 1);
        breaks.push(2);
    }
    // "trans-" + vowel
    if s.starts_with("trans") && s.len() > 6 && is_vowel_at(core, 5) {
        breaks.retain(|&b| b != 4);
        breaks.push(5);
    }
    // "inter-" + vowel
    if s.starts_with("inter") && s.len() > 6 && is_vowel_at(core, 5) {
        breaks.retain(|&b| b != 4);
        breaks.push(5);
    }
    // "super-" + vowel
    if s.starts_with("super") && s.len() > 6 && is_vowel_at(core, 5) {
        breaks.retain(|&b| b != 4);
        breaks.push(5);
    }
}

/// Unrestricted break points of `word` (char indices; a hyphen goes before that char).
pub fn word_breaks(word: &str) -> Vec<usize> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_word_char(chars[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && is_word_char(chars[j]) {
            j += 1;
        }
        let (mut a, mut b) = (i, j);
        while a < b && is_apostrophe(chars[a]) {
            a += 1;
        }
        while b > a && is_apostrophe(chars[b - 1]) {
            b -= 1;
        }
        let core: Vec<char> = chars[a..b].iter().map(|&c| if is_apostrophe(c) { '\'' } else { c.to_lowercase().next().unwrap_or(c) }).collect();
        let pts = core_breaks(&core);
        out.extend(pts.into_iter().filter(|&p| p > 0 && p < core.len() && core[p - 1] != '\'' && core[p] != '\'').map(|p| a + p));
        i = j;
    }
    out
}

/// Allowed hyphenation points in Spanish `word` as char indices with paragraph limits applied.
///
/// Prevents orphan vowels (RAE §5.2.2.1: "Una vocal sola no debe quedar al final de línea"),
/// capitalized acronyms, and words shorter than [`Limits::min_word`].
pub fn hyphen_points(word: &str, lim: &Limits) -> Vec<usize> {
    let chars: Vec<char> = word.chars().collect();
    let pts = word_breaks(word);
    if pts.is_empty() {
        return pts;
    }
    let min_before = lim.after_first.max(2);
    let min_after = lim.before_last.max(2);

    let run_of = |p: usize| {
        let mut a = p;
        while a > 0 && is_word_char(chars[a - 1]) {
            a -= 1;
        }
        let mut b = p;
        while b < chars.len() && is_word_char(chars[b]) {
            b += 1;
        }
        while a < b && is_apostrophe(chars[a]) {
            a += 1;
        }
        while b > a && is_apostrophe(chars[b - 1]) {
            b -= 1;
        }
        (a, b)
    };

    pts.into_iter()
        .filter(|&p| {
            let (a, b) = run_of(p);
            let run = &chars[a..b];
            let letters = run.iter().filter(|c| c.is_alphabetic()).count();
            if letters < lim.min_word.max(4) {
                return false;
            }
            if letters > 1 && run.iter().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase()) {
                return false;
            }
            if !lim.capitalized && run[0].is_uppercase() {
                return false;
            }
            let before = chars[a..p].iter().filter(|c| c.is_alphabetic()).count();
            let after = chars[p..b].iter().filter(|c| c.is_alphabetic()).count();
            before >= min_before && after >= min_after
        })
        .collect()
}

/// `word` with `-` at every hyphenation point (tests and diagnostics).
pub fn hyphenate_word(word: &str, lim: &Limits) -> String {
    let pts = hyphen_points(word, lim);
    let mut s = String::with_capacity(word.len() + pts.len());
    for (i, ch) in word.chars().enumerate() {
        if pts.contains(&i) {
            s.push('-');
        }
        s.push(ch);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(w: &str) -> String {
        hyphenate_word(w, &Limits::default())
    }

    #[test]
    fn test_basic_syllabification() {
        assert_eq!(h("declaración"), "de-cla-ra-ción");
        assert_eq!(h("construcción"), "cons-truc-ción");
        assert_eq!(h("estrella"), "es-tre-lla");
        assert_eq!(h("hablar"), "ha-blar");
        assert_eq!(h("cuadro"), "cua-dro");
        assert_eq!(h("guitarra"), "gui-ta-rra");
        assert_eq!(h("guerra"), "gue-rra");
        assert_eq!(h("pingüino"), "pin-güi-no");
    }

    #[test]
    fn test_diphthongs_and_hiatuses() {
        // Diphthongs are not split
        assert_eq!(h("cuidado"), "cui-da-do");
        assert_eq!(h("ciudad"), "ciu-dad");
        assert_eq!(h("farmacia"), "far-ma-cia");
        assert_eq!(h("canción"), "can-ción");
        assert_eq!(h("averigüéis"), "ave-ri-güéis");

        // Hiatuses are split
        assert_eq!(h("teatro"), "te-a-tro");
        assert_eq!(h("poesía"), "po-e-sía");
        assert_eq!(h("países"), "pa-í-ses");
        assert_eq!(h("héroe"), "hé-roe");
    }

    #[test]
    fn test_no_orphan_vowels() {
        // RAE rule: no single vowel alone at line start or line end
        assert_eq!(h("aéreo"), "aé-reo");
        assert_eq!(h("amor"), "amor");
        assert_eq!(h("único"), "úni-co");
        assert_eq!(h("abuelo"), "abue-lo");
        assert_eq!(h("avión"), "avión");
    }

    #[test]
    fn test_h_muda_and_prefixes() {
        assert_eq!(h("alcohol"), "al-co-hol");
        assert_eq!(h("alhaja"), "al-ha-ja");
        assert_eq!(h("desarmar"), "des-ar-mar");
        assert_eq!(h("deshacer"), "des-ha-cer");
        assert_eq!(h("inútil"), "in-ú-til");
        assert_eq!(h("sublunar"), "sub-lu-nar");
        assert_eq!(h("transporte"), "trans-por-te");
    }

    #[test]
    fn test_acronyms_and_capitalization() {
        assert_eq!(h("UNESCO"), "UNESCO");
        assert_eq!(h("Declaración"), "De-cla-ra-ción");
        let lim_no_caps = Limits { capitalized: false, ..Limits::default() };
        assert_eq!(hyphenate_word("Declaración", &lim_no_caps), "Declaración");
    }

    #[test]
    fn test_complex_and_modern_words() {
        assert_eq!(h("ciberseguridad"), "ci-ber-se-gu-ri-dad");
        assert_eq!(h("geolocalización"), "ge-o-lo-ca-li-za-ción");
        assert_eq!(h("escríbemelo"), "es-crí-be-me-lo");
        assert_eq!(h("diciéndoselo"), "di-cién-do-se-lo");
        assert_eq!(h("interactivo"), "in-ter-ac-ti-vo");
        assert_eq!(h("superestructura"), "su-per-es-truc-tu-ra");
        assert_eq!(h("transatlántico"), "trans-at-lán-ti-co");
        assert_eq!(h("perspicaz"), "pers-pi-caz");
        assert_eq!(h("obstrucción"), "obs-truc-ción");
        assert_eq!(h("instrucción"), "ins-truc-ción");
        assert_eq!(h("subacuático"), "sub-a-cuá-ti-co");
        assert_eq!(h("inyección"), "in-yec-ción");
    }
}
