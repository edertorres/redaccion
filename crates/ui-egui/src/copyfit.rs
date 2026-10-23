//! Read-only copyfit feedback above the canvas. Uses the existing composition cache;
//! it never changes the document, frame geometry, styles or the story editor.

use designcraft_doc::StoryId;
use serde_json::json;

use crate::{DesignApp, ScreenMode, theme::Tokens};

pub use designcraft_engine::cmd::articles::compute_story_slack;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) story: StoryId,
    pub(crate) label: String,
    /// None means no overset.
    pub(crate) overflow_chars: Option<usize>,
    pub(crate) overflow_lines: usize,
    pub(crate) underflow_lines: usize,
    /// If a specific frame is selected in a threaded story: Some((frame_1_based_index, total_frames))
    pub(crate) frame_pos: Option<(usize, usize)>,
    pub(crate) flows_to_next: bool,
    pub(crate) is_empty_frame: bool,
    pub(crate) empty_frames_after: usize,
    pub(crate) total_story_underflow: usize,
}

fn status(app: &DesignApp) -> Option<Status> {
    let st = app.session.active()?;
    // The canvas follows its own selection, even when the story editor shows another story.
    // Multiple selected frames are unambiguous only when they share the same story.
    let (sid, selected_item_id) = if let Some(text) = &st.selection.text {
        let cs = app.session.cache.get(&st.doc, text.story, None);
        let frame_id = cs.frames.iter().find(|ft| ft.range.contains(&text.anchor) || (ft.range.is_empty() && text.anchor == 0)).map(|ft| ft.frame);
        (text.story, frame_id)
    } else {
        let mut stories = st.selection.items.iter().filter_map(|id| {
            let tf = st.doc.item(*id)?.text_frame()?;
            Some((tf.story, *id))
        });
        let first = stories.next()?;
        let mut all_same = true;
        for (s, _) in stories {
            if s != first.0 {
                all_same = false;
                break;
            }
        }
        if !all_same {
            return None;
        }
        let sel_item = if st.selection.items.len() == 1 { Some(st.selection.items[0]) } else { None };
        (first.0, sel_item)
    };

    let story = st.doc.story(sid)?;
    let cs = app.session.cache.get(&st.doc, sid, None);
    let total_frames = cs.frames.len();
    let selected_frame_idx = selected_item_id.and_then(|fid| cs.frames.iter().position(|ft| ft.frame == fid));
    let frame_pos = selected_frame_idx.map(|idx| (idx + 1, total_frames));

    let overflow_chars = cs.overset_at.map(|at| {
        // Defensive boundary handling also covers malformed offsets without slicing panics.
        story.text.char_indices().skip_while(|(byte, _)| *byte < at).count()
    });
    let overflow_lines =
        if let Some(at) = cs.overset_at { designcraft_engine::cmd::articles::estimate_overflow_lines(&cs, &story.text, at) } else { 0 };
    let (total_story_underflow, _) = designcraft_engine::cmd::articles::compute_story_slack(&cs);

    let mut flows_to_next = false;
    let mut is_empty_frame = false;
    let mut empty_frames_after = 0;
    let underflow_lines;

    if let Some(idx) = selected_frame_idx {
        if overflow_chars.is_some() {
            underflow_lines = 0;
        } else {
            let ft = cs.frames.get(idx)?;
            let lead_fallback = ft.lines.last().map(|l| if l.leading > 1.0 { l.leading } else { 14.4 }).unwrap_or(14.4);
            if ft.lines.is_empty() {
                is_empty_frame = true;
                let (f_lines, _) = designcraft_engine::cmd::articles::compute_frame_slack(ft, lead_fallback);
                underflow_lines = f_lines;
            } else {
                let has_subsequent_lines = cs.frames.iter().skip(idx + 1).any(|f| !f.lines.is_empty());
                if has_subsequent_lines {
                    flows_to_next = true;
                    underflow_lines = 0;
                } else {
                    let (f_lines, _) = designcraft_engine::cmd::articles::compute_frame_slack(ft, lead_fallback);
                    underflow_lines = f_lines;
                    empty_frames_after = total_frames.saturating_sub(idx + 1);
                }
            }
        }
    } else {
        underflow_lines = if overflow_chars.is_some() { 0 } else { total_story_underflow };
        if let Some(lfi) = cs.frames.iter().rposition(|ft| !ft.lines.is_empty()) {
            empty_frames_after = total_frames.saturating_sub(lfi + 1);
        }
    }

    Some(Status {
        story: sid,
        label: crate::story_editor::story_display_label(&st.doc, sid),
        overflow_chars,
        overflow_lines,
        underflow_lines,
        frame_pos,
        flows_to_next,
        is_empty_frame,
        empty_frames_after,
        total_story_underflow,
    })
}

