//! Character styles that paragraphs apply by themselves: nested styles (from the paragraph start
//! through or up to a delimiter) and GREP styles (every match of a pattern).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use designcraft_doc::{GrepStyle, NestedStyle, NestedUntil};

#[derive(Clone, Debug)]
struct CompiledGrep {
    re: regex::Regex,
    group1: bool,
}

thread_local! {
    static PATTERNS: RefCell<HashMap<String, Option<CompiledGrep>>> = RefCell::new(HashMap::new());
}

fn convert_lookaround(pat: &str) -> Option<String> {
    let mut s = pat;
    let mut prefix = None;
    if let Some(rest) = s.strip_prefix("(?<=") {
        let end_prefix = rest.find(')')?;
        prefix = Some(&rest[..end_prefix]);
        s = &rest[end_prefix + 1..];
    }
    let mut suffix = None;
    if let Some(idx) = s.rfind("(?=")
        && s.ends_with(')')
        && idx + 3 < s.len()
    {
        suffix = Some(&s[idx + 3..s.len() - 1]);
        s = &s[..idx];
    }
    if prefix.is_none() && suffix.is_none() {
        return None;
    }
    let mut res = String::new();
    if let Some(p) = prefix {
        res.push_str("(?:");
        res.push_str(p);
        res.push(')');
    }
    res.push('(');
    res.push_str(s);
    res.push(')');
    if let Some(suf) = suffix {
        res.push_str("(?:");
        res.push_str(suf);
        res.push(')');
    }
    Some(res)
}

fn compile_grep(pattern: &str) -> Option<CompiledGrep> {
    if let Ok(re) = regex::Regex::new(pattern) {
        let group1 = re.captures_len() > 1;
        return Some(CompiledGrep { re, group1 });
    }
    let converted = convert_lookaround(pattern)?;
    let re = regex::Regex::new(&converted).ok()?;
    Some(CompiledGrep { re, group1: true })
}

fn is_event(u: &NestedUntil, c: char, prev: Option<char>, next: Option<char>) -> bool {
    match u {
        NestedUntil::Sentences => matches!(c, '.' | '!' | '?') && next.is_none_or(char::is_whitespace),
        NestedUntil::Words => c.is_whitespace() && prev.is_some_and(|p| !p.is_whitespace()),
        NestedUntil::Characters => !matches!(c, '\n'),
        NestedUntil::Letters => c.is_alphabetic(),
        NestedUntil::Digits => c.is_numeric(),
        NestedUntil::Tab => c == '\t',
        NestedUntil::ForcedLineBreak => c == designcraft_doc::FORCED_LINE_BREAK,
        NestedUntil::EmSpace => c == '\u{2003}',
        NestedUntil::EnSpace => c == '\u{2002}',
        NestedUntil::Chars(s) => s.contains(c),
    }
}

/// Character-style ranges for the paragraph `text[range]`, in application order (later wins).
pub(crate) fn overlays(text: &str, range: Range<usize>, nested: &[NestedStyle], grep: &[GrepStyle]) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let para = &text[range.clone()];
    // The paragraph's own end mark isn't styled.
    let end = range.start + para.trim_end_matches('\n').len();
    let mut pos = range.start;
    for ns in nested {
        if pos >= end {
            break;
        }
        let mut count = 0;
        let mut stop = end;
        let chars: Vec<(usize, char)> = text[pos..end].char_indices().map(|(i, c)| (pos + i, c)).collect();
        for (k, &(i, c)) in chars.iter().enumerate() {
            let prev = if k > 0 { Some(chars[k - 1].1) } else { None };
            let next = chars.get(k + 1).map(|x| x.1);
            if is_event(&ns.until, c, prev, next) {
                count += 1;
                if count >= ns.count.max(1) {
                    stop = if ns.through { i + c.len_utf8() } else { i };
                    break;
                }
            }
        }
        if stop > pos && !ns.style.is_empty() {
            out.push((pos..stop, ns.style.clone()));
        }
        pos = stop;
    }
    for g in grep {
        if g.style.is_empty() || g.pattern.is_empty() {
            continue;
        }
        PATTERNS.with(|p| {
            let mut p = p.borrow_mut();
            let entry = p.entry(g.pattern.clone()).or_insert_with(|| compile_grep(&g.pattern));
            if let Some(compiled) = entry {
                for caps in compiled.re.captures_iter(&text[range.start..end]) {
                    let target = if compiled.group1 { caps.get(1) } else { caps.get(0) };
                    if let Some(m) = target
                        && !m.is_empty()
                    {
                        out.push((range.start + m.start()..range.start + m.end(), g.style.clone()));
                    }
                }
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_and_grep_ranges() {
        let text = "Q: what is 42? A drop of 7 words.\n".to_string();
        let n = |style: &str, through: bool, count: u32, until: NestedUntil| NestedStyle { style: style.into(), through, count, until };
        // Through the first ':' then up to the second word end.
        let o = overlays(&text, 0..text.len(), &[n("Lead", true, 1, NestedUntil::Chars(":".into())), n("Next", false, 2, NestedUntil::Words)], &[]);
        assert_eq!(&text[o[0].0.clone()], "Q:");
        assert_eq!(&text[o[1].0.clone()], " what is", "up to the end of the second word");
        // Digits by GREP.
        let o = overlays(&text, 0..text.len(), &[], &[GrepStyle { style: "Num".into(), pattern: r"\d+".into() }]);
        let got: Vec<&str> = o.iter().map(|(r, _)| &text[r.clone()]).collect();
        assert_eq!(got, ["42", "7"]);
        // A bad pattern is ignored.
        assert!(overlays(&text, 0..text.len(), &[], &[GrepStyle { style: "X".into(), pattern: "(".into() }]).is_empty());

        // Capture group targets group 1 (excluding delimiters)
        let text2 = "Hello #world# and #rust# today.\n";
        let o2 = overlays(text2, 0..text2.len(), &[], &[GrepStyle { style: "Red".into(), pattern: r"#([^#]+)#".into() }]);
        let got2: Vec<&str> = o2.iter().map(|(r, _)| &text2[r.clone()]).collect();
        assert_eq!(got2, ["world", "rust"]);

        // Lookaround conversion also targets the text inside the delimiters
        let o3 = overlays(text2, 0..text2.len(), &[], &[GrepStyle { style: "Red".into(), pattern: r"(?<=#)[^#]+(?=#)".into() }]);
        let got3: Vec<&str> = o3.iter().map(|(r, _)| &text2[r.clone()]).collect();
        assert_eq!(got3, ["world", "rust"]);
    }
}
