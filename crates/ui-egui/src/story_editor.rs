//! Story Editor (Edit → Edit in Story Editor, ⌘Y): the story as plain text with each paragraph's
//! style in the left column and the overset point marked. Edits are applied as minimal range
//! replacements, so formatting around them is kept.

use egui::vec2;

use crate::DesignApp;
use crate::theme::Tokens;

pub fn show(app: &mut DesignApp, ctx: &egui::Context) {
    let Some(sid) = app.story_editor else { return };

    let mut open = true;
    let title = app
        .session
        .active()
        .and_then(|st| {
            let label = story_display_label(&st.doc, sid);
            let story = st.doc.story(sid)?;
            let words = story.text.split_whitespace().count();
            Some(format!("✍ {} — {} {}", label, words, crate::i18n::tr(&app.ui.language, "words")))
        })
        .unwrap_or_else(|| crate::i18n::tr(&app.ui.language, "Story Editor").to_string());

    egui::Window::new(title)
        .id(egui::Id::new("story_editor"))
        .open(&mut open)
        .default_pos(egui::pos2(40.0, 75.0))
        .default_size(vec2(520.0, 460.0))
        .min_size(vec2(340.0, 220.0))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            show_window_contents(app, ui, sid);
        });

    if !open {
        app.story_editor = None;
    }
}

fn show_window_contents(app: &mut DesignApp, ui: &mut egui::Ui, sid: designcraft_doc::StoryId) {
    let stories_count = app.session.active().map(|st| st.doc.stories.len()).unwrap_or(0);
    let is_editor = app.session.editor_mode;

    // Window top bar: Story selector and quick actions
    ui.horizontal(|ui| {
        if stories_count > 1 {
            let current_label = app.session.active().map(|st| story_display_label(&st.doc, sid)).unwrap_or_default();
            egui::ComboBox::from_id_salt("story_picker_dropdown").selected_text(current_label).width(240.0).show_ui(ui, |ui| {
                if let Some(st) = app.session.active() {
                    for &other_sid in st.doc.stories.keys() {
                        let label = story_display_label(&st.doc, other_sid);
                        if ui.selectable_label(app.story_editor == Some(other_sid), label).clicked() {
                            app.story_editor = Some(other_sid);
                        }
                    }
                }
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if is_editor {
                if ui
                    .button(egui::RichText::new("🖨 PDF").color(egui::Color32::from_rgb(120, 210, 255)).strong())
                    .on_hover_text("Exportar PDF/X-4 listo para imprenta con sangrados y marcas")
                    .clicked()
                {
                    let _ = app.run("app.exportPressPdf", serde_json::json!({}));
                }
                if ui.button(egui::RichText::new(format!("💾 {}", crate::i18n::tr(&app.ui.language, "Save"))).strong()).clicked() {
                    let _ = app.run("app.save", serde_json::json!({}));
                }
            }
            if ui.small_button("A+").on_hover_text("Aumentar tamaño de texto").clicked() {
                app.ui.story_editor_size = (app.ui.story_editor_size + 2.0).min(32.0);
            }
            if ui.small_button("A-").on_hover_text("Disminuir tamaño de texto").clicked() {
                app.ui.story_editor_size = (app.ui.story_editor_size - 2.0).max(10.0);
            }
        });
    });
    ui.add_space(2.0);
    show_inner(app, ui);
}

pub(crate) fn story_display_label(doc: &designcraft_doc::Document, sid: designcraft_doc::StoryId) -> String {
    let mut script_tag = None;
    let mut frame_name = None;

    if let Some(s) = doc.story(sid) {
        for &fid in &s.frames {
            if let Some(it) = doc.item(fid) {
                if script_tag.is_none() && !it.label.is_empty() {
                    script_tag = Some(it.label.clone());
                }
                if frame_name.is_none() && !it.name.is_empty() {
                    frame_name = Some(it.name.clone());
                }
            }
        }
    }

    if script_tag.is_none() && frame_name.is_none() {
        for sp in doc.spreads.iter().chain(doc.parents.iter()) {
            for it in &sp.items {
                it.walk(&mut |i| {
                    if let designcraft_doc::Content::Text(tf) = &i.content
                        && tf.story == sid
                    {
                        if script_tag.is_none() && !i.label.is_empty() {
                            script_tag = Some(i.label.clone());
                        }
                        if frame_name.is_none() && !i.name.is_empty() {
                            frame_name = Some(i.name.clone());
                        }
                    }
                });
                if script_tag.is_some() {
                    break;
                }
            }
            if script_tag.is_some() {
                break;
            }
        }
    }

    let prefix = script_tag.or(frame_name);

    let first_line = doc.story(sid).and_then(|s| {
        let l = s.text.lines().next()?.trim();
        if l.is_empty() { None } else { Some(l.chars().take(22).collect::<String>()) }
    });

    match (prefix, first_line) {
        (Some(p), Some(text)) => format!("{}: \"{}\"", p, text),
        (Some(p), None) => p,
        (None, Some(text)) => format!("Story #{}: \"{}\"", sid.0, text),
        (None, None) => format!("Story #{}", sid.0),
    }
}

pub(crate) fn safe_char_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i = i.saturating_sub(1);
    }
    i
}

