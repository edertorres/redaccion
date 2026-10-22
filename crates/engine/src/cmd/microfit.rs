//! Bounded, opt-in typographic closing. Trials never enter session history.
use designcraft_compose::{ComposeOptions, ComposedStory};
use designcraft_doc::{Document, StoryId};
use serde_json::{Value, json};

use super::{CommandSpec, cmd, has_doc};

fn bad(_: &str, message: &str) -> crate::EngineError {
    crate::EngineError::Other(message.into())
}
use crate::{Result, Session};

const ID: &str = "text.microfit";
const NO_FIT: &str = "No professional fit within the template limits; revise the text";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "text.microfit",
        "Microfit Text",
        ["Type"],
        None,
        "{story? | frame?, size?, tracking?, kerning?, leading?, h_scale?, max_excess_words?, close_bottom?} — autofit story to frame",
        has_doc,
        run
    )]
}

#[derive(Clone, Copy)]
struct Limits {
    size: f64,
    tracking: f64,
    kerning: f64,
    leading: f64,
    h_scale: f64,
    size_range: (f64, f64),
    tracking_range: (f64, f64),
    kerning_range: (f64, f64),
    leading_range: (f64, f64),
    h_scale_range: (f64, f64),
    close: bool,
}

fn parse_param_range(p: &Value, keys: &[&str], keys_min: &[&str], keys_max: &[&str]) -> Option<(f64, f64)> {
    for k in keys {
        if let Some(arr) = p.get(*k).and_then(Value::as_array)
            && arr.len() == 2
            && let (Some(a), Some(b)) = (arr[0].as_f64(), arr[1].as_f64())
            && a.is_finite()
            && b.is_finite()
            && a <= b
        {
            return Some((a, b));
        }
        if let Some(obj) = p.get(*k).and_then(Value::as_object) {
            let start = obj.get("start").or_else(|| obj.get("min")).and_then(Value::as_f64);
            let end = obj.get("end").or_else(|| obj.get("max")).and_then(Value::as_f64);
            if let (Some(a), Some(b)) = (start, end)
                && a.is_finite()
                && b.is_finite()
                && a <= b
            {
                return Some((a, b));
            }
        }
    }
    for (k_min, k_max) in keys_min.iter().zip(keys_max.iter()) {
        if let (Some(a), Some(b)) = (p.get(*k_min).and_then(Value::as_f64), p.get(*k_max).and_then(Value::as_f64))
            && a.is_finite()
            && b.is_finite()
            && a <= b
        {
            return Some((a, b));
        }
    }
    None
}

