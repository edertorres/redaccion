//! Contextual Task Bar (InDesign 2026 §6): a floating 36 pt HUD just below the selection with the
//! most common actions for it. Window > Contextual Task Bar toggles it (on by default).

use egui::{Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::widgets::{self, FIELD_H, NumField};
use crate::{DesignApp, icons, panels};

pub const HEIGHT: f32 = 36.0;

#[derive(Clone)]
struct EditorialFeedback {
    uid: u64,
    revision: u64,
    story: designcraft_doc::StoryId,
    message: String,
    error: bool,
}

fn editorial_feedback(app: &DesignApp, ui: &Ui, story: designcraft_doc::StoryId, message: String, error: bool) {
    if let Some(st) = app.session.active() {
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new("editorial_feedback"), EditorialFeedback { uid: st.uid, revision: st.revision, story, message, error })
        });
    }
}

fn editorial_number_with_id(ui: &mut Ui, id: &str, key: &str, value: Option<f64>, min: f64, max: f64, suffix: &str) -> Option<f64> {
    NumField::number(id, value, suffix, 2).width(92.0).spinner().step(if key == "size" { 0.01 } else { 0.1 }).range(min, max).show(ui)
}

#[cfg(test)]
fn editorial_number(ui: &mut Ui, key: &str, value: Option<f64>, min: f64, max: f64, suffix: &str) -> Option<f64> {
    editorial_number_with_id(ui, &format!("editor_{key}"), key, value, min, max, suffix)
}

pub(crate) fn show_copyfit_adjustments(
    app: &mut DesignApp,
    ui: &mut Ui,
    sid: designcraft_doc::StoryId,
    attrs: &serde_json::Value,
    id_prefix: &str,
) -> bool {
    let mut shown = false;
    for (key, policy, label, suffix) in [
        ("size", "sizeFlexibility", "Size", " pt"),
        ("tracking", "trackingFlexibility", "Tracking", ""),
        ("kerning", "kerningAllowance", "Kerning", ""),
        ("leading", "leadingFlexibility", "Leading", " pt"),
    ] {
        let range = &attrs["copyfit"][policy];
        let (Some(min), Some(max)) = (range["start"].as_f64(), range["end"].as_f64()) else { continue };
        if !min.is_finite() || !max.is_finite() || min > max {
            continue;
        }
        shown = true;
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
        let cur_val = match key {
            "kerning" => attrs["chars"]["kerning"]["value"].as_f64().or_else(|| attrs["chars"]["kerning"].as_f64()),
            "leading" => attrs["chars"]["leading"]["value"].as_f64().or_else(|| attrs["chars"]["leading"].as_f64()),
            _ => attrs["chars"][key].as_f64(),
        };
        let field_id = format!("{id_prefix}_{key}");
        if let Some(value) = editorial_number_with_id(ui, &field_id, key, cur_val, min, max, suffix)
            && cur_val.is_none_or(|old| (old - value).abs() > 1e-9)
        {
            let mut changes = serde_json::Map::new();
            match key {
                "kerning" => {
                    changes.insert(key.into(), json!({"kind": "manual", "value": value}));
                }
                "leading" => {
                    changes.insert(key.into(), json!({"kind": "points", "value": value}));
                }
                _ => {
                    changes.insert(key.into(), json!(value));
                }
            }
            if app.session.active().and_then(|st| st.selection.text).map(|t| t.story) != Some(sid) {
                let _ = app.run("text.select", json!({"story": sid.0, "anchor": 0, "focus": 0}));
            }
            if let Err(error) = app.run("type.char", json!({"attrs": changes})) {
                editorial_feedback(app, ui, sid, crate::i18n::tr(&app.ui.language, &error).to_string(), true);
            } else {
                ui.ctx().data_mut(|data| data.remove::<EditorialFeedback>(egui::Id::new("editorial_feedback")));
            }
        }
    }
    shown
}

