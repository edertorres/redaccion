//! Window › Articles: named reading-order lists of objects (EPUB / HTML export order).

use designcraft_doc::{Article, ItemId};
use serde_json::{Value, json};

use super::{CommandSpec, bad, cmd, has_doc, ids_param, ok, str_param};
use crate::Session;

fn sel_ids(s: &Session, p: &Value) -> Vec<ItemId> {
    ids_param(p, "ids").unwrap_or_else(|| s.active().map(|d| d.selection.items.clone()).unwrap_or_default())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "article.list", "Articles", [], None, "{} → [{name, items, export}]", has_doc, |s, _| Ok(serde_json::to_value(&s.doc()?.doc.articles).unwrap_or_default())),
        cmd!("article.new", "New Article", ["Window", "Articles"], None, "{name?, ids? (default: the selection)} → {name}", has_doc, |s, p| {
            let ids = sel_ids(s, p);
            let want = str_param(p, "name").map(str::to_string);
            s.edit(|d, _| {
                let name = match want {
                    Some(n) if d.articles.iter().any(|a| a.name == n) => return Err(bad("article.new", format!("an article named `{n}` exists"))),
                    Some(n) => n,
                    None => (1..=d.articles.len() + 1)
                        .map(|i| format!("Article {i}"))
                        .find(|n| !d.articles.iter().any(|a| a.name == *n))
                        .ok_or_else(|| bad("article.new", "no free article name"))?,
                };
                d.articles.push(Article { name: name.clone(), items: ids.into_iter().filter(|i| d.item(*i).is_some()).collect(), export: true });
                Ok(json!({"name": name}))
            })
        }),
        cmd!("article.add", "Add Selection to Article", [], None, "{name, ids? (default: the selection)} — appended in order", has_doc, |s, p| {
            let ids = sel_ids(s, p);
            let name = str_param(p, "name").unwrap_or("").to_string();
            s.edit(|d, _| {
                let valid: Vec<ItemId> = ids.into_iter().filter(|i| d.item(*i).is_some()).collect();
                let a = d.articles.iter_mut().find(|a| a.name == name).ok_or_else(|| bad("article.add", format!("no article `{name}`")))?;
                for i in valid {
                    if !a.items.contains(&i) {
                        a.items.push(i);
                    }
                }
                ok()
            })
        }),
        cmd!("article.remove", "Remove from Article", [], None, "{name, ids}", has_doc, |s, p| {
            let ids = sel_ids(s, p);
            let name = str_param(p, "name").unwrap_or("").to_string();
            s.edit(|d, _| {
                let a = d.articles.iter_mut().find(|a| a.name == name).ok_or_else(|| bad("article.remove", format!("no article `{name}`")))?;
                a.items.retain(|i| !ids.contains(i));
                ok()
            })
        }),
        cmd!("article.options", "Article Options", [], None, "{name, to?: new name, export?: bool, order?: [ids] (reorder)}", has_doc, |s, p| {
            let name = str_param(p, "name").unwrap_or("").to_string();
            let to = str_param(p, "to").map(str::to_string);
            let export = p.get("export").and_then(Value::as_bool);
            let order = ids_param(p, "order");
            s.edit(|d, _| {
                let a = d.articles.iter_mut().find(|a| a.name == name).ok_or_else(|| bad("article.options", format!("no article `{name}`")))?;
                if let Some(e) = export {
                    a.export = e;
                }
                if let Some(o) = order {
                    let mut items: Vec<ItemId> = o.into_iter().filter(|i| a.items.contains(i)).collect();
                    items.extend(a.items.iter().filter(|i| !items.contains(i)).copied().collect::<Vec<_>>());
                    a.items = items;
                }
                if let Some(t) = to {
                    a.name = t;
                }
                ok()
            })
        }),
        cmd!("article.delete", "Delete Article", [], None, "{name}", has_doc, |s, p| {
            let name = str_param(p, "name").unwrap_or("").to_string();
            s.edit(|d, _| {
                let n = d.articles.len();
                d.articles.retain(|a| a.name != name);
                if n == d.articles.len() {
                    return Err(bad("article.delete", format!("no article `{name}`")));
                }
                ok()
            })
        }),
        cmd!(query "article.get", "Get Article", [], None, "{name?} → {name, items, stories}", has_doc, |s, p| {
            let doc = &s.doc()?.doc;
            let want = str_param(p, "name");
            if let Some(name) = want {
                let a = doc.articles.iter().find(|a| a.name == name);
                if let Some(art) = a {
                    let mut stories = Vec::new();
                    for &id in &art.items {
                        if let Some(it) = doc.item(id)
                            && let designcraft_doc::Content::Text(tf) = &it.content
                        {
                            let cs = s.cache.get(doc, tf.story, None);
                            let mut info = compute_copyfit(doc, &cs, tf.story);
                            if let Some(st) = doc.story(tf.story) {
                                info["text"] = json!(st.text);
                            }
                            stories.push(info);
                        }
                    }
                    return Ok(json!({
                        "name": art.name,
                        "items": art.items.iter().map(|i| i.0).collect::<Vec<_>>(),
                        "export": art.export,
                        "stories": stories,
                    }));
                }
                // Check if a text frame has this name or label
                for sp in &doc.spreads {
                    for it in &sp.items {
                        if (it.name == name || it.name.eq_ignore_ascii_case(name) || it.label == name)
                            && let designcraft_doc::Content::Text(tf) = &it.content
                        {
                            let cs = s.cache.get(doc, tf.story, None);
                            let mut info = compute_copyfit(doc, &cs, tf.story);
                            if let Some(st) = doc.story(tf.story) {
                                info["text"] = json!(st.text);
                            }
                            return Ok(json!({
                                "name": it.name,
                                "items": [it.id.0],
                                "export": true,
                                "stories": [info],
                            }));
                        }
                    }
                }
                return Err(bad("article.get", format!("no article or frame `{name}`")));
            }

            // Return all articles in the document
            let mut list = Vec::new();
            for art in &doc.articles {
                let mut stories = Vec::new();
                for &id in &art.items {
                    if let Some(it) = doc.item(id)
                        && let designcraft_doc::Content::Text(tf) = &it.content
                    {
                        let cs = s.cache.get(doc, tf.story, None);
                        stories.push(compute_copyfit(doc, &cs, tf.story));
                    }
                }
                list.push(json!({
                    "name": art.name,
                    "items": art.items.iter().map(|i| i.0).collect::<Vec<_>>(),
                    "export": art.export,
                    "stories": stories,
                }));
            }
            Ok(Value::Array(list))
        }),
        cmd!("article.update", "Update Article", [], None, "{name, text? | fields?: {name: text}} → {ok, article, copyfit}", has_doc, |s, p| {
            let name = str_param(p, "name").ok_or_else(|| bad("article.update", "missing article name"))?.to_string();
            let text_opt = str_param(p, "text");
            let fields_opt = p.get("fields").and_then(Value::as_object);
            let mut touched_stories = Vec::new();

            s.edit(|d, _| {
                let mut matched_sids = Vec::new();
                if let Some(art) = d.articles.iter().find(|a| a.name == name) {
                    for &id in &art.items {
                        if let Some(it) = d.item(id)
                            && let designcraft_doc::Content::Text(tf) = &it.content
                        {
                            matched_sids.push((it.name.clone(), tf.story));
                        }
                    }
                } else {
                    for sp in &d.spreads {
                        for it in &sp.items {
                            if (it.name == name || it.name.eq_ignore_ascii_case(&name) || it.label == name)
                                && let designcraft_doc::Content::Text(tf) = &it.content
                            {
                                matched_sids.push((it.name.clone(), tf.story));
                            }
                        }
                    }
                }

                if matched_sids.is_empty() {
                    return Err(bad("article.update", format!("no article or text frame `{name}`")));
                }

                if let Some(text) = text_opt {
                    for (_, sid) in &matched_sids {
                        d.set_story_text(*sid, text)?;
                        touched_stories.push(*sid);
                    }
                } else if let Some(fields) = fields_opt {
                    for (k, v) in fields {
                        let val_str = v.as_str().unwrap_or("");
                        for (fname, sid) in &matched_sids {
                            if fname.eq_ignore_ascii_case(k) || k == &name {
                                d.set_story_text(*sid, val_str)?;
                                touched_stories.push(*sid);
                            }
                        }
                    }
                }
                ok()
            })?;

            let doc = &s.doc()?.doc;
            let mut copyfits = Vec::new();
            for sid in touched_stories {
                let cs = s.cache.get(doc, sid, None);
                copyfits.push(compute_copyfit(doc, &cs, sid));
            }

            Ok(json!({
                "ok": true,
                "article": name,
                "copyfit": copyfits,
            }))
        }),
        cmd!(query "story.list", "List Stories", [], None, "{} → [{id, name, words, chars, lines, overset, overflowLines, snippet}]", has_doc, |s, _| {
            let doc = &s.doc()?.doc;
            let mut list = Vec::new();
            for &sid in doc.stories.keys() {
                let cs = s.cache.get(doc, sid, None);
                let mut info = compute_copyfit(doc, &cs, sid);
                if let Some(st) = doc.story(sid) {
                    let snippet: String = st.text.chars().take(60).collect();
                    info["snippet"] = json!(snippet);
                }
                list.push(info);
            }
            list.sort_by_key(|v| v["id"].as_u64().unwrap_or(0));
            Ok(Value::Array(list))
        }),
        cmd!("story.update", "Update Story", [], None, "{story, text?, paragraphStyle?} → {ok, story, copyfit}", has_doc, |s, p| {
            let sid = p.get("story").and_then(Value::as_u64).ok_or_else(|| bad("story.update", "missing story id"))?;
            let story_id = designcraft_doc::StoryId(sid);
            let text_opt = str_param(p, "text");
            let style_opt = str_param(p, "paragraphStyle");

            s.edit(|d, _| {
                if d.story(story_id).is_none() {
                    return Err(bad("story.update", format!("story `{sid}` not found")));
                }
                if let Some(text) = text_opt {
                    d.set_story_text(story_id, text)?;
                }
                if let Some(style) = style_opt
                    && let Some(st) = d.story_mut(story_id)
                {
                    for p in &mut st.paras {
                        p.style = style.to_string();
                    }
                }
                ok()
            })?;

            let doc = &s.doc()?.doc;
            let cs = s.cache.get(doc, story_id, None);
            let copyfit = compute_copyfit(doc, &cs, story_id);

            Ok(json!({
                "ok": true,
                "story": sid,
                "copyfit": copyfit,
            }))
        }),
        cmd!(
            "story.injectFields",
            "Inject Structured Fields",
            [],
            None,
            "{fields: {name: text|{text,style?}|base64}} → {ok, updated, copyfit}",
            has_doc,
            |s, p| {
                let fields = p.get("fields").and_then(Value::as_object).ok_or_else(|| bad("story.injectFields", "missing fields object"))?;
                let mut updated_keys = Vec::new();
                let mut touched_stories = Vec::new();

                s.edit(|d, _| {
                    for (key, val) in fields {
                        let mut matched_items = Vec::new();
                        for sp in &d.spreads {
                            for it in &sp.items {
                                if it.name.eq_ignore_ascii_case(key) || it.label.eq_ignore_ascii_case(key) {
                                    matched_items.push(it.id);
                                }
                            }
                        }

                        for fid in matched_items {
                            let (is_text, sid_opt, is_graphic) = if let Some(it) = d.item(fid) {
                                match &it.content {
                                    designcraft_doc::Content::Text(tf) => (true, Some(tf.story), false),
                                    designcraft_doc::Content::Graphic(_) => (false, None, true),
                                    _ => (false, None, false),
                                }
                            } else {
                                continue;
                            };

                            if is_text && let Some(sid) = sid_opt {
                                let (text, style_opt) = match val {
                                    Value::String(s) => (s.as_str(), None),
                                    Value::Object(obj) => {
                                        let t = obj.get("text").and_then(Value::as_str).unwrap_or("");
                                        let sty = obj.get("style").and_then(Value::as_str);
                                        (t, sty)
                                    }
                                    other => (other.as_str().unwrap_or(""), None),
                                };

                                d.set_story_text(sid, text)?;
                                if let Some(sty) = style_opt
                                    && d.styles.para(sty).is_some()
                                    && let Some(st) = d.story_mut(sid)
                                {
                                    for p in &mut st.paras {
                                        p.style = sty.to_string();
                                    }
                                }
                                if !touched_stories.contains(&sid) {
                                    touched_stories.push(sid);
                                }
                                if !updated_keys.contains(key) {
                                    updated_keys.push(key.clone());
                                }
                            } else if is_graphic && let Some(str_val) = val.as_str() {
                                let b64 = if let Some(stripped) = str_val.strip_prefix("data:") {
                                    stripped.split(',').nth(1).unwrap_or(str_val)
                                } else {
                                    str_val
                                };
                                let bytes = super::base64_decode(b64);
                                if !bytes.is_empty() {
                                    let aid = designcraft_doc::AssetId(d.alloc());
                                    let mime = designcraft_render::image_mime(&bytes).to_string();
                                    let (w, h) = designcraft_render::image_size(&bytes).unwrap_or((100, 100));
                                    d.assets.insert(
                                        aid,
                                        std::sync::Arc::new(designcraft_doc::Asset {
                                            page: 0,
                                            id: aid,
                                            name: format!("{key}.png"),
                                            mime,
                                            link: None,
                                            data: std::sync::Arc::new(bytes),
                                            pixels: None,
                                        }),
                                    );
                                    if let Some(it) = d.item_mut(fid) {
                                        it.content = designcraft_doc::Content::Graphic(designcraft_doc::Graphic {
                                            asset: aid,
                                            size: (w as f64, h as f64),
                                            xf: designcraft_geom::Affine::IDENTITY,
                                            auto_fit: designcraft_doc::Fitting::FillProportionally,
                                            fit_align: 4,
                                            crop: [0.0; 4],
                                        });
                                    }
                                    if !updated_keys.contains(key) {
                                        updated_keys.push(key.clone());
                                    }
                                }
                            }
                        }
                    }
                    ok()
                })?;

                let doc = &s.doc()?.doc;
                let mut copyfit_results = Vec::new();
                for sid in touched_stories {
                    let cs = s.cache.get(doc, sid, None);
                    copyfit_results.push(compute_copyfit(doc, &cs, sid));
                }

                Ok(json!({
                    "ok": true,
                    "updated": updated_keys,
                    "copyfit": copyfit_results,
                }))
            }
        ),
        cmd!(query "document.copyfit", "Document Copyfit Status", [], None, "{} → {hasOverset, totalOverflowLines, stories}", has_doc, |s, _| {
            let doc = &s.doc()?.doc;
            let mut list = Vec::new();
            let mut has_overset = false;
            let mut total_overflow = 0;
            for &sid in doc.stories.keys() {
                let cs = s.cache.get(doc, sid, None);
                let info = compute_copyfit(doc, &cs, sid);
                if info["overset"].as_bool().unwrap_or(false) {
                    has_overset = true;
                    total_overflow += info["overflowLines"].as_u64().unwrap_or(0) as usize;
                }
                list.push(info);
            }
            Ok(json!({
                "hasOverset": has_overset,
                "totalOverflowLines": total_overflow,
                "stories": list,
            }))
        }),
    ]
}