fn limits(doc: &Document, sid: StoryId, p: &Value) -> Result<Limits> {
    let story = doc.story(sid).ok_or_else(|| bad(ID, "No text story selected"))?;
    if story.is_empty()
        || story.len() > 16_384
        || story.frames.len() > 16
        || story.paras.len() > 256
        || story.chars.len() > 1024
        || story.vertical
        || !story.tables.is_empty()
        || !story.notes.is_empty()
        || !story.endnotes.is_empty()
        || !story.objects.is_empty()
        || story.link.is_some()
    {
        return Err(bad(ID, "Microfit requires a short, ordinary horizontal text story"));
    }
    for id in &story.frames {
        let tf = doc.item(*id).and_then(|i| i.text_frame()).ok_or_else(|| bad(ID, "Missing text frame"))?;
        if tf.options.auto_size != designcraft_doc::AutoSize::Off || tf.options.vertical_justification != designcraft_doc::VerticalJustification::Top
        {
            return Err(bad(ID, "Microfit requires fixed frames with top alignment"));
        }
    }
    let p_size = parse_param_range(p, &["size", "size_range", "sizeRange"], &["size_min", "sizeMin"], &["size_max", "sizeMax"]);
    let p_trk = parse_param_range(
        p,
        &["tracking", "tracking_range", "trk_range"],
        &["trk_min", "trkMin", "tracking_min"],
        &["trk_max", "trkMax", "tracking_max"],
    );
    let p_kern = parse_param_range(
        p,
        &["kerning", "kerning_range", "kern_range"],
        &["kern_min", "kernMin", "kerning_min"],
        &["kern_max", "kernMax", "kerning_max"],
    );
    let p_lead = parse_param_range(p, &["leading", "leading_range"], &["leading_min", "leadingMin"], &["leading_max", "leadingMax"]);
    let p_hscale = parse_param_range(p, &["h_scale", "hscale", "hScale", "hscale_range"], &["hscale_min", "hScaleMin"], &["hscale_max", "hScaleMax"]);
    let p_close = p.get("close_bottom").or_else(|| p.get("closeBottom")).and_then(Value::as_bool);

    let mut result: Option<Limits> = None;
    let paras = story.para_ranges();
    if paras.iter().any(|p| p.is_empty()) {
        return Err(bad(ID, "Microfit requires nonempty paragraphs"));
    }
    for (range, format) in story.runs() {
        for (index, pr) in paras.iter().enumerate() {
            if range.start >= pr.end || range.end <= pr.start {
                continue;
            }
            let para = story.paras.get(index).ok_or_else(|| bad(ID, "Invalid paragraph"))?;
            let (_, base) = doc.styles.resolve_para(para);
            let cp = doc.styles.resolve_char(&base, format);
            let policy = doc.styles.resolve_copyfit(&para.style, &format.style);
            if policy.is_none() && p_size.is_none() && p_trk.is_none() && p_kern.is_none() && p_lead.is_none() && p_hscale.is_none() {
                return Err(bad(ID, "Configure copyfit limits in the template style first"));
            }
            if !cp.size.is_finite() || cp.size <= 0.0 || !cp.tracking.is_finite() {
                return Err(bad(ID, "Invalid typography"));
            }
            let cur_kern = match cp.kerning {
                designcraft_doc::attrs::Kerning::Manual(v) => v,
                _ => 0.0,
            };
            let cur_lead = match cp.leading {
                designcraft_doc::attrs::Leading::Points(v) => v,
                _ => cp.size * 1.2,
            };
            let cur_hscale = cp.h_scale;

            let size_range = if let Some(r) = p_size {
                r
            } else if let Some(pol) = policy {
                bounded(pol.size_flexibility.as_ref(), cp.size, (cp.size * 0.99, cp.size * 1.01))?
            } else {
                (cp.size * 0.99, cp.size * 1.01)
            };

            let tracking_range = if let Some(r) = p_trk {
                r
            } else if let Some(pol) = policy {
                bounded(pol.tracking_flexibility.as_ref(), cp.tracking, ((cp.tracking - 5.0).max(-15.0), (cp.tracking + 5.0).min(15.0)))?
            } else {
                ((cp.tracking - 5.0).max(-15.0), (cp.tracking + 5.0).min(15.0))
            };

            let kerning_range = if let Some(r) = p_kern {
                r
            } else if let Some(pol) = policy {
                bounded(pol.kerning_allowance.as_ref(), cur_kern, (-20.0, 20.0))?
            } else {
                (cur_kern, cur_kern)
            };

            let leading_range = if let Some(r) = p_lead {
                r
            } else if let Some(pol) = policy {
                bounded(pol.leading_flexibility.as_ref(), cur_lead, (cur_lead * 0.98, cur_lead * 1.02))?
            } else {
                (cur_lead, cur_lead)
            };

            let h_scale_range = if let Some(r) = p_hscale {
                r
            } else if let Some(pol) = policy {
                bounded(pol.h_scale_flexibility.as_ref(), cur_hscale, (0.98, 1.02))?
            } else {
                (cur_hscale, cur_hscale)
            };

            let close = p_close.unwrap_or_else(|| policy.map(|p| p.close_bottom).unwrap_or(false));

            if let Some(r) = &mut result {
                if ((r.size - cp.size).abs() > 0.0001 || (r.tracking - cp.tracking).abs() > 0.0001) && p_size.is_none() && p_trk.is_none() {
                    return Err(bad(ID, "Microfit requires consistent body size and tracking"));
                }
                r.size_range = intersect(r.size_range, size_range);
                r.tracking_range = intersect(r.tracking_range, tracking_range);
                r.kerning_range = intersect(r.kerning_range, kerning_range);
                r.leading_range = intersect(r.leading_range, leading_range);
                r.h_scale_range = intersect(r.h_scale_range, h_scale_range);
                r.close &= close;
            } else {
                result = Some(Limits {
                    size: cp.size,
                    tracking: cp.tracking,
                    kerning: cur_kern,
                    leading: cur_lead,
                    h_scale: cur_hscale,
                    size_range,
                    tracking_range,
                    kerning_range,
                    leading_range,
                    h_scale_range,
                    close,
                });
            }
        }
    }
    result.ok_or_else(|| bad(ID, "No ordinary text to adjust"))
}

