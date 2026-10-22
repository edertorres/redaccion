//! Template invariants for the local editorial mode. This is workflow protection,
//! not authentication: hosts remain responsible for selecting the user's role.

use std::sync::Arc;

use designcraft_doc::{Content, Document, Item, Spread};
use serde_json::Value;

use crate::{EngineError, Result, Session};

const REASON: &str = "The template protects this content or layout";

#[derive(Default)]
pub(crate) struct History {
    active: bool,
    generation: u64,
    // Only editorial undo entries count; designer history remains intact underneath.
    entries: std::collections::HashMap<u64, (usize, usize)>,
}

impl History {
    pub(crate) fn sync(&mut self, active: bool) {
        if self.active != active {
            self.entries.clear();
            self.active = active;
        }
    }

    pub(crate) fn record(&mut self, uid: u64) {
        if self.active {
            self.generation = self.generation.wrapping_add(1);
            let entry = self.entries.entry(uid).or_default();
            entry.0 = entry.0.saturating_add(1);
            entry.1 = 0;
        }
    }

    pub(crate) fn stamp(&self) -> u64 {
        self.generation
    }

    pub(crate) fn moved(&mut self, uid: u64, undo: bool) {
        let entry = self.entries.entry(uid).or_default();
        if undo {
            entry.0 = entry.0.saturating_sub(1);
            entry.1 = entry.1.saturating_add(1);
        } else {
            entry.1 = entry.1.saturating_sub(1);
            entry.0 = entry.0.saturating_add(1);
        }
    }
}

pub(crate) fn denied() -> EngineError {
    EngineError::Other(REASON.into())
}

pub(crate) fn editable_item(doc: &Document, id: designcraft_doc::ItemId) -> bool {
    let Some(loc) = doc.find(id) else { return false };
    if matches!(loc.spread, designcraft_doc::SpreadRef::Parent(_)) || loc.path.len() >= 128 {
        return false;
    }
    let Some(spread) = doc.spread(loc.spread) else { return false };
    if let Some(item) = doc.item(id)
        && let Some(pi) = spread.page_at_x(item.bounds().center().x)
        && let Some(page) = spread.pages.get(pi)
        && page.editorial.status == designcraft_doc::PageStatus::Approved
    {
        return false;
    }
    let mut siblings = spread.items.as_slice();
    for index in loc.path {
        let Some(item) = siblings.get(index) else { return false };
        if item.content_locked || item.hidden || doc.layer(item.layer).is_some_and(|l| l.locked || !l.visible) {
            return false;
        }
        siblings = item.children();
    }
    true
}