pub(crate) fn story_frame_name(doc: &designcraft_doc::Document, sid: designcraft_doc::StoryId) -> String {
    for sp in &doc.spreads {
        for it in &sp.items {
            if let designcraft_doc::Content::Text(tf) = &it.content
                && tf.story == sid
                && !it.name.is_empty()
            {
                return it.name.clone();
            }
        }
    }
    format!("Story #{}", sid.0)
}

pub fn estimate_overflow_lines(cs: &designcraft_compose::ComposedStory, story_text: &str, overset_byte: usize) -> usize {
    let o_safe = if overset_byte <= story_text.len() {
        let mut b = overset_byte;
        while b > 0 && !story_text.is_char_boundary(b) {
            b -= 1;
        }
        b
    } else {
        story_text.len()
    };
    let overset_str = story_text.get(o_safe..).unwrap_or("");
    let overset_chars = overset_str.chars().count();
    if overset_chars == 0 {
        return 1;
    }

    let col_w = cs.frames.iter().flat_map(|f| &f.columns).map(|c| c.width()).find(|w| *w > 10.0).unwrap_or(200.0);
    let font_size = cs.frames.iter().flat_map(|f| &f.lines).map(|l| l.ascent + l.descent).find(|h| *h > 6.0).unwrap_or(12.0);
    let char_w = (font_size * 0.50).clamp(4.0, 12.0);
    let avg_cpl = (col_w / char_w).round().clamp(20.0, 100.0);

    let mut total_lines = 0;
    for paragraph in overset_str.lines() {
        let p_chars = paragraph.chars().count();
        if p_chars == 0 {
            total_lines += 1;
        } else {
            let l = ((p_chars as f64) / avg_cpl).ceil() as usize;
            total_lines += l.max(1);
        }
    }
    total_lines.max(1)
}