/// Show the bar under `sel` (screen rect of the selection), kept inside `canvas`.
pub fn show(app: &mut DesignApp, ctx: &egui::Context, sel: Rect, canvas: Rect) {
    // (Two separate context calls: `is_pointer_over_egui` inside an `input` closure deadlocks.)
    if !app.ui.task_bar || (ctx.input(|i| i.pointer.any_down()) && !ctx.is_pointer_over_egui()) {
        return;
    }
    let Some(st) = app.session.active() else { return };
    let has_text = st.selection.text.is_some();
    if st.selection.items.is_empty() && !has_text {
        return;
    }
    let t = Tokens::get(ctx);
    let id = egui::Id::new("task_bar");
    // Width from the previous frame (content-sized).
    let previous = ctx.memory(|m| m.area_rect(id));
    let w = previous.map_or(420.0, |r| r.width());
    let height = previous.map_or(HEIGHT, |r| r.height().max(HEIGHT));
    let mut x = sel.center().x - w / 2.0;
    x = x.clamp(canvas.min.x + 8.0, (canvas.max.x - w - 8.0).max(canvas.min.x + 8.0));
    let mut y = sel.max.y + 14.0;
    if y + height > canvas.max.y - 8.0 {
        y = (sel.min.y - height - 14.0).max(canvas.min.y + 8.0);
    }
    if y + height > canvas.max.y - 4.0 {
        y = canvas.max.y - height - 8.0;
    }
    egui::Area::new(id).order(egui::Order::Middle).fixed_pos(pos2(x, y)).show(ctx, |ui| {
        egui::Frame::NONE
            .fill(t.panel)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(6)
            .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: egui::Color32::from_black_alpha(80) })
            .inner_margin(egui::Margin { left: 4, right: 8, top: 0, bottom: 0 })
            .show(ui, |ui| {
                ui.set_height(HEIGHT);
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (g, _) = ui.allocate_exact_size(vec2(12.0, 20.0), Sense::hover());
                    icons::paint(ui.painter(), g, "grip", t.icon);
                    let text_frame = panels::sel_info(app).is_some_and(|i| i.is_text && i.count == 1);
                    if app.session.editor_mode {
                        editorial_controls(app, ui, has_text || text_frame);
                    } else if has_text || text_frame {
                        text_controls(app, ui, has_text);
                    } else {
                        object_controls(app, ui);
                    }
                    if !app.session.editor_mode {
                        more(app, ui);
                    }
                });
                if app.session.editor_mode {
                    show_editorial_feedback(app, ui);
                }
            });
    });
}

fn show_editorial_feedback(app: &DesignApp, ui: &mut Ui) {
    let feedback = ui.ctx().data(|data| data.get_temp::<EditorialFeedback>(egui::Id::new("editorial_feedback")));
    let selected = app.session.active().and_then(|st| {
        st.selection.text.map(|ts| ts.story).or_else(|| st.selection.items.first().and_then(|id| st.doc.item(*id)?.text_frame().map(|tf| tf.story)))
    });
    if let Some(feedback) = feedback
        && app.session.active().is_some_and(|st| st.uid == feedback.uid && st.revision == feedback.revision)
        && selected == Some(feedback.story)
    {
        let t = Tokens::get(ui.ctx());
        ui.set_max_width(ui.min_rect().width().max(200.0));
        ui.add_space(3.0);
        ui.add(egui::Label::new(egui::RichText::new(&feedback.message).color(if feedback.error { t.copyfit_error() } else { t.text })).wrap());
    }
}

pub(crate) fn editorial_panel(app: &mut DesignApp, ui: &mut Ui) {
    let text = app.session.active().is_some_and(|st| {
        st.selection.text.is_some() || st.selection.items.first().is_some_and(|id| st.doc.item(*id).is_some_and(|item| item.is_text_frame()))
    });
    ui.horizontal_wrapped(|ui| editorial_controls(app, ui, text));
    show_editorial_feedback(app, ui);
}

