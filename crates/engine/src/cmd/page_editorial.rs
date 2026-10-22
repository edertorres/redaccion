//! Layout › Editorial: Page workflow status, assignment, approval and whole-edition overview.

use std::time::{SystemTime, UNIX_EPOCH};

use designcraft_doc::{Content, Document, Item, PageStatus, SpreadRef};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, ok, str_param};
use crate::Result;
use crate::Session;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "page.editorial.get",
            "Page Editorial Status",
            [],
            None,
            "{page?: usize} → {page, pageNumber, status, section, assignedTo, approvedBy, approvedAt, notes}",
            has_doc,
            get_status
        ),
        cmd!(
            "page.editorial.set",
            "Set Page Editorial Metadata",
            ["Layout", "Pages"],
            None,
            "{page?: usize, status?: string, section?: string, assignedTo?: string, notes?: string}",
            has_doc,
            set_status
        ),
        cmd!(
            "page.editorial.approve",
            "Approve Page for Press",
            ["Layout", "Pages"],
            None,
            "{page?: usize, approved: bool, by?: string} — locks page from editor edits",
            has_doc,
            approve_page
        ),
        cmd!(
            query "page.editorial.overview",
            "Edition Overview",
            [],
            None,
            "{} → {pageCount, approvedCount, oversetCount, pages: [...]}",
            has_doc,
            overview
        ),
    ]
}

fn resolve_target_page(s: &Session, p: &Value) -> Result<usize> {
    let d = &s.doc()?.doc;
    if let Some(i) = p.get("page").and_then(Value::as_u64) {
        let idx = i as usize;
        if idx < d.page_count() {
            return Ok(idx);
        }
        return Err(bad("page.editorial", format!("page index {idx} out of bounds")));
    }
    // If an item is selected, resolve to its page; otherwise default to 0.
    if let Some(it_id) = s.doc()?.selection.items.first()
        && let Some(loc) = d.find(*it_id)
        && let Some(it) = d.item(*it_id)
        && let Some(pg) = page_of(d, loc.spread, it)
    {
        return Ok(pg);
    }
    Ok(0)
}

fn page_of(d: &Document, si: SpreadRef, it: &Item) -> Option<usize> {
    let SpreadRef::Doc(si) = si else { return None };
    let sp = d.spreads.get(si)?;
    Some(d.first_page_of_spread(si) + sp.page_at_x(it.bounds().center().x).unwrap_or(0))
}

fn get_status(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let idx = resolve_target_page(s, p)?;
    let page = st.doc.page(idx).ok_or_else(|| bad("page.editorial.get", "page not found"))?;
    let label = st.doc.page_name(idx);
    let ed = &page.editorial;
    Ok(json!({
        "page": idx,
        "pageNumber": label,
        "editorialPageNumber": ed.page_number,
        "editionDate": ed.edition_date,
        "status": ed.status.as_str(),
        "section": ed.section.as_deref().or_else(|| st.doc.section_of(idx).map(|sec| sec.prefix.as_str())),
        "assignedTo": ed.assigned_to,
        "approvedBy": ed.approved_by,
        "approvedAt": ed.approved_at,
        "notes": ed.notes,
    }))
}

fn upsert_text_var(vars: &mut Vec<designcraft_doc::vars::TextVariable>, name: &str, val: &str) {
    if let Some(v) = vars.iter_mut().find(|v| v.name.eq_ignore_ascii_case(name)) {
        v.kind = designcraft_doc::vars::VarKind::Custom { text: val.to_string() };
    } else if vars.len() < designcraft_doc::vars::VAR_MAX {
        vars.push(designcraft_doc::vars::TextVariable::new(name, designcraft_doc::vars::VarKind::Custom { text: val.to_string() }));
    }
}