pub fn compute_frame_slack(ft: &designcraft_compose::FrameText, lead_fallback: f64) -> (usize, f64) {
    if ft.columns.is_empty() {
        return (0, 0.0);
    }
    let (lead, asc, desc) = if let Some(l) = ft.lines.last() {
        let lead = if l.leading > 1.0 { l.leading } else { lead_fallback.max(12.0) };
        (lead, l.ascent.max(8.0), l.descent.max(2.0))
    } else {
        (lead_fallback.max(12.0), lead_fallback * 0.75, lead_fallback * 0.25)
    };

    if ft.lines.is_empty() {
        let mut total_lines = 0;
        let mut total_slack = 0.0;
        for col in &ft.columns {
            let h = col.height().max(0.0);
            total_slack += h;
            if h >= asc + desc {
                total_lines += 1 + ((h - (asc + desc)) / lead).floor() as usize;
            }
        }
        return (total_lines, total_slack);
    }

    let Some(last_line) = ft.lines.last() else {
        return (0, 0.0);
    };

    let last_col = (last_line.column as usize).min(ft.columns.len().saturating_sub(1));
    let mut slack = 0.0;
    let mut lines = 0;

    if let Some(col) = ft.columns.get(last_col) {
        let bottom = last_line.baseline + last_line.descent;
        let rem = (col.y1 - bottom).max(0.0);
        slack += rem;
        lines += (rem / lead).floor() as usize;
    }

    for col in ft.columns.iter().skip(last_col + 1) {
        let h = col.height().max(0.0);
        slack += h;
        if h >= asc + desc {
            lines += 1 + ((h - (asc + desc)) / lead).floor() as usize;
        }
    }

    (lines, slack)
}

