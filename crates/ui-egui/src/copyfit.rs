//! Read-only copyfit feedback above the canvas. Uses the existing composition cache;
//! it never changes the document, frame geometry, styles or the story editor.

use designcraft_doc::StoryId;
use serde_json::json;

use crate::{DesignApp, ScreenMode, theme::Tokens};

#[derive(Debug, PartialEq, Eq)]
struct Status {
    story: StoryId,
    label: String,
    /// None means no overset, not that the frame is exactly full.
    overflow_chars: Option<usize>,
}

fn status(app: &DesignApp) -> Option<Status> {
    let st = app.session.active()?;
    // The canvas follows its own selection, even when the story editor shows another story.
    // Multiple selected frames are unambiguous only when they share the same story.
    let sid = if let Some(text) = st.selection.text {
        text.story
    } else {
        let mut stories = st.selection.items.iter().map(|id| st.doc.item(*id)?.text_frame().map(|tf| tf.story));
        let first = stories.next()??;
        if !stories.all(|sid| sid == Some(first)) {
            return None;
        }
        first
    };
    let story = st.doc.story(sid)?;
    let cs = app.session.cache.get(&st.doc, sid, None);
    let overflow_chars = cs.overset_at.map(|at| {
        // Defensive boundary handling also covers malformed offsets without slicing panics.
        story.text.char_indices().skip_while(|(byte, _)| *byte < at).count()
    });
    Some(Status { story: sid, label: crate::story_editor::story_display_label(&st.doc, sid), overflow_chars })
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
                let (text, color) = match current.overflow_chars {
                    Some(chars) => (
                        format!("! {} · {}", tr("Text doesn't fit"), tr("{chars} characters don't fit").replace("{chars}", &chars.to_string())),
                        t.copyfit_error(),
                    ),
                    None => (format!("✓ {}", tr("Fits frame")), t.copyfit_ok()),
                };
                crate::rtl::label(ui, egui::RichText::new(text).strong().color(color));
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
        app.run("selection.set", json!({"ids": [first["id"]]})).unwrap();
        assert_eq!(status(&app), Some(end));
        app.run("selection.set", json!({"ids": [first["id"], second["to"]]})).unwrap();
        assert!(status(&app).unwrap().overflow_chars.is_some());
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
}