fn set_status(s: &mut Session, p: &Value) -> Result<Value> {
    let idx = resolve_target_page(s, p)?;
    let new_status = str_param(p, "status").and_then(PageStatus::parse);
    let section = str_param(p, "section").map(str::to_string);
    let assigned_to = str_param(p, "assignedTo").map(str::to_string);
    let notes = str_param(p, "notes").map(str::to_string);
    let edition_date = str_param(p, "editionDate").map(str::to_string);
    let page_number = p.get("pageNumber").and_then(Value::as_u64).map(|n| n as u32);

    let editor_mode = s.editor_mode;

    s.edit(|d, _| {
        let page = d.page_mut(idx).ok_or_else(|| bad("page.editorial.set", "page not found"))?;

        // If the page is already approved and the user is an editor, reject any modifications.
        if editor_mode && page.editorial.status == PageStatus::Approved {
            return Err(bad("page.editorial.set", "Page is approved and closed for edits"));
        }

        // An editor cannot promote a page to Approved directly (only Designers / Editors-in-Chief can).
        if editor_mode && new_status == Some(PageStatus::Approved) {
            return Err(bad("page.editorial.set", "Only designers or editors-in-chief can approve a page"));
        }

        if let Some(st) = new_status {
            page.editorial.status = st;
        }
        if let Some(sec) = section {
            page.editorial.section = if sec.trim().is_empty() { None } else { Some(sec) };
        }
        if let Some(who) = assigned_to {
            page.editorial.assigned_to = if who.trim().is_empty() { None } else { Some(who) };
        }
        if let Some(nt) = notes {
            page.editorial.notes = if nt.trim().is_empty() { None } else { Some(nt) };
        }
        if let Some(ed_date) = edition_date {
            page.editorial.edition_date = if ed_date.trim().is_empty() { None } else { Some(ed_date) };
        }
        if let Some(p_num) = page_number {
            page.editorial.page_number = Some(p_num);
        }

        let date_opt = page.editorial.edition_date.clone();
        let sec_opt = page.editorial.section.clone();
        let pnum_opt = page.editorial.page_number;

        // Synchronize text variables in document
        if let Some(ref date) = date_opt {
            upsert_text_var(&mut d.text_variables, "Fecha", date);
            upsert_text_var(&mut d.text_variables, "Fecha de edición", date);
        }
        if let Some(ref sec) = sec_opt {
            upsert_text_var(&mut d.text_variables, "Sección", sec);
        }
        if let Some(pnum) = pnum_opt {
            upsert_text_var(&mut d.text_variables, "Página", &format!("{pnum}"));
            upsert_text_var(&mut d.text_variables, "Número de página", &format!("{pnum:02}"));
        }

        // Replace template placeholders in stories on this document
        for story in d.stories.values_mut() {
            let mut text = story.text.clone();
            let mut changed = false;
            if let Some(ref date) = date_opt {
                let replaced = text.replace("[FECHA]", date).replace("[Fecha]", date);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if let Some(ref sec) = sec_opt {
                let replaced = text.replace("[SECCION]", sec).replace("[SECCIÓN]", sec).replace("[Sección]", sec);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if let Some(pnum) = pnum_opt {
                let pstr = format!("{pnum:02}");
                let replaced = text.replace("[PAGINA]", &pstr).replace("[PÁGINA]", &pstr).replace("[Página]", &pstr);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if changed {
                let mut modified_story = (**story).clone();
                modified_story.text = text;
                *story = std::sync::Arc::new(modified_story);
            }
        }

        ok()
    })
}

fn approve_page(s: &mut Session, p: &Value) -> Result<Value> {
    if s.editor_mode {
        return Err(bad("page.editorial.approve", "Only designers or editors-in-chief can approve pages"));
    }

    let idx = resolve_target_page(s, p)?;
    let approve = p.get("approved").and_then(Value::as_bool).unwrap_or(true);
    let by = str_param(p, "by").unwrap_or("Editor").to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

    s.edit(|d, _| {
        let page = d.page_mut(idx).ok_or_else(|| bad("page.editorial.approve", "page not found"))?;
        if approve {
            page.editorial.status = PageStatus::Approved;
            page.editorial.approved_by = Some(by);
            page.editorial.approved_at = Some(now);
        } else {
            page.editorial.status = PageStatus::NeedsReview;
            page.editorial.approved_by = None;
            page.editorial.approved_at = None;
        }
        Ok(json!({
            "page": idx,
            "status": page.editorial.status.as_str(),
            "approved": approve
        }))
    })
}

fn overview(s: &mut Session, _: &Value) -> Result<Value> {
    let st = s.doc()?;
    let d = &st.doc;
    let n_pages = d.page_count();

    // Map which pages have overset stories.
    let mut pages_with_overset = std::collections::HashSet::new();
    for story in d.stories.values() {
        let cs = s.cache.get(d, story.id, None);
        if cs.is_overset() {
            let last = story.frames.last().copied();
            if let Some(page_idx) = last.and_then(|f| d.find(f).zip(d.item(f))).and_then(|(loc, it)| page_of(d, loc.spread, it)) {
                pages_with_overset.insert(page_idx);
            }
        }
    }

    // Count items per page
    let mut stories_per_page = vec![0usize; n_pages];
    let mut graphics_per_page = vec![0usize; n_pages];
    for (si, sp) in d.spreads.iter().enumerate() {
        let first_p = d.first_page_of_spread(si);
        for it in &sp.items {
            let p_offset = sp.page_at_x(it.bounds().center().x).unwrap_or(0);
            let abs_page = first_p + p_offset;
            if abs_page < n_pages {
                match &it.content {
                    Content::Text(_) => stories_per_page[abs_page] = stories_per_page[abs_page].saturating_add(1),
                    Content::Graphic(_) => graphics_per_page[abs_page] = graphics_per_page[abs_page].saturating_add(1),
                    _ => {}
                }
            }
        }
    }

    let mut approved_count = 0usize;
    let mut overset_count = 0usize;
    let mut pages_out = Vec::with_capacity(n_pages);

    for (abs_idx, _, page) in d.pages() {
        let is_approved = page.editorial.status == PageStatus::Approved;
        if is_approved {
            approved_count = approved_count.saturating_add(1);
        }
        let has_overset = pages_with_overset.contains(&abs_idx);
        if has_overset {
            overset_count = overset_count.saturating_add(1);
        }

        let section_fallback = d.section_of(abs_idx).map(|sec| sec.prefix.clone());
        let section = page.editorial.section.clone().or(section_fallback);

        pages_out.push(json!({
            "page": abs_idx,
            "pageNumber": d.page_name(abs_idx),
            "editorialPageNumber": page.editorial.page_number,
            "editionDate": page.editorial.edition_date,
            "side": match page.side {
                designcraft_doc::PageSide::Single => "single",
                designcraft_doc::PageSide::Left => "left",
                designcraft_doc::PageSide::Right => "right",
            },
            "status": page.editorial.status.as_str(),
            "section": section,
            "assignedTo": page.editorial.assigned_to,
            "approvedBy": page.editorial.approved_by,
            "approvedAt": page.editorial.approved_at,
            "notes": page.editorial.notes,
            "hasOverset": has_overset,
            "storiesCount": stories_per_page.get(abs_idx).copied().unwrap_or(0),
            "graphicsCount": graphics_per_page.get(abs_idx).copied().unwrap_or(0),
        }));
    }

    Ok(json!({
        "pageCount": n_pages,
        "approvedCount": approved_count,
        "oversetCount": overset_count,
        "pages": pages_out
    }))
}