pub fn compute_story_slack(cs: &designcraft_compose::ComposedStory) -> (usize, f64) {
    if cs.frames.is_empty() {
        return (0, 0.0);
    }
    let last_fi = cs.frames.iter().rposition(|ft| !ft.lines.is_empty());
    let Some(lfi) = last_fi else {
        let lead = 14.4;
        let mut lines = 0;
        let mut slack = 0.0;
        for ft in &cs.frames {
            let (f_lines, f_slack) = compute_frame_slack(ft, lead);
            lines += f_lines;
            slack += f_slack;
        }
        return (lines, slack);
    };

    let lead = cs.frames[lfi].lines.last().map(|l| if l.leading > 1.0 { l.leading } else { 14.4 }).unwrap_or(14.4);

    let (active_frame_lines, active_frame_slack) = compute_frame_slack(&cs.frames[lfi], lead);
    let mut remaining_frames_lines = 0;
    let mut remaining_frames_slack = 0.0;
    for next_ft in cs.frames.iter().skip(lfi + 1) {
        let (f_lines, f_slack) = compute_frame_slack(next_ft, lead);
        remaining_frames_lines += f_lines;
        remaining_frames_slack += f_slack;
    }

    (active_frame_lines + remaining_frames_lines, active_frame_slack + remaining_frames_slack)
}