fn show_inner(app: &mut DesignApp, ui: &mut egui::Ui) {
    let Some(sid) = app.story_editor else { return };
    let Some(st) = app.session.active() else {
        app.story_editor = None;
        return;
    };
    let Some(story) = st.doc.story(sid).cloned() else {
        app.story_editor = None;
        return;
    };
    let t = Tokens::get(ui.ctx());
    let is_editor = app.session.editor_mode;
    let cs = app.session.cache.get(&st.doc, sid, None);
    let content_editable = app.session.editorial_story_editable(sid);
    let overset = cs.overset_at;
    let para_styles: Vec<String> = st.doc.styles.paragraph.iter().map(|p| p.name.clone()).collect();
    let para_shortcuts: Vec<(String, String)> =
        st.doc.styles.paragraph.iter().filter(|p| !p.shortcut.is_empty()).map(|p| (p.name.clone(), p.shortcut.clone())).collect();
    let mut edit: Option<(usize, usize, String, bool)> = None;
    let mut style_change: Option<(usize, String)> = None;

    let base_size = app.ui.story_editor_size.clamp(8.0, 36.0);

    // Real-time Copyfit & Overset Banner (sticky at the top of the editor)
    let words_count = story.text.split_whitespace().count();
    let chars_count = story.text.chars().count();
    let (underflow_lines, _) = crate::copyfit::compute_story_slack(&cs);

    ui.horizontal(|ui| {
        if let Some(o) = overset {
            let o_safe = safe_char_boundary(&story.text, o);
            let overset_str = story.text.get(o_safe..).unwrap_or("");
            let chars_overset = overset_str.chars().count();
            let lines_overset = designcraft_engine::cmd::articles::estimate_overflow_lines(&cs, &story.text, o);

            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(140, 32, 32))
                .corner_radius(4.0)
                .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🔴").strong());
                        ui.label(egui::RichText::new(crate::i18n::tr(&app.ui.language, "OVERSET")).color(egui::Color32::WHITE).strong());
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                crate::i18n::tr(&app.ui.language, "{lines} lines / {chars} characters don't fit")
                                    .replace("{lines}", &lines_overset.to_string())
                                    .replace("{chars}", &chars_overset.to_string()),
                            )
                            .color(egui::Color32::from_rgb(255, 210, 210))
                            .strong(),
                        );
                    });
                });
        } else if underflow_lines >= 1 {
            let total_frames = cs.frames.len();
            let lfi = cs.frames.iter().rposition(|ft| !ft.lines.is_empty());
            let (active_lines, empty_after) = if let Some(fi) = lfi {
                let lead = cs.frames[fi].lines.last().map(|l| if l.leading > 1.0 { l.leading } else { 14.4 }).unwrap_or(14.4);
                let (al, _) = designcraft_engine::cmd::articles::compute_frame_slack(&cs.frames[fi], lead);
                (al, total_frames.saturating_sub(fi + 1))
            } else {
                (underflow_lines, 0)
            };

            let underflow_msg = if total_frames > 1 && empty_after > 0 {
                let current_frame = lfi.unwrap_or(0) + 1;
                let frame_part = crate::i18n::tr(&app.ui.language, "Empty space in frame {current}/{total}: ~{lines} lines needed to fill frame")
                    .replace("{current}", &current_frame.to_string())
                    .replace("{total}", &total_frames.to_string())
                    .replace("{lines}", &active_lines.to_string());
                let empty_part = crate::i18n::tr(&app.ui.language, "(+{count} empty frames follow)").replace("{count}", &empty_after.to_string());
                format!("{frame_part} {empty_part}")
            } else {
                crate::i18n::tr(&app.ui.language, "Empty space: ~{lines} lines needed to fill frame").replace("{lines}", &underflow_lines.to_string())
            };

            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(130, 85, 15))
                .corner_radius(4.0)
                .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🟡").strong());
                        ui.label(
                            egui::RichText::new(crate::i18n::tr(&app.ui.language, "Underset (empty space)")).color(egui::Color32::WHITE).strong(),
                        );
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(underflow_msg).color(egui::Color32::from_rgb(255, 235, 170)).strong());
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                crate::i18n::tr(&app.ui.language, "{words} words, {chars} characters")
                                    .replace("{words}", &words_count.to_string())
                                    .replace("{chars}", &chars_count.to_string()),
                            )
                            .color(egui::Color32::from_rgb(240, 220, 160))
                            .size(11.5),
                        );
                    });
                });
        } else {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(25, 75, 38))
                .corner_radius(4.0)
                .inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 4 })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🟢").strong());
                        ui.label(egui::RichText::new(crate::i18n::tr(&app.ui.language, "Fits frame")).color(egui::Color32::WHITE).strong());
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                crate::i18n::tr(&app.ui.language, "{words} words, {chars} characters")
                                    .replace("{words}", &words_count.to_string())
                                    .replace("{chars}", &chars_count.to_string()),
                            )
                            .color(egui::Color32::from_rgb(190, 230, 205))
                            .size(11.5),
                        );
                    });
                });
        }
    });
    ui.add_space(4.0);
    if is_editor && content_editable {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Microfit Text"))).clicked() {
                let _ = app.run("text.microfit", serde_json::json!({"story": sid.0, "max_excess_words": 50}));
            }
            if let Some(attrs) = crate::panels::story_text_attrs(app, sid) {
                crate::taskbar::show_copyfit_adjustments(app, ui, sid, &attrs, "se_cf");
            }
        });
        ui.add_space(4.0);
    }
    if !content_editable {
        crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Protected content")).strong().color(t.text_dim));
    }
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            if !is_editor {
                // Style column (for designers only; redactors cannot apply styles).
                ui.vertical(|ui| {
                    ui.set_width(135.0);
                    for (i, r) in story.para_ranges().iter().enumerate() {
                        let lines = story.slice(r.clone()).len() / 60 + 1;
                        let style = &story.paras[i].style;
                        let mut chosen = style.clone();
                        egui::ComboBox::from_id_salt(("se_para_style", sid.0, i))
                            .selected_text(crate::rtl::widget(ui, egui::RichText::new(crate::i18n::style_name(&app.ui.language, style)).size(11.0)))
                            .width(130.0)
                            .show_ui(ui, |ui| {
                                for sname in &para_styles {
                                    if ui
                                        .selectable_label(sname == style, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, sname)))
                                        .clicked()
                                    {
                                        chosen = sname.clone();
                                    }
                                }
                            });
                        if chosen != *style {
                            style_change = Some((i, chosen));
                        }
                        for _ in 1..lines {
                            ui.label(egui::RichText::new(" ").size(11.0));
                        }
                    }
                });
                ui.separator();
            }
            let mut text = story.text.clone();

            let font = egui::FontId::proportional(base_size);
            let text_color = egui::Color32::from_rgb(20, 20, 20);
            let overset_color = egui::Color32::from_rgb(220, 35, 35);

            let output = egui::Frame::NONE
                .fill(egui::Color32::WHITE)
                .corner_radius(4.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(215, 220, 228)))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.visuals_mut().override_text_color = Some(text_color);
                    ui.visuals_mut().selection.bg_fill = egui::Color32::from_rgb(180, 215, 255);
                    ui.visuals_mut().selection.stroke = egui::Stroke::NONE;

                    egui::TextEdit::multiline(&mut text)
                        .id(egui::Id::new(("story_editor_text", sid)))
                        .interactive(content_editable)
                        .font(font.clone())
                        .desired_width(f32::INFINITY)
                        .desired_rows(16)
                        .frame(egui::Frame::NONE)
                        .layouter(&mut |ui, string, wrap_width| {
                            let string = string.as_str();
                            let str_len = string.len();
                            let mut job = egui::text::LayoutJob::default();

                            let append_segment = |job: &mut egui::text::LayoutJob,
                                                  raw_start: usize,
                                                  raw_end: usize,
                                                  color: egui::Color32,
                                                  st: egui::Stroke,
                                                  ul: egui::Stroke| {
                                let start = safe_char_boundary(string, raw_start);
                                let end = safe_char_boundary(string, raw_end);
                                if start >= end {
                                    return;
                                }

                                if let Some(o_raw) = overset {
                                    let o = safe_char_boundary(string, o_raw);
                                    if o <= start {
                                        if let Some(slice) = string.get(start..end) {
                                            job.append(
                                                slice,
                                                0.0,
                                                egui::TextFormat {
                                                    font_id: font.clone(),
                                                    color: overset_color,
                                                    strikethrough: st,
                                                    underline: ul,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        return;
                                    } else if o < end {
                                        if let Some(s1) = string.get(start..o) {
                                            job.append(
                                                s1,
                                                0.0,
                                                egui::TextFormat {
                                                    font_id: font.clone(),
                                                    color,
                                                    strikethrough: st,
                                                    underline: ul,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        if let Some(s2) = string.get(o..end) {
                                            job.append(
                                                s2,
                                                0.0,
                                                egui::TextFormat {
                                                    font_id: font.clone(),
                                                    color: overset_color,
                                                    strikethrough: st,
                                                    underline: ul,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        return;
                                    }
                                }
                                if let Some(slice) = string.get(start..end) {
                                    job.append(
                                        slice,
                                        0.0,
                                        egui::TextFormat { font_id: font.clone(), color, strikethrough: st, underline: ul, ..Default::default() },
                                    );
                                }
                            };

                            if string == story.text {
                                for (run_range, fmt) in story.runs() {
                                    if run_range.start >= str_len {
                                        continue;
                                    }
                                    let start = run_range.start;
                                    let end = run_range.end.min(str_len);
                                    if start >= end {
                                        continue;
                                    }

                                    let mut color = text_color;
                                    let mut strikethrough = egui::Stroke::NONE;
                                    let mut underline = egui::Stroke::NONE;

                                    if let Some(mark) = &fmt.over.change {
                                        if mark.is_inserted() {
                                            color = egui::Color32::from_rgb(25, 140, 25);
                                            underline = egui::Stroke::new(1.0, color);
                                        } else if mark.is_deleted() {
                                            color = egui::Color32::from_rgb(190, 30, 30);
                                            strikethrough = egui::Stroke::new(1.0, color);
                                        }
                                    }
                                    append_segment(&mut job, start, end, color, strikethrough, underline);
                                }
                            } else {
                                // User is actively typing: layout string directly to prevent stale runs mismatch
                                append_segment(&mut job, 0, str_len, text_color, egui::Stroke::NONE, egui::Stroke::NONE);
                            }

                            job.wrap.max_width = wrap_width;
                            ui.ctx().fonts_mut(|f| f.layout_job(job))
                        })
                        .show(ui)
                })
                .inner;
            let r = output.response;

            let paste_event_text = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) => Some(t.clone()),
                    _ => None,
                })
            });

            if r.changed() {
                // Minimal diff: common prefix and suffix.
                let old = &story.text;
                let pre = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count();
                let pre = (0..=pre).rev().find(|&i| old.is_char_boundary(i) && text.is_char_boundary(i)).unwrap_or(0);
                let max_suf = old.len().min(text.len()) - pre;
                let suf = old.bytes().rev().zip(text.bytes().rev()).take(max_suf).take_while(|(a, b)| a == b).count();
                let suf = (0..=suf).rev().find(|&k| old.is_char_boundary(old.len() - k) && text.is_char_boundary(text.len() - k)).unwrap_or(0);

                let clear_format = ui.input(|i| i.modifiers.shift);
                let new_slice = text.get(pre..text.len().saturating_sub(suf)).unwrap_or("");
                edit = Some((pre, old.len().saturating_sub(suf), new_slice.to_string(), clear_format));
            } else if content_editable && let Some(pasted) = paste_event_text {
                let cursor_idx: usize = output.cursor_range.as_ref().map(|cr| cr.primary.index.into()).unwrap_or(story.text.len());
                let pos = safe_char_boundary(&story.text, cursor_idx);
                let clear_format = ui.input(|i| i.modifiers.shift);
                edit = Some((pos, pos, pasted, clear_format));
            }
            if !is_editor
                && r.has_focus()
                && let Some(cursor_idx) = output.cursor_range.as_ref().map(|cr| cr.primary.index)
            {
                let c_idx: usize = cursor_idx.into();
                let para_idx = story.para_at(c_idx.min(story.text.len()));
                let events = ui.input(|i| i.events.clone());
                for e in events {
                    let egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } = e else { continue };
                    if !modifiers.command && !modifiers.ctrl && !modifiers.alt {
                        continue;
                    }
                    for (sname, sc) in &para_shortcuts {
                        if let Some((m, k)) = crate::menus::parse_shortcut(sc)
                            && k == key
                            && crate::menus::modifiers_match(m, modifiers)
                        {
                            style_change = Some((para_idx, sname.clone()));
                            break;
                        }
                    }
                }
            }
        });
        if let Some(o) = overset {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(crate::rtl::widget(
                    ui,
                    egui::RichText::new(crate::i18n::tr(&app.ui.language, "OVERSET")).color(egui::Color32::from_rgb(230, 40, 40)).strong(),
                ));
                let o_safe = safe_char_boundary(&story.text, o);
                let overset_str = story.text.get(o_safe..).unwrap_or("");
                let chars_overset = overset_str.chars().count();
                // Predictive line counter based on newlines and an average line length of 45 characters
                let lines_overset = (overset_str.matches('\n').count() + (chars_overset / 45)).max(1);

                crate::rtl::label(
                    ui,
                    egui::RichText::new(
                        crate::i18n::tr(&app.ui.language, "{lines} lines / {chars} characters don't fit")
                            .replace("{lines}", &lines_overset.to_string())
                            .replace("{chars}", &chars_overset.to_string()),
                    )
                    .color(t.text_dim),
                );
            });
        }

        // Track changes: Accept/Reject
        let mut selected_change = None;
        if let Ok(d) = app.session.doc()
            && let Some(t) = &d.selection.text
            && t.story == sid
            && let Some((r, f)) = story
                .runs()
                .find(|(r, f)| f.over.change.as_ref().is_some_and(|c| c.is_inserted() || c.is_deleted()) && r.start <= t.anchor && t.anchor < r.end)
            && let Some(mark) = &f.over.change
        {
            selected_change = Some((r.start, mark.clone()));
        }
        if let Some((start, mark)) = selected_change {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let author = match &mark {
                    designcraft_doc::ChangeMark::Inserted(a) => a.clone(),
                    designcraft_doc::ChangeMark::Deleted(a) => a.clone(),
                    _ => String::new(),
                };
                let desc = if mark.is_inserted() { format!("Inserted by {}", author) } else { format!("Deleted by {}", author) };
                ui.label(egui::RichText::new(desc).color(t.text_dim));
                if ui.add(egui::Button::new(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Accept")))).clicked() {
                    let _ = app.run("changes.accept", serde_json::json!({"story": sid.0, "start": start}));
                }
                if ui.add(egui::Button::new(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Reject")))).clicked() {
                    let _ = app.run("changes.reject", serde_json::json!({"story": sid.0, "start": start}));
                }
            });
        }
    });
    let had_changes = style_change.is_some() || edit.is_some();
    if let Some((pi, new_style)) = style_change {
        let _ = app.run("style.paragraph.apply", serde_json::json!({"story": sid.0, "paraIndex": pi, "name": new_style}));
    }
    if let Some((a, b, text, clear_format)) = edit {
        let _ = app.run("story.replaceRange", serde_json::json!({"story": sid.0, "start": a, "end": b, "text": text, "clearFormat": clear_format}));
    }
    if had_changes {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use designcraft_doc::build::NewDocument;
    use designcraft_doc::{Document, ParaFormat, SpreadRef};
    use designcraft_geom::kurbo::Rect;

    #[test]
    fn story_display_label_uses_script_label() {
        let mut doc = Document::new(&NewDocument::default());
        let (fid, sid) = doc
            .add_text_frame(
                SpreadRef::Doc(0),
                Rect::new(0.0, 0.0, 100.0, 100.0),
                doc.default_layer(),
                "Breaking news headline here",
                ParaFormat::default(),
            )
            .unwrap();
        doc.item_mut(fid).unwrap().label = "TITULO1".to_string();

        let label = story_display_label(&doc, sid);
        assert_eq!(label, "TITULO1: \"Breaking news headline\"");
    }

    #[test]
    fn paste_in_story_editor_under_editorial_mode() {
        use egui_kittest::Harness;
        let mut app = DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", serde_json::json!({})).unwrap();
        let frame_a = app.run("frame.create", serde_json::json!({"rect":[36,36,240,160],"content":"text","text":"Historia A"})).unwrap();
        let frame_b = app.run("frame.create", serde_json::json!({"rect":[36,200,240,160],"content":"text","text":"Historia B"})).unwrap();
        let sid_a = designcraft_doc::StoryId(frame_a["story"].as_u64().unwrap());
        let sid_b = designcraft_doc::StoryId(frame_b["story"].as_u64().unwrap());
        // Canvas selection is Frame A
        app.run("selection.set", serde_json::json!({"ids": [frame_a["id"]]})).unwrap();
        // Story editor is viewing Story B!
        app.story_editor = Some(sid_b);
        app.session.restrict_to_editor();
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
            |ui, app| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.run();
        h.event(egui::Event::Paste(" Pegado".into()));
        h.run();
        let text_a = h.state().session.active().unwrap().doc.story(sid_a).unwrap().text.clone();
        let text_b = h.state().session.active().unwrap().doc.story(sid_b).unwrap().text.clone();
        assert_eq!(text_a, "Historia A", "Canvas selection leaked paste into Frame A!");
        assert!(text_b.contains("Pegado"), "Story B in story editor did not get paste! Text was: '{text_b}'");
    }
}