fn editorial_controls(app: &mut DesignApp, ui: &mut Ui, text: bool) {
    let t = Tokens::get(ui.ctx());
    if text {
        let sid = app.session.active().and_then(|st| {
            st.selection
                .text
                .map(|ts| ts.story)
                .or_else(|| st.selection.items.first().and_then(|id| st.doc.item(*id)?.text_frame().map(|tf| tf.story)))
        });
        let Some(sid) = sid else { return };
        if !app.session.editorial_story_editable(sid) {
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Protected content")).color(t.text_dim));
            return;
        }
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Edit in Story Editor"))).clicked() {
            let _ = app.run("app.storyEditor", json!({"story": sid.0}));
        }
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Microfit Text"))).clicked() {
            let (message, error) = match app.run("text.microfit", json!({"story": sid.0, "max_excess_words": 50})) {
                Ok(result) => {
                    let key =
                        if result["status"] == "adjusted" { "Microfit applied; undo to restore" } else { "Text already meets the closing tolerance" };
                    (crate::i18n::tr(&app.ui.language, key).to_string(), false)
                }
                Err(error) => (crate::i18n::tr(&app.ui.language, &error).to_string(), true),
            };
            app.status(message.clone());
            editorial_feedback(app, ui, sid, message, error);
        }
        let Some(attrs) = panels::text_attrs(app) else { return };
        show_copyfit_adjustments(app, ui, sid, &attrs, "editor");
    } else {
        let Some(id) = app.session.active().and_then(|st| st.selection.items.first().copied()) else { return };
        if !app.session.editorial_content_editable(id) {
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Protected content")).color(t.text_dim));
            return;
        }
        let graphic = app
            .session
            .active()
            .and_then(|st| st.doc.item(id))
            .is_some_and(|it| matches!(it.content, designcraft_doc::Content::Graphic(_) | designcraft_doc::Content::Unassigned));
        if !graphic {
            return;
        }
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Place…"))).clicked() {
            let _ = app.run("file.place", json!({"frame": id.0}));
        }
        for (label, mode) in [("Fill Frame Proportionally", "fillProportionally"), ("Fit Content Proportionally", "fitProportionally")] {
            if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                let _ = app.run("object.fit", json!({"mode": mode, "ids": [id.0]}));
            }
        }
        let is_placed_photo =
            app.session.active().and_then(|st| st.doc.item(id)).is_some_and(|it| matches!(it.content, designcraft_doc::Content::Graphic(_)));
        if is_placed_photo {
            let resp = ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Retoque Rotativa (AIE)")));
            if resp.on_hover_text(crate::i18n::tr(&app.ui.language, "Optimizar foto para rotativa de periódico (ISO 12647-3 / TAC ≤ 220%)")).clicked()
            {
                let _ = app.run("image.retouchRotary", json!({"ids": [id.0], "preset": "coldset", "format": "cmyk"}));
            }
        }
    }
}

fn split_button(ui: &mut Ui, icon: &str, label: &str) -> (egui::Response, egui::Response) {
    let t = Tokens::get(ui.ctx());
    let g = crate::rtl::plain(ui.ctx(), label, egui::FontId::proportional(11.0), t.text);
    let w = 26.0 + g.size().x + 8.0 + widgets::SEG_W;
    let (r, _) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::hover());
    let main = Rect::from_min_max(r.min, pos2(r.max.x - widgets::SEG_W, r.max.y));
    let seg = Rect::from_min_max(pos2(main.max.x, r.min.y), r.max);
    let mr = ui.interact(main, ui.id().with(("split", label)), Sense::click());
    let sr = ui.interact(seg, ui.id().with(("splitseg", label)), Sense::click());
    for (rr, resp) in [(main, &mr), (seg, &sr)] {
        if resp.hovered() {
            ui.painter().rect_filled(rr, 0.0, t.hover);
        }
    }
    icons::paint(ui.painter(), Rect::from_min_size(main.min + vec2(5.0, 2.5), vec2(16.0, 16.0)), icon, t.icon);
    ui.painter().galley(pos2(main.min.x + 26.0, main.center().y - g.size().y / 2.0), g, t.text);
    widgets::chevron_down(ui.painter(), seg.center(), t.icon);
    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    ui.painter().line_segment([seg.left_top(), seg.left_bottom()], Stroke::new(1.0, t.field_border));
    (mr, sr)
}