fn intersect(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0.max(b.0), a.1.min(b.1))
}

fn bounded(range: Option<&std::ops::Range<f64>>, current: f64, cap: (f64, f64)) -> Result<(f64, f64)> {
    let Some(r) = range else {
        return Ok((current, current));
    };
    if !r.start.is_finite() || !r.end.is_finite() || r.start > current || r.end < current {
        return Err(bad(ID, "Current typography is outside the copyfit limits"));
    }
    let v = intersect((r.start, r.end), cap);
    // Existing typography outside the conservative automatic cap may only stay unchanged.
    Ok(if v.0 <= current && current <= v.1 { v } else { (current, current) })
}

fn quality(cs: &ComposedStory) -> [usize; 5] {
    let mut scores = [0usize; 5];
    let mut streak = 0usize;
    for l in cs.frames.iter().flat_map(|f| &f.lines) {
        for (index, threshold) in [1, 2, 3].into_iter().enumerate() {
            if l.hj >= threshold
                && let Some(score) = scores.get_mut(index)
            {
                *score = score.saturating_add(1);
            }
        }
        if let Some(score) = scores.get_mut(3) {
            *score = score.saturating_add(usize::from(l.keep_violation));
        }
        streak = if l.hyphenated { streak.saturating_add(1) } else { 0 };
        if let Some(score) = scores.get_mut(4) {
            *score = (*score).max(streak);
        }
    }
    scores
}

/// Distance from the last baseline to the final column's usable last baseline, in lines.
fn bottom_gap(cs: &ComposedStory) -> Option<f64> {
    let frame = cs.frames.last()?;
    let line = frame.lines.last()?;
    let col = frame.columns.last()?;
    if usize::try_from(line.column).ok()? != frame.columns.len().checked_sub(1)? || line.leading <= 0.0 {
        return None;
    }
    let gap = (col.y1 - line.descent - line.baseline) / line.leading;
    gap.is_finite().then_some(gap)
}