pub(crate) fn compute_copyfit(doc: &designcraft_doc::Document, cs: &designcraft_compose::ComposedStory, sid: designcraft_doc::StoryId) -> Value {
    let Some(st) = doc.story(sid) else {
        return json!({
            "id": sid.0,
            "name": format!("Story #{}", sid.0),
            "words": 0,
            "chars": 0,
            "lines": 0,
            "overset": false,
            "overflowLines": 0,
            "underflowLines": 0,
            "slackPoints": 0.0,
            "status": "fitted"
        });
    };
    let overset = cs.overset_at.is_some();
    let overflow_lines = if let Some(o) = cs.overset_at { estimate_overflow_lines(cs, &st.text, o) } else { 0 };
    let (underflow_lines, slack_points) = if overset { (0, 0.0) } else { compute_story_slack(cs) };
    let status = if overset {
        "overset"
    } else if underflow_lines >= 1 {
        "underset"
    } else {
        "fitted"
    };
    json!({
        "id": sid.0,
        "name": story_frame_name(doc, sid),
        "words": st.text.split_whitespace().count(),
        "chars": st.text.chars().count(),
        "lines": cs.line_count(),
        "overset": overset,
        "overflowLines": overflow_lines,
        "underflowLines": underflow_lines,
        "slackPoints": (slack_points * 10.0).round() / 10.0,
        "status": status,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn articles_set_the_export_reading_order() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let a =
            s.execute("frame.create", &json!({"rect": [72, 72, 300, 100], "content": "text", "text": "Top story", "caret": false})).unwrap()["id"]
                .clone();
        let b =
            s.execute("frame.create", &json!({"rect": [72, 400, 300, 430], "content": "text", "text": "Lower story", "caret": false})).unwrap()["id"]
                .clone();
        let html = |s: &mut Session| s.execute("file.exportHtml", &json!({})).unwrap()["text"].as_str().unwrap().to_string();
        let h = html(&mut s);
        assert!(h.find("Top").unwrap() < h.find("Lower").unwrap(), "page order without articles");
        s.execute("article.new", &json!({"name": "Main", "ids": [b, a]})).unwrap();
        let h = html(&mut s);
        assert!(h.find("Lower").unwrap() < h.find("Top").unwrap(), "article order");
        s.execute("article.remove", &json!({"name": "Main", "ids": [a]})).unwrap();
        assert!(!html(&mut s).contains("Top story"), "only articles' objects are exported");
        s.execute("article.options", &json!({"name": "Main", "export": false})).unwrap();
        assert!(html(&mut s).contains("Top story"), "no exported articles: page order again");
    }

    #[test]
    fn article_get_update_and_copyfit() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        let f1 = s
            .execute("frame.create", &json!({"rect": [72, 72, 300, 150], "content": "text", "text": "Initial headline", "name": "Titular"}))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        let f2 = s
            .execute("frame.create", &json!({"rect": [72, 200, 300, 400], "content": "text", "text": "Initial body paragraph", "name": "Cuerpo"}))
            .unwrap()["id"]
            .as_u64()
            .unwrap();

        // Create an article grouping them
        s.execute("article.new", &json!({"name": "Portada", "ids": [f1, f2]})).unwrap();

        // Query article
        let get_res = s.execute("article.get", &json!({"name": "Portada"})).unwrap();
        assert_eq!(get_res["name"], "Portada");
        assert_eq!(get_res["stories"].as_array().unwrap().len(), 2);

        // Update article text
        let upd_res = s.execute("article.update", &json!({"name": "Portada", "text": "Updated text for the whole story"})).unwrap();
        assert_eq!(upd_res["ok"], true);
        assert_eq!(upd_res["copyfit"].as_array().unwrap().len(), 2);

        // Check document copyfit
        let cf = s.execute("document.copyfit", &json!({})).unwrap();
        assert!(cf["stories"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn story_inject_fields_structured() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("frame.create", &json!({"rect": [72, 72, 300, 150], "content": "text", "name": "Titular"})).unwrap();
        s.execute("frame.create", &json!({"rect": [72, 200, 300, 400], "content": "text", "name": "Bajada"})).unwrap();

        let inj = s
            .execute(
                "story.injectFields",
                &json!({
                    "fields": {
                        "Titular": "Acuerdo histórico en el congreso",
                        "Bajada": "Tras 14 horas de debate se aprueba la reforma"
                    }
                }),
            )
            .unwrap();

        assert_eq!(inj["ok"], true);
        let updated = inj["updated"].as_array().unwrap();
        assert!(updated.iter().any(|v| v == "Titular"));
        assert!(updated.iter().any(|v| v == "Bajada"));

        let copyfits = inj["copyfit"].as_array().unwrap();
        assert_eq!(copyfits.len(), 2);
        assert_eq!(copyfits[0]["overset"], false);
    }

    #[test]
    fn story_list_get_update_and_copyfit() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("frame.create", &json!({"rect": [72, 72, 300, 150], "content": "text", "name": "Cuerpo", "text": "Texto inicial"})).unwrap();

        // story.list
        let list = s.execute("story.list", &json!({})).unwrap();
        let stories = list.as_array().unwrap();
        assert_eq!(stories.len(), 1);
        let sid = stories[0]["id"].as_u64().unwrap();
        assert_eq!(stories[0]["name"], "Cuerpo");
        assert_eq!(stories[0]["snippet"], "Texto inicial");

        // story.get
        let get = s.execute("story.get", &json!({"story": sid})).unwrap();
        assert_eq!(get["text"], "Texto inicial");
        assert_eq!(get["name"], "Cuerpo");
        assert_eq!(get["copyfit"]["words"], 2);
        assert_eq!(get["copyfit"]["status"], "underset");
        assert!(get["copyfit"]["underflowLines"].as_u64().unwrap() >= 1);

        // story.update
        let upd = s.execute("story.update", &json!({"story": sid, "text": "Nuevo contenido ampliado para el redactor"})).unwrap();
        assert_eq!(upd["ok"], true);
        assert_eq!(upd["story"], sid);
        assert_eq!(upd["copyfit"]["words"], 6);
        assert_eq!(upd["copyfit"]["status"], "underset");

        // verify updated text via story.get
        let get2 = s.execute("story.get", &json!({"story": sid})).unwrap();
        assert_eq!(get2["text"], "Nuevo contenido ampliado para el redactor");
        assert_eq!(get2["copyfit"]["status"], "underset");
    }

    #[test]
    fn copyfit_slack_and_overflow_estimation() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        // Frame of height 200 pt
        let f1 = s.execute("frame.create", &json!({"rect": [72, 72, 300, 272], "content": "text", "text": "Linea 1\nLinea 2"})).unwrap();
        let sid = designcraft_doc::StoryId(f1["story"].as_u64().unwrap());
        let doc = s.doc().unwrap();
        let cs = s.cache.get(&doc.doc, sid, None);
        let (lines, slack) = super::compute_story_slack(&cs);
        assert!(lines >= 8, "2 short lines in 200pt frame should leave ~9-11 lines slack, got {lines}");
        assert!(slack > 100.0);

        // Estimate overflow lines
        let short_text = "Linea corta.\n".repeat(10);
        let ov_lines = super::estimate_overflow_lines(&cs, &short_text, 0);
        assert_eq!(ov_lines, 10);
    }
}