fn text_controls(app: &mut DesignApp, ui: &mut Ui, in_text: bool) {
    let t = Tokens::get(ui.ctx());
    if !in_text {
        let (main, seg) = split_button(ui, "fit-text", crate::i18n::tr(&app.ui.language, "Fit Text"));
        if main
            .on_hover_ui(|ui| {
                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Fit frame height to text"));
            })
            .clicked()
        {
            let _ = app.run("object.textFrameOptions", json!({"autoSize": "heightOnly"}));
        }
        egui::Popup::menu(&seg).show(|ui| {
            for (label, v) in [("Off", "off"), ("Height Only", "heightOnly"), ("Width Only", "widthOnly"), ("Height and Width", "heightAndWidth")] {
                if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                    let _ = app.run("object.textFrameOptions", json!({"autoSize": v}));
                    ui.close();
                }
            }
        });
        sep(ui);
    }
    let Some(a) = panels::text_attrs(app) else { return };
    let c = a["chars"].clone();
    let fam = c["fontFamily"].as_str().unwrap_or("").to_string();
    let sty = c["fontStyle"].as_str().unwrap_or("").to_string();
    let fonts = panels::fonts(app);
    let menu = panels::font_menu(app);
    let english = app.session.prefs.show_font_names_in_english;
    let labels: Vec<String> = menu.iter().map(|f| f.label(english).to_string()).collect();
    let cur = menu.iter().position(|f| f.family == fam);
    let shown = if fam.is_empty() { "—".to_string() } else { panels::font_label(app, &menu, &fam) };
    if let Some(f) = widgets::dropdown_list(ui, &shown, 150.0, &labels, cur).and_then(|k| menu.get(k)) {
        let f = f.family.clone();
        let styles = fonts.styles(&f);
        let style = if styles.iter().any(|s| s == "Regular") { "Regular".to_string() } else { styles.first().cloned().unwrap_or_default() };
        let _ = app.run("type.char", json!({"attrs": {"fontFamily": f, "fontStyle": style}}));
    }
    let styles = fonts.styles(&fam);
    let cur = styles.iter().position(|s| *s == sty);
    if let Some(k) = widgets::dropdown_list(ui, if sty.is_empty() { "—" } else { &sty }, 90.0, &styles, cur) {
        let _ = app.run("type.char", json!({"attrs": {"fontStyle": styles[k]}}));
    }
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    icons::paint(ui.painter(), r, "font-size", t.icon);
    const SIZES: &[f64] = &[6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0, 24.0, 30.0, 36.0, 48.0, 60.0, 72.0];
    if let Some(v) =
        NumField::number("tb_size", c["size"].as_f64(), " pt", 2).width(88.0).spinner().step(0.01).presets(SIZES).range(0.1, 1296.0).show(ui)
    {
        let _ = app.run("type.char", json!({"attrs": {"size": v}}));
    }
    sep(ui);
    // Text fill.
    if let Some(doc) = app.session.active().map(|d| d.doc.clone()) {
        let sw = c["fill"].as_str().unwrap_or("[Black]").to_string();
        let (col, g) = widgets::swatch_colors(&doc, &sw, 1.0);
        let resp = widgets::swatch_chip(ui, 17.0, col, g).on_hover_text(format!("Fill: {sw}"));
        let mut picked = None;
        egui::Popup::menu(&resp).show(|ui| {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for s in &doc.swatches {
                    if ui.selectable_label(s.name == sw, &s.name).clicked() {
                        picked = Some(s.name.clone());
                        ui.close();
                    }
                }
            });
        });
        if let Some(p) = picked {
            let _ = app.run("type.char", json!({"attrs": {"fill": p}}));
        }
    }
    // Paragraph alignment.
    let al = a["para"]["align"].as_str().unwrap_or("left").to_string();
    let icon = match al.as_str() {
        "center" => "palign-center",
        "right" => "palign-right",
        "leftJustified" | "centerJustified" | "rightJustified" | "fullyJustified" => "palign-justify-left",
        _ => "palign-left",
    };
    let resp = widgets::icon_toggle(ui, icon, false, crate::i18n::tr(&app.ui.language, "Paragraph alignment"));
    egui::Popup::menu(&resp).show(|ui| {
        for (label, v) in [
            ("Align Left", "left"),
            ("Align Center", "center"),
            ("Align Right", "right"),
            ("Justify", "leftJustified"),
            ("Justify All", "fullyJustified"),
        ] {
            if ui.selectable_label(al == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                let _ = app.run("type.para", json!({"attrs": {"align": v}}));
                ui.close();
            }
        }
    });
    if widgets::icon_toggle(ui, "frame-options", false, crate::i18n::tr(&app.ui.language, "Text Frame Options")).clicked() {
        app.ui.dialog = Some(crate::dialogs::Dialog::new("textFrameOptions", json!({})));
    }
}