fn adjusted(
    doc: &Document,
    sid: StoryId,
    size: Option<f64>,
    tracking: Option<f64>,
    kerning: Option<f64>,
    leading: Option<f64>,
    h_scale: Option<f64>,
    base_size: f64,
    base_leading: f64,
) -> Result<Document> {
    let mut trial = doc.clone();
    let st = trial.story_mut(sid).ok_or_else(|| bad(ID, "Missing story"))?;
    let size_ratio = size.map(|s| s / base_size);
    let leading_ratio = leading.map(|l| l / base_leading);
    st.format_chars(0..st.len(), |f| {
        if let Some(ratio) = size_ratio {
            let cur = f.over.size.unwrap_or(base_size);
            f.over.size = Some((cur * ratio * 100.0).round() / 100.0);
        }
        if let Some(v) = tracking {
            f.over.tracking = Some(v);
        }
        if let Some(k) = kerning {
            f.over.kerning = Some(designcraft_doc::attrs::Kerning::Manual(k));
        }
        if let Some(ratio) = leading_ratio {
            let cur = match f.over.leading {
                Some(designcraft_doc::attrs::Leading::Points(p)) => p,
                _ => base_leading,
            };
            f.over.leading = Some(designcraft_doc::attrs::Leading::Points((cur * ratio * 100.0).round() / 100.0));
        }
        if let Some(h) = h_scale {
            f.over.h_scale = Some(h);
        }
    });
    Ok(trial)
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    for key in ["story", "frame"] {
        if p.get(key).is_some_and(|v| v.as_u64().is_none()) {
            return Err(bad(ID, "Invalid story or frame identifier"));
        }
    }
    if s.doc()?.interaction.is_some() {
        return Err(bad(ID, "Finish the current gesture before microfit"));
    }
    if p.get("story").is_none() && p.get("frame").is_none() {
        let st = s.doc()?;
        let mut selected = st.selection.items.iter().filter_map(|id| st.doc.item(*id)?.text_frame().map(|tf| tf.story));
        if let Some(first) = selected.next()
            && selected.any(|sid| sid != first)
        {
            return Err(bad(ID, "Select frames from one story for microfit"));
        }
    }
    let sid = super::text::story_of(s, p).ok_or_else(|| bad(ID, "No text story selected"))?;
    if !s.editorial_story_editable(sid) {
        return Err(crate::editor_policy::denied());
    }
    let original = s.doc()?.doc.clone();
    let lim = limits(&original, sid, p)?;
    let story = original.story(sid).ok_or_else(|| bad(ID, "Missing story"))?;
    let before = designcraft_compose::compose_story(&original, sid, &ComposeOptions::default());
    let shrink = before.is_overset();
    let has_explicit_params = p.get("size").is_some()
        || p.get("size_min").is_some()
        || p.get("tracking").is_some()
        || p.get("trk_min").is_some()
        || p.get("kerning").is_some()
        || p.get("kern_min").is_some()
        || p.get("leading").is_some()
        || p.get("leading_min").is_some()
        || p.get("h_scale").is_some()
        || p.get("hscale_min").is_some()
        || p.get("max_excess_words").is_some();
    let max_words = p.get("max_excess_words").and_then(Value::as_u64).unwrap_or(if has_explicit_params { 50 } else { 3 }) as usize;
    let max_chars = p.get("max_excess_chars").and_then(Value::as_u64).unwrap_or(if has_explicit_params { 300 } else { 32 }) as usize;
    let allow_newlines = p.get("allow_newlines").and_then(Value::as_bool).unwrap_or(has_explicit_params);

    if shrink {
        let excess = before.overset_at.and_then(|at| story.text.get(at..)).ok_or_else(|| bad(ID, NO_FIT))?;
        if excess.split_whitespace().count() > max_words || excess.chars().count() > max_chars || (!allow_newlines && excess.contains('\n')) {
            return Err(bad(ID, "Too much excess text for microfit; revise the text"));
        }
    } else {
        if !lim.close {
            return Err(bad(ID, "Bottom closing is not enabled in the template style"));
        }
        let gap = bottom_gap(&before).ok_or_else(|| bad(ID, NO_FIT))?;
        if gap < 0.8 {
            return Ok(json!({"status":"alreadyFits"}));
        }
        if gap > 1.8 && !has_explicit_params {
            return Err(bad(ID, "Too much empty space for microfit; add text"));
        }
    }
    let q = quality(&before);
    let direction = if shrink { -1.0 } else { 1.0 };
    let size_end = if shrink { lim.size_range.0 } else { lim.size_range.1 };
    let trk_end = if shrink { lim.tracking_range.0 } else { lim.tracking_range.1 };
    let kern_end = if shrink { lim.kerning_range.0 } else { lim.kerning_range.1 };
    let lead_end = if shrink { lim.leading_range.0 } else { lim.leading_range.1 };
    let hscale_end = if shrink { lim.h_scale_range.0 } else { lim.h_scale_range.1 };

    let mut trials = 0;
    let mut winner = None;

    'search: for size_step in 0..=4 {
        let size = lim.size + (size_end - lim.size) * f64::from(size_step) / 4.0;
        let size_change = ((size - lim.size).abs() > 0.00001).then_some(size);

        let leading = lim.leading + (lead_end - lim.leading) * f64::from(size_step) / 4.0;
        let leading_change = ((leading - lim.leading).abs() > 0.00001).then_some(leading);

        for hscale_step in 0..=2 {
            let hscale = lim.h_scale + (hscale_end - lim.h_scale) * f64::from(hscale_step) / 2.0;
            let hscale_change = ((hscale - lim.h_scale).abs() > 0.00001).then_some(hscale);

            for trk_step in 0..=6 {
                let tracking = lim.tracking + (trk_end - lim.tracking) * f64::from(trk_step) / 6.0;
                let tracking_change = ((tracking - lim.tracking).abs() > 0.00001).then_some(tracking);

                for kern_step in 0..=2 {
                    if size_step == 0 && hscale_step == 0 && trk_step == 0 && kern_step == 0 {
                        continue;
                    }
                    let kerning = lim.kerning + (kern_end - lim.kerning) * f64::from(kern_step) / 2.0;
                    let kerning_change = ((kerning - lim.kerning).abs() > 0.00001).then_some(kerning);

                    let trial =
                        adjusted(&original, sid, size_change, tracking_change, kerning_change, leading_change, hscale_change, lim.size, lim.leading)?;
                    let cs = designcraft_compose::compose_story(&trial, sid, &ComposeOptions::default());
                    trials += 1;
                    let after_q = quality(&cs);
                    if cs.is_overset() {
                        continue;
                    }
                    if !has_explicit_params && after_q.iter().zip(q).any(|(after, before)| *after > before) {
                        continue;
                    }
                    let Some(last) = cs.frames.last().and_then(|f| f.lines.last()) else { continue };
                    let tail = story.text.get(last.range.clone()).ok_or_else(|| bad(ID, "Invalid composed text range"))?;
                    if !has_explicit_params && tail.split_whitespace().count() < 2 {
                        continue;
                    }
                    if !shrink {
                        let Some(gap) = bottom_gap(&cs) else { continue };
                        if !(0.0..0.8).contains(&gap) || cs.line_count() != before.line_count().saturating_add(1) {
                            continue;
                        }
                        if !(2..=3).contains(&tail.split_whitespace().count()) {
                            continue;
                        }
                    }
                    winner = Some((size_change, tracking_change, kerning_change, leading_change, hscale_change));
                    break 'search;
                }
            }
        }
    }

    let Some((size, tracking, kerning, leading, hscale)) = winner else {
        return Err(bad(ID, NO_FIT));
    };
    let result = adjusted(&original, sid, size, tracking, kerning, leading, hscale, lim.size, lim.leading)?;
    s.edit(|d, _| {
        let replacement = result.story(sid).ok_or_else(|| bad(ID, "Missing trial story"))?.clone();
        let target = d.story_mut(sid).ok_or_else(|| bad(ID, "Missing story"))?;
        *target = replacement;
        Ok(())
    })?;
    Ok(json!({
        "status": "adjusted",
        "direction": if direction < 0.0 { "shrink" } else { "expand" },
        "size": size,
        "tracking": tracking,
        "kerning": kerning,
        "leading": leading,
        "h_scale": hscale,
        "trials": trials
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(words: usize, width: f64, height: f64, close: bool) -> (Session, StoryId) {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("style.paragraph.create", &json!({"name":"Body", "chars":{"size":12}})).unwrap();
        s.execute(
            "style.paragraph.edit",
            &json!({"name":"Body","copyfit":{"size_min":11.88,"size_max":12.12,"trk_min":-5,"trk_max":5,"close_bottom":close}}),
        )
        .unwrap();
        let text = "casa sol papel noticia breve ".repeat(words.div_ceil(5));
        let text = text.split_whitespace().take(words).collect::<Vec<_>>().join(" ");
        let frame = s.execute("frame.create", &json!({"rect":[36,36,width,height],"content":"text","text":text})).unwrap();
        let sid = StoryId(frame["story"].as_u64().unwrap());
        s.execute("text.select", &json!({"story":sid.0,"anchor":0,"focus":text.len()})).unwrap();
        // Set the style explicitly through the document API, as template preparation would.
        s.edit(|d, _| {
            for p in &mut d.story_mut(sid).unwrap().paras {
                p.style = "Body".into();
            }
            Ok(())
        })
        .unwrap();
        s.execute("selection.set", &json!({"ids":[frame["id"]]})).unwrap();
        s.editor_mode = true;
        (s, sid)
    }

    #[test]
    fn microfit_shrinks_real_overflow_atomically_and_undo_restores_template() {
        let (mut s, sid) = fixture(18, 180.0, 100.0, false);
        let doc = s.doc().unwrap().doc.clone();
        assert!(designcraft_compose::compose_story(&doc, sid, &ComposeOptions::default()).is_overset());
        let history = s.doc().unwrap().history.undo.len();
        let selection = s.doc().unwrap().selection.clone();
        let result = s.execute(ID, &json!({"story":sid.0})).unwrap();
        assert_eq!(result["direction"], "shrink");
        let after = s.doc().unwrap().doc.clone();
        assert!(!designcraft_compose::compose_story(&after, sid, &ComposeOptions::default()).is_overset());
        assert_eq!(after.story(sid).unwrap().text, doc.story(sid).unwrap().text);
        assert_eq!(after.spreads, doc.spreads);
        assert_eq!(after.styles, doc.styles);
        assert_eq!(s.doc().unwrap().selection, selection);
        assert_eq!(s.doc().unwrap().history.undo.len(), history + 1);
        if let Some(size) = result["size"].as_f64() {
            assert!((11.88..=12.12).contains(&size));
        }
        if let Some(tracking) = result["tracking"].as_f64() {
            assert!((-5.0..=5.0).contains(&tracking));
        }
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc().unwrap().doc, *doc);
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(*s.doc().unwrap().doc, *after);
    }

    #[test]
    fn microfit_expands_only_a_small_opted_in_bottom_gap() {
        let (mut s, sid) = fixture(17, 208.0, 100.0, true);
        let doc = s.doc().unwrap().doc.clone();
        let before = designcraft_compose::compose_story(&doc, sid, &ComposeOptions::default());
        assert!(!before.is_overset());
        let result = s.execute(ID, &json!({"story":sid.0})).unwrap();
        assert_eq!(result["status"], "adjusted");
        assert_eq!(result["direction"], "expand");
        let after = designcraft_compose::compose_story(&s.doc().unwrap().doc, sid, &ComposeOptions::default());
        assert!(!after.is_overset());
        assert!(bottom_gap(&after).unwrap() < 0.8);
        assert_eq!(after.line_count(), before.line_count() + 1);
        assert_eq!(s.doc().unwrap().doc.story(sid).unwrap().text, doc.story(sid).unwrap().text);
        let history = s.doc().unwrap().history.undo.len();
        assert_eq!(s.execute(ID, &json!({"story":sid.0})).unwrap()["status"], "alreadyFits");
        assert_eq!(s.doc().unwrap().history.undo.len(), history);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(*s.doc().unwrap().doc, *doc);
    }

    #[test]
    fn rejected_microfit_preserves_document_selection_and_history() {
        for (words, close) in [(200, false), (5, true), (20, false)] {
            let (mut s, sid) = fixture(words, 140.0, 100.0, close);
            let doc = s.doc().unwrap().doc.clone();
            let selection = s.doc().unwrap().selection.clone();
            let history = s.doc().unwrap().history.undo.len();
            assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
            assert_eq!(*s.doc().unwrap().doc, *doc);
            assert_eq!(s.doc().unwrap().selection, selection);
            assert_eq!(s.doc().unwrap().history.undo.len(), history);
        }
    }

    #[test]
    fn protection_and_absent_policy_cannot_be_bypassed_by_microfit() {
        let (mut s, sid) = fixture(35, 140.0, 100.0, true);
        s.editor_mode = false;
        s.execute("object.setFlags", &json!({"ids":s.doc().unwrap().doc.story(sid).unwrap().frames,"contentLocked":true})).unwrap();
        s.editor_mode = true;
        assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
        let (mut s, sid) = fixture(35, 140.0, 100.0, true);
        s.editor_mode = false;
        s.execute("style.paragraph.edit", &json!({"name":"Body","copyfit":null})).unwrap();
        s.editor_mode = true;
        assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
    }
    #[test]
    fn microfit_intersects_all_character_run_policies() {
        let (mut s, sid) = fixture(18, 180.0, 100.0, false);
        s.editor_mode = false;
        s.execute("style.character.create", &json!({"name":"Fixed"})).unwrap();
        s.execute("style.character.edit", &json!({"name":"Fixed", "copyfit":{"size_min":12,"size_max":12,"trk_min":0,"trk_max":0}})).unwrap();
        s.edit(|d, _| {
            d.story_mut(sid).unwrap().format_chars(1..4, |f| f.style = "Fixed".into());
            Ok(())
        })
        .unwrap();
        s.editor_mode = true;
        let doc = s.doc().unwrap().doc.clone();
        assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
        assert_eq!(*s.doc().unwrap().doc, *doc);
    }

    #[test]
    fn copyfit_bottom_opt_in_survives_native_roundtrip_and_old_documents_default_off() {
        let old: designcraft_doc::CopyfitPolicy = serde_json::from_value(json!({"trackingFlexibility":{"start":-5,"end":5}})).unwrap();
        assert!(!old.close_bottom);
        let (s, sid) = fixture(17, 208.0, 100.0, true);
        let bytes = super::super::file_bytes(&s.doc().unwrap().doc);
        let restored = super::super::file_from(&bytes).unwrap();
        assert!(limits(&restored, sid, &json!({})).unwrap().close);
    }

    #[test]
    fn invalid_policy_and_nonuniform_typography_are_rejected_without_changes() {
        for mixed in [true, false] {
            let (mut s, sid) = fixture(18, 180.0, 100.0, false);
            s.editor_mode = false;
            s.edit(|d, _| {
                if mixed {
                    d.story_mut(sid).unwrap().format_chars(1..4, |f| f.over.size = Some(14.0));
                } else {
                    d.styles_mut().para_mut("Body").unwrap().copyfit.as_mut().unwrap().tracking_flexibility = Some(f64::NAN..5.0);
                }
                Ok(())
            })
            .unwrap();
            s.editor_mode = true;
            let doc = s.doc().unwrap().doc.clone();
            assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
            assert!(std::sync::Arc::ptr_eq(&doc, &s.doc().unwrap().doc));
        }
    }
    #[test]
    fn malformed_explicit_target_never_falls_back_to_the_selected_story() {
        let (mut s, sid) = fixture(18, 180.0, 100.0, false);
        let doc = s.doc().unwrap().doc.clone();
        for p in [json!({"story":"wrong"}), json!({"frame":-1}), json!({"story":sid.0,"frame":null})] {
            assert!(s.execute(ID, &p).is_err());
            assert!(std::sync::Arc::ptr_eq(&doc, &s.doc().unwrap().doc));
        }
    }
    #[test]
    fn inherited_template_limits_are_shared_by_query_manual_adjustments_and_microfit() {
        for fixed in [true, false] {
            let (mut s, sid) = fixture(18, 180.0, 100.0, false);
            s.editor_mode = false;
            s.execute("style.paragraph.create", &json!({"name":"Child","basedOn":"Body"})).unwrap();
            s.execute("story.update", &json!({"story":sid.0,"paragraphStyle":"Child"})).unwrap();
            if fixed {
                s.execute("style.paragraph.edit", &json!({"name":"Body","copyfit":{"size_min":12,"size_max":12,"trk_min":0,"trk_max":0}})).unwrap();
            }
            s.editor_mode = true;
            let attrs = s.execute("type.selectionAttrs", &json!({})).unwrap();
            assert_eq!(attrs["copyfit"]["sizeFlexibility"]["end"], json!(if fixed { 12.0 } else { 12.12 }));
            let before = s.doc().unwrap().doc.clone();
            if fixed {
                assert!(s.execute("type.char", &json!({"attrs":{"size":11.99}})).is_err());
                assert!(s.execute(ID, &json!({"story":sid.0})).is_err());
                assert!(std::sync::Arc::ptr_eq(&before, &s.doc().unwrap().doc));
            } else {
                assert_eq!(s.execute(ID, &json!({"story":sid.0})).unwrap()["status"], "adjusted");
                assert!(!designcraft_compose::compose_story(&s.doc().unwrap().doc, sid, &ComposeOptions::default()).is_overset());
                assert_eq!(before.styles, s.doc().unwrap().doc.styles);
            }
        }
    }
    #[test]
    fn invalid_template_ranges_are_rejected_instead_of_silently_resetting() {
        let (mut s, _) = fixture(18, 180.0, 100.0, false);
        s.editor_mode = false;
        let original = s.doc().unwrap().doc.clone();
        for policy in [
            json!({"size_min":12,"size_max":11}),
            json!({"trk_min":-2}),
            json!({"size_min":null,"size_max":12}),
            json!({"size_min":0,"size_max":12}),
            json!({"close_bottom":"yes"}),
        ] {
            assert!(s.execute("style.paragraph.edit", &json!({"name":"Body","copyfit":policy})).is_err());
            assert!(std::sync::Arc::ptr_eq(&original, &s.doc().unwrap().doc));
        }
    }

    #[test]
    fn autofit_with_explicit_parameters_and_kerning_leading_hscale() {
        let (mut s, sid) = fixture(20, 140.0, 100.0, false);
        let res = s
            .execute(
                ID,
                &json!({
                    "story": sid.0,
                    "max_excess_words": 30,
                    "size": [10.5, 12.0],
                    "tracking": [-25.0, 10.0],
                    "kerning": [-15.0, 15.0],
                    "leading": [11.0, 15.0],
                    "h_scale": [0.97, 1.03],
                }),
            )
            .unwrap();
        assert_eq!(res["status"], "adjusted");
        assert_eq!(res["direction"], "shrink");
        let after = designcraft_compose::compose_story(&s.doc().unwrap().doc, sid, &ComposeOptions::default());
        assert!(!after.is_overset());
    }
}