pub(crate) fn character_format(s: &Session, attrs: &Value, a: &designcraft_doc::CharAttrs) -> Result<()> {
    let fields = attrs.as_object().ok_or_else(denied)?;
    let allowed = ["size", "tracking", "kerning", "baselineshift", "leading", "hscale"];
    if fields.keys().any(|key| !allowed.contains(&key.replace('_', "").to_ascii_lowercase().as_str())) {
        return Err(denied());
    }
    let st = s.doc()?;
    for target in crate::cmd::text::format_targets(s) {
        let story = st.doc.text_story(target.story, target.cell).ok_or_else(denied)?;
        let mut positions = std::collections::BTreeSet::from([target.range.start]);
        let mut byte = 0usize;
        for run in &story.chars {
            if byte > target.range.start && byte < target.range.end {
                positions.insert(byte);
            }
            byte = byte.saturating_add(run.len);
        }
        for (byte, ch) in story.text.char_indices() {
            let next = byte.saturating_add(ch.len_utf8());
            if ch == '\n' && next > target.range.start && next < target.range.end {
                positions.insert(next);
            }
        }
        for pos in positions {
            let para = story.paras.get(story.para_at(pos)).ok_or_else(denied)?;
            let chars = story.char_format_at(pos);
            let policy = st.doc.styles.resolve_copyfit(&para.style, &chars.style);
            let policy = match policy {
                Some(p) => p,
                None if st.selection.text.is_none() => {
                    if let Some(story_policy) =
                        story.paras.iter().find_map(|p| st.doc.styles.resolve_copyfit(&p.style, designcraft_doc::NO_CHAR_STYLE))
                    {
                        story_policy
                    } else {
                        return Err(denied());
                    }
                }
                None => return Err(denied()),
            };
            for key in fields.keys() {
                let key = key.replace('_', "").to_ascii_lowercase();
                let (range, value) = match key.as_str() {
                    "size" => (policy.size_flexibility.as_ref(), a.size),
                    "tracking" => (policy.tracking_flexibility.as_ref(), a.tracking),
                    "baselineshift" => (policy.baseline_shift_flexibility.as_ref(), a.baseline_shift),
                    "leading" => {
                        let value = match a.leading {
                            Some(designcraft_doc::attrs::Leading::Points(v)) => Some(v),
                            _ => None,
                        };
                        (policy.leading_flexibility.as_ref(), value)
                    }
                    "hscale" => (policy.h_scale_flexibility.as_ref(), a.h_scale),
                    "kerning" => {
                        let value = match a.kerning {
                            Some(designcraft_doc::attrs::Kerning::Manual(v)) => Some(v),
                            Some(
                                designcraft_doc::attrs::Kerning::Metrics
                                | designcraft_doc::attrs::Kerning::Optical
                                | designcraft_doc::attrs::Kerning::None,
                            ) => Some(0.0),
                            None => None,
                        };
                        (policy.kerning_allowance.as_ref(), value)
                    }
                    _ => return Err(denied()),
                };
                let range = range.ok_or_else(denied)?;
                if !range.start.is_finite()
                    || !range.end.is_finite()
                    || range.start > range.end
                    || value.is_some_and(|v| !v.is_finite() || v < range.start || v > range.end)
                {
                    return Err(EngineError::BadParams { cmd: "type.char".into(), msg: "Value exceeds copyfit policy limits".into() });
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn command(s: &Session, id: &str, p: &Value) -> Result<()> {
    if s.editor_role_locked() && id == "file.newFromTemplate" && p.get("editorMode").and_then(Value::as_bool) == Some(false) {
        return Err(denied());
    }
    if !s.editor_mode {
        return Ok(());
    }
    let spec = crate::find_command(id);
    if matches!(id, "edit.undo" | "edit.redo") {
        let counts = s.active().and_then(|st| s.editor_history.entries.get(&st.uid)).copied().unwrap_or_default();
        if (id == "edit.undo" && counts.0 == 0) || (id == "edit.redo" && counts.1 == 0) {
            return Err(denied());
        }
    }
    let design_format = id.starts_with("style.") && spec.is_some_and(|c| c.undoable)
        || id.starts_with("type.") && !matches!(id, "type.char" | "type.sizeUp" | "type.sizeDown" | "type.changeCase" | "type.selectionAttrs");
    let structural = spec.is_some_and(|c| c.undoable)
        && id != "layout.rotateSpreadView"
        && (["layout.", "layer.", "path.", "line.", "frame.", "swatch.", "ink."].iter().any(|prefix| id.starts_with(prefix))
            || ["table.", "anchored.", "font.", "condition."].iter().any(|prefix| id.starts_with(prefix))
            || id.starts_with("object.") && !matches!(id, "object.fit" | "object.fittingOptions")
            || id.starts_with("story.") && !matches!(id, "story.setText" | "story.replaceRange" | "story.update")
            || id == "math.insert"
            || id.starts_with("transform.") && !s.active().is_some_and(|st| st.selection.content)
            || matches!(id, "text.thread" | "text.unthread" | "story.setDirection"));
    if design_format
        || structural
        || matches!(id, "object.lock" | "object.unlockAll" | "object.setFlags" | "page.editorial.approve")
        || id == "story.injectFields"
        || id == "story.update" && p.get("paragraphStyle").is_some()
        || id == "find.change" && p.get("attrs").is_some()
    {
        return Err(denied());
    }
    Ok(())
}

/// Validate a copy-on-write candidate before publishing it. Comparing the entire normalized
/// document makes unlisted commands obey the same rules, including explicitly named targets.
pub(crate) fn document(before: &Document, after: &Document) -> Result<()> {
    if before.stories.len() != after.stories.len() {
        return Err(denied());
    }
    for (id, story) in &before.stories {
        let next = after.story(*id).ok_or_else(denied)?;
        if story.frames != next.frames || story.direction != next.direction || story.vertical != next.vertical {
            return Err(denied());
        }
    }
    let mut normalized = after.clone();
    normalized.spreads = spreads(before, after, &before.spreads, &after.spreads, false)?;
    normalized.parents = spreads(before, after, &before.parents, &after.parents, true)?;
    normalized.stories = before.stories.clone();
    normalized.assets = before.assets.clone();
    normalized.next_id = before.next_id;
    normalized.modified = before.modified;
    normalized.title = before.title.clone();
    normalized.user_words = before.user_words.clone();
    if normalized != *before {
        return Err(denied());
    }
    Ok(())
}

fn spreads(before: &Document, after: &Document, old: &[Arc<Spread>], new: &[Arc<Spread>], protected: bool) -> Result<Vec<Arc<Spread>>> {
    if old.len() != new.len() {
        return Err(denied());
    }
    let mut result = Vec::with_capacity(new.len());
    for (a, b) in old.iter().zip(new) {
        let mut candidate = (**b).clone();
        // Rotation is a screen-only navigation preference, never a template dimension.
        for (page, original) in candidate.pages.iter_mut().zip(&a.pages) {
            page.view_rotation = original.view_rotation;
            if original.editorial.status != designcraft_doc::PageStatus::Approved && page.editorial.status != designcraft_doc::PageStatus::Approved {
                page.editorial = original.editorial.clone();
            }
        }
        candidate.items = items(before, after, a, &a.items, &b.items, protected, 0)?;
        if candidate != **a {
            return Err(denied());
        }
        result.push(a.clone());
    }
    Ok(result)
}

fn items(
    before: &Document,
    after: &Document,
    spread: &Spread,
    old: &[Arc<Item>],
    new: &[Arc<Item>],
    protected: bool,
    depth: usize,
) -> Result<Vec<Arc<Item>>> {
    if depth >= 128 || old.len() != new.len() {
        return Err(denied());
    }
    let mut result = Vec::with_capacity(new.len());
    for (a, b) in old.iter().zip(new) {
        let on_approved_page = spread
            .page_at_x(a.bounds().center().x)
            .and_then(|pi| spread.pages.get(pi))
            .is_some_and(|p| p.editorial.status == designcraft_doc::PageStatus::Approved);
        let protected =
            protected || on_approved_page || a.content_locked || a.hidden || before.layer(a.layer).is_some_and(|l| l.locked || !l.visible);
        if protected {
            match &a.content {
                Content::Text(tf) if before.story(tf.story) != after.story(tf.story) => return Err(denied()),
                Content::Graphic(g) if before.assets.get(&g.asset) != after.assets.get(&g.asset) => return Err(denied()),
                _ => {}
            }
        }
        let mut candidate = (**b).clone();
        match (&a.content, &b.content) {
            (Content::Group { items: aa }, Content::Group { items: bb }) => {
                candidate.content = Content::Group { items: items(before, after, spread, aa, bb, protected, depth + 1)? };
            }
            (Content::Graphic(_) | Content::Unassigned, Content::Graphic(_)) if !protected => {
                candidate.content = a.content.clone();
            }
            _ => {}
        }
        if candidate != **a {
            return Err(denied());
        }
        result.push(a.clone());
    }
    Ok(result)
}