fn object_controls(app: &mut DesignApp, ui: &mut Ui) {
    let Some(i) = panels::sel_info(app) else { return };
    let Some(doc) = app.session.active().map(|d| d.doc.clone()) else { return };
    if i.is_graphic {
        for (icon, mode, tip) in [
            ("fit-fill", "fillProportionally", "Fill Frame Proportionally"),
            ("fit-prop", "fitProportionally", "Fit Content Proportionally"),
            ("fit-frame", "fitFrameToContent", "Fit Frame to Content"),
            ("fit-center", "centerContent", "Center Content"),
        ] {
            if widgets::icon_toggle(ui, icon, false, crate::i18n::tr(&app.ui.language, tip)).clicked() {
                let _ = app.run("object.fit", json!({"mode": mode}));
            }
        }
        sep(ui);
    }
    for (k, (cmd, sw)) in [("object.fill", i.fill.clone()), ("object.stroke", i.stroke.clone())].into_iter().enumerate() {
        let (col, g) = widgets::swatch_colors(&doc, &sw, 1.0);
        let (r, resp) = ui.allocate_exact_size(vec2(17.0, 17.0), Sense::click());
        if k == 0 {
            widgets::paint_chip(ui.painter(), r, col, g);
        } else {
            widgets::paint_stroke_chip(ui.painter(), r, col, egui::Color32::from_gray(20));
        }
        let resp = resp.on_hover_text(format!("{}: {sw}", if k == 0 { "Fill" } else { "Stroke" }));
        let mut picked = None;
        egui::Popup::menu(&resp).show(|ui| {
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for s in &doc.swatches {
                    if ui.selectable_label(s.name == sw, &s.name).clicked() {
                        picked = Some(s.name.clone());
                        ui.close();
                    }
                }
            });
        });
        if let Some(p) = picked {
            let _ = app.run(cmd, json!({"swatch": p}));
        }
    }
    const WEIGHTS: &[f64] = &[0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0];
    if let Some(v) = NumField::number("tb_sw", Some(i.stroke_weight), " pt", 3).width(64.0).spinner().presets(WEIGHTS).range(0.0, 800.0).show(ui) {
        let _ = app.run("object.stroke", json!({"weight": v}));
    }
    sep(ui);
    for (cmd, icon, tip) in [("object.bringToFront", "objalign-top", "Bring to Front"), ("object.sendToBack", "objalign-bottom", "Send to Back")] {
        if widgets::icon_toggle(ui, icon, false, crate::i18n::tr(&app.ui.language, tip)).clicked() {
            let _ = app.run(cmd, json!({}));
        }
    }
    if i.count > 1 && widgets::outline_button(ui, crate::i18n::tr(&app.ui.language, "Group"), 52.0).clicked() {
        let _ = app.run("object.group", json!({}));
    }
}