pub(crate) fn show(app: &mut DesignApp, ui: &mut egui::Ui) {
    if matches!(app.ui.screen_mode, ScreenMode::Preview | ScreenMode::Presentation) || app.session.active().is_none() {
        return;
    }
    let current = status(app);
    let t = Tokens::get(ui.ctx());
    let lang = app.ui.language.clone();
    let tr = |key| crate::i18n::tr(&lang, key);
    let mut open = None;
    // Reserve space outside the canvas: no overlay can obscure the caret or intercept a
    // frame gesture. Keep the strip present without a selection to avoid viewport jumps.
    egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            crate::rtl::label(ui, egui::RichText::new(tr("Copyfit")).strong());
            if let Some(current) = &current {
                let frame_prefix = if let Some((curr, tot)) = current.frame_pos {
                    if tot > 1 {
                        format!("{}: ", tr("Frame {current}/{total}").replace("{current}", &curr.to_string()).replace("{total}", &tot.to_string()))
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let (dot, text, color) = match current.overflow_chars {
                    Some(chars) => {
                        let lines_txt = if current.overflow_lines == 1 {
                            tr("1 line doesn't fit").to_string()
                        } else {
                            tr("{lines} lines don't fit").replace("{lines}", &current.overflow_lines.to_string())
                        };
                        (
                            "🔴",
                            format!("! {frame_prefix}{} · {lines_txt}", tr("{chars} characters don't fit").replace("{chars}", &chars.to_string())),
                            t.copyfit_error(),
                        )
                    }
                    None if current.flows_to_next => ("🟢", format!("✓ {frame_prefix}{}", tr("Text flows to next frame")), t.copyfit_ok()),
                    None if current.is_empty_frame => (
                        "🟡",
                        format!("⚡ {frame_prefix}{} (~{} {})", tr("Empty frame"), current.underflow_lines, tr("lines capacity")),
                        t.copyfit_warn(),
                    ),
                    None if current.underflow_lines >= 1 => {
                        let msg = if let Some((curr, tot)) = current.frame_pos {
                            if tot > 1 {
                                let mut s = tr("Empty space in frame {current}/{total}: ~{lines} lines needed to fill frame")
                                    .replace("{current}", &curr.to_string())
                                    .replace("{total}", &tot.to_string())
                                    .replace("{lines}", &current.underflow_lines.to_string());
                                if current.empty_frames_after > 0 {
                                    s.push(' ');
                                    s.push_str(&tr("(+{count} empty frames follow)").replace("{count}", &current.empty_frames_after.to_string()));
                                }
                                s
                            } else {
                                format!("{} (~{} {})", tr("Underset (empty space)"), current.underflow_lines, tr("lines needed"))
                            }
                        } else {
                            format!("{} (~{} {})", tr("Underset (empty space)"), current.underflow_lines, tr("lines needed"))
                        };
                        ("🟡", format!("⚡ {msg}"), t.copyfit_warn())
                    }
                    None => {
                        let msg = if let Some((_curr, tot)) = current.frame_pos {
                            if tot > 1 && current.empty_frames_after > 0 {
                                format!(
                                    "{frame_prefix}{} {}",
                                    tr("Fits frame"),
                                    tr("(+{count} empty frames follow)").replace("{count}", &current.empty_frames_after.to_string())
                                )
                            } else {
                                format!("{frame_prefix}{}", tr("Fits frame"))
                            }
                        } else {
                            tr("Fits frame").to_string()
                        };
                        ("🟢", format!("✓ {msg}"), t.copyfit_ok())
                    }
                };
                crate::rtl::label(ui, egui::RichText::new(format!("{dot} {text}")).strong().color(color));
                if ui.button(crate::rtl::widget(ui, tr("Edit in Story Editor"))).clicked() {
                    open = Some(current.story);
                }
                ui.add(egui::Label::new(crate::rtl::widget(ui, egui::RichText::new(&current.label).color(t.text_dim))).truncate())
                    .on_hover_text(&current.label);
            } else {
                crate::rtl::label(ui, egui::RichText::new(tr("Select text or frames from one story")).color(t.text_dim));
            }
        });
    });
    if let Some(sid) = open {
        // Existing UI command: same semantics and control-channel access as the menu.
        let _ = app.run("app.storyEditor", json!({"story": sid.0}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use designcraft_engine::Session;

    fn app_with_frame(text: &str) -> (DesignApp, serde_json::Value) {
        let mut app = DesignApp::new(Session::new(), Services::default());
        app.run("file.new", json!({})).unwrap();
        let frame = app.run("frame.create", json!({"rect": [36, 36, 180, 80], "content": "text", "text": text})).unwrap();
        (app, frame)
    }

    #[test]
    fn copyfit_tracks_edits_undo_redo_without_mutating_document() {
        let (mut app, frame) = app_with_frame("Breve");
        let before = app.session.active().unwrap().doc.clone();
        assert_eq!(status(&app).unwrap().overflow_chars, None);
        assert!(std::sync::Arc::ptr_eq(&before, &app.session.active().unwrap().doc));
        let long = "Crónica de la edición 🙂 漢字. ".repeat(150);
        app.run("story.update", json!({"story": frame["story"], "text": long})).unwrap();
        app.run("selection.set", json!({"ids": [frame["id"]]})).unwrap();
        let overflow = status(&app).unwrap().overflow_chars.unwrap();
        let st = app.session.active().unwrap();
        let sid = StoryId(frame["story"].as_u64().unwrap());
        let cs = app.session.cache.get(&st.doc, sid, None);
        assert_eq!(overflow, st.doc.story(sid).unwrap().text.get(cs.overset_at.unwrap()..).unwrap().chars().count());
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(status(&app).unwrap().overflow_chars, None);
        app.run("edit.redo", json!({})).unwrap();
        assert_eq!(status(&app).unwrap().overflow_chars, Some(overflow));
    }

    #[test]
    fn canvas_selection_is_independent_of_story_editor_and_ambiguous_selections() {
        let (mut app, first) = app_with_frame("");
        assert_eq!(status(&app).unwrap().overflow_chars, None);
        let second = app.run("frame.create", json!({"rect": [220, 36, 360, 80], "content": "text", "text": "Otra historia"})).unwrap();
        app.story_editor = Some(StoryId(first["story"].as_u64().unwrap()));
        assert_eq!(status(&app).unwrap().story.0, second["story"].as_u64().unwrap());
        app.run("selection.set", json!({"ids": [first["id"], second["id"]]})).unwrap();
        assert!(status(&app).is_none());
        app.run("selection.set", json!({"ids": []})).unwrap();
        assert!(status(&app).is_none());
        app.run("text.select", json!({"story": first["story"], "anchor": 0})).unwrap();
        assert_eq!(status(&app).unwrap().story.0, first["story"].as_u64().unwrap());
    }

    #[test]
    fn threaded_story_reports_overflow_from_any_selected_frame() {
        let (mut app, first) = app_with_frame(&"Información ".repeat(300));
        let second = app.run("text.thread", json!({"from": first["id"], "rect": [220, 36, 360, 80]})).unwrap();
        let end = status(&app).unwrap();
        assert_eq!(end.frame_pos, Some((2, 2)));
        app.run("selection.set", json!({"ids": [first["id"]]})).unwrap();
        let first_st = status(&app).unwrap();
        assert_eq!(first_st.overflow_chars, end.overflow_chars);
        assert_eq!(first_st.overflow_lines, end.overflow_lines);
        assert_eq!(first_st.frame_pos, Some((1, 2)));
        app.run("selection.set", json!({"ids": [first["id"], second["to"]]})).unwrap();
        assert!(status(&app).unwrap().overflow_chars.is_some());
    }

    #[test]
    fn threaded_story_first_frame_fitted_when_text_continues_to_next() {
        let text = "Linea 1.\nLinea 2.\nLinea 3.\nLinea 4.\nLinea 5.\nLinea 6.\nLinea 7.\nLinea 8.\nLinea 9.\nLinea 10.";
        let (mut app, first) = app_with_frame(text);
        let second = app.run("text.thread", json!({"from": first["id"], "rect": [220, 36, 360, 250]})).unwrap();
        app.run("selection.set", json!({"ids": [first["id"]]})).unwrap();
        let st1 = status(&app).unwrap();
        assert_eq!(st1.frame_pos, Some((1, 2)));
        assert!(st1.flows_to_next, "Frame 1 must report that text flows to next frame");
        assert_eq!(st1.underflow_lines, 0, "Frame 1 must not report underflow when text flows out");

        app.run("selection.set", json!({"ids": [second["to"]]})).unwrap();
        let st2 = status(&app).unwrap();
        assert_eq!(st2.frame_pos, Some((2, 2)));
        assert!(!st2.flows_to_next);
        assert!(st2.underflow_lines >= 1, "Frame 2 has empty space so it reports underflow lines");
    }

    #[test]
    fn threaded_story_short_text_reports_only_frame_slack_not_subsequent_empty_frames() {
        let (mut app, first) = app_with_frame("Solo dos lineas breves.\nAqui termina.");
        let second = app.run("text.thread", json!({"from": first["id"], "rect": [220, 36, 360, 200]})).unwrap();

        // Select Frame 1: Frame 1 has some empty space, and Frame 2 is completely empty.
        app.run("selection.set", json!({"ids": [first["id"]]})).unwrap();
        let st1 = status(&app).unwrap();
        assert_eq!(st1.frame_pos, Some((1, 2)));
        assert!(!st1.flows_to_next);
        assert_eq!(st1.empty_frames_after, 1);
        // Crucial check: Frame 1 slack must NOT include the empty 200pt Frame 2 (~14 lines)!
        assert!(
            st1.underflow_lines <= 4,
            "Frame 1 must report its own slack (~2-3 lines), not the empty 200pt Frame 2! Got: {}",
            st1.underflow_lines
        );

        // Select Frame 2: Frame 2 is empty
        app.run("selection.set", json!({"ids": [second["to"]]})).unwrap();
        let st2 = status(&app).unwrap();
        assert_eq!(st2.frame_pos, Some((2, 2)));
        assert!(st2.is_empty_frame);
        assert!(st2.underflow_lines >= 10, "Frame 2 capacity must reflect 200pt frame height");
    }

    #[test]
    fn redactor_can_open_the_selected_story_without_changing_the_document() {
        use egui_kittest::{Harness, kittest::Queryable};

        let (mut app, frame) = app_with_frame("Noticia");
        app.session.editor_mode = true;
        let before = app.session.active().unwrap().doc.clone();
        let mut harness = Harness::builder().with_size(egui::vec2(620.0, 100.0)).build_ui_state(|ui, app| show(app, ui), app);
        harness.get_by_label("Edit in Story Editor").click();
        harness.run();
        assert_eq!(harness.state().story_editor, Some(StoryId(frame["story"].as_u64().unwrap())));
        assert!(std::sync::Arc::ptr_eq(&before, &harness.state().session.active().unwrap().doc));
    }

    #[test]
    fn preview_and_presentation_reserve_no_copyfit_space() {
        let (mut app, _) = app_with_frame("Noticia");
        let ctx = egui::Context::default();
        for mode in [ScreenMode::Normal, ScreenMode::Preview, ScreenMode::Presentation] {
            app.ui.screen_mode = mode;
            let mut height = 0.0;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let before = ui.cursor().min.y;
                show(&mut app, ui);
                height = ui.cursor().min.y - before;
            });
            output.textures_delta.clear();
            if mode == ScreenMode::Normal {
                assert!(height > 0.0);
            } else {
                assert_eq!(height, 0.0);
            }
        }
    }

    #[test]
    fn narrow_copyfit_strip_supports_every_theme_without_editing_content() {
        use egui_kittest::{Harness, kittest::Queryable};

        for brightness in crate::theme::Brightness::ALL {
            let (app, _) = app_with_frame(&"Edición diaria. ".repeat(100));
            let before = app.session.active().unwrap().doc.clone();
            let tokens = Tokens::for_brightness(brightness);
            let mut initialized = false;
            let mut harness = Harness::builder().with_size(egui::vec2(320.0, 200.0)).build_ui_state(
                move |ui, app| {
                    if !initialized {
                        crate::theme::apply(ui.ctx(), &tokens);
                        initialized = true;
                    }
                    show(app, ui);
                },
                app,
            );
            harness.run();
            harness.get_by_label("Edit in Story Editor");
            assert!(std::sync::Arc::ptr_eq(&before, &harness.state().session.active().unwrap().doc));
        }
    }

    #[test]
    fn copyfit_three_state_traffic_light() {
        // State 1: Yellow / Underset (empty frame or short text)
        let (mut app, frame) = app_with_frame("Una linea");
        let st_yellow = status(&app).unwrap();
        assert_eq!(st_yellow.overflow_chars, None);
        assert!(st_yellow.underflow_lines >= 1, "short text in 80pt frame must report underflow lines >= 1");

        // State 2: Red / Overset (too much text)
        let long = "Demasiado texto para caber en este marco de ochenta puntos. ".repeat(30);
        app.run("story.update", json!({"story": frame["story"], "text": long})).unwrap();
        app.run("selection.set", json!({"ids": [frame["id"]]})).unwrap();
        let st_red = status(&app).unwrap();
        assert!(st_red.overflow_chars.is_some(), "long text must report overflow chars");
        assert_eq!(st_red.underflow_lines, 0);

        // State 3: Green / Fitted (tuned text filling the frame)
        let tuned = "Linea 1.\nLinea 2.";
        app.run("story.update", json!({"story": frame["story"], "text": tuned})).unwrap();
        let st_green = status(&app).unwrap();
        assert_eq!(st_green.overflow_chars, None);
        assert_eq!(st_green.underflow_lines, 0);
    }
}