fn more(app: &mut DesignApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, FIELD_H), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, t.hover);
    }
    for k in 0..3 {
        ui.painter().circle_filled(pos2(r.min.x + 4.0 + k as f32 * 7.0, r.center().y), 1.8, t.icon);
    }
    let resp = resp.on_hover_ui(|ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "More Options"));
    });
    egui::Popup::menu(&resp).show(|ui| {
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Hide Bar"))).clicked() {
            app.ui.task_bar = false;
            ui.close();
        }
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Properties Panel"))).clicked() {
            let _ = app.run("window.panel", json!({"panel": "properties"}));
            ui.close();
        }
    });
}

fn sep(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 24.0), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, t.border));
}

#[cfg(test)]
mod editorial_tests {
    use super::*;

    #[test]
    fn microstep_spinner_changes_actual_story_and_clamps_at_template_limits() {
        for (key, initial, min, max, step) in [("size", 12.0, 11.98, 12.02, 0.01), ("tracking", 0.0, -0.2, 0.2, 0.1)] {
            let mut app = DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({})).unwrap();
            let frame = app.run("frame.create", json!({"rect":[36,36,240,200],"content":"text","text":"Texto de prueba"})).unwrap();
            app.run("style.paragraph.create", json!({"name":"Body","chars":{"size":12}})).unwrap();
            app.run("style.paragraph.edit", json!({"name":"Body","copyfit":{"size_min":11.98,"size_max":12.02,"trk_min":-0.2,"trk_max":0.2}}))
                .unwrap();
            app.run("story.update", json!({"story":frame["story"],"paragraphStyle":"Body"})).unwrap();
            app.run("selection.set", json!({"ids":[frame["id"]]})).unwrap();
            app.session.editor_mode = true;
            let original = app.session.active().unwrap().doc.clone();
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx, "");
            let mut origin = egui::Pos2::ZERO;
            let mut commits = Vec::new();
            let mut run = |events: Vec<egui::Event>| {
                let input =
                    egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))), events, ..Default::default() };
                let mut output = ctx.run_ui(input, |ui| {
                    origin = ui.cursor().min;
                    let attrs = panels::text_attrs(&mut app).unwrap();
                    if let Some(value) = editorial_number(ui, key, attrs["chars"][key].as_f64(), min, max, "")
                        && attrs["chars"][key].as_f64().is_none_or(|old| (old - value).abs() > 1e-9)
                    {
                        let mut changes = serde_json::Map::new();
                        changes.insert(key.into(), json!(value));
                        app.run("type.char", json!({"attrs":changes})).unwrap();
                        commits.push(value);
                    }
                });
                output.textures_delta.clear();
                origin
            };
            let position = run(vec![]) + vec2(9.0, 5.0);
            for _ in 0..3 {
                run(vec![egui::Event::PointerMoved(position)]);
                run(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }]);
                run(vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            assert!((commits.first().copied().unwrap() - initial - step).abs() < 1e-9);
            let attrs = panels::text_attrs(&mut app).unwrap();
            assert!((attrs["chars"][key].as_f64().unwrap() - max).abs() < 1e-9);
            assert_eq!(app.session.active().unwrap().doc.spreads, original.spreads);
            let sid = designcraft_doc::StoryId(frame["story"].as_u64().unwrap());
            let story = app.session.active().unwrap().doc.story(sid).unwrap();
            for (_, format) in story.runs() {
                let value = if key == "size" { format.over.size } else { format.over.tracking };
                assert!((value.unwrap() - max).abs() < 1e-9);
            }
            app.run("edit.undo", json!({})).unwrap();
            let attrs = panels::text_attrs(&mut app).unwrap();
            assert!((attrs["chars"][key].as_f64().unwrap() - initial - step).abs() < 1e-9);
        }
    }
}
