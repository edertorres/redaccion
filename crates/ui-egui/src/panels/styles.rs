//! Paragraph Styles and Character Styles panels.

use egui::{Sense, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::{DesignApp, icons};

fn list(app: &mut DesignApp, ui: &mut egui::Ui, para: bool) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    let names: Vec<String> = if para {
        st.doc.styles.paragraph.iter().map(|s| s.name.clone()).collect()
    } else {
        st.doc.styles.character.iter().map(|s| s.name.clone()).collect()
    };
    let attrs = super::text_attrs(app);
    let current = attrs.as_ref().and_then(|a| a[if para { "paragraphStyle" } else { "characterStyle" }].as_str().map(str::to_string));
    let overrides = attrs.as_ref().map(|a| a[if para { "paraOverrides" } else { "charOverrides" }].as_u64().unwrap_or(0)).unwrap_or(0);
    crate::rtl::label(
        ui,
        egui::RichText::new(match &current {
            Some(c) => format!("{}{}", crate::i18n::style_name(&app.ui.language, c), if overrides > 0 { "+" } else { "" }),
            None => crate::i18n::tr(&app.ui.language, "No text selected").into(),
        })
        .size(11.0)
        .color(t.text_dim),
    );
    // Groups (`Group/Name`) as folders, ungrouped styles first.
    let groups: Vec<String> = {
        let mut g: Vec<String> = names.iter().filter_map(|n| n.rsplit_once('/').map(|(g, _)| g.to_string())).collect();
        g.sort();
        g.dedup();
        g
    };
    for n in names.iter().filter(|n| !n.contains('/')) {
        if para && n == designcraft_doc::NO_PARA_STYLE {
            continue;
        }
        style_row(app, ui, para, n, current.as_deref(), &groups, &t);
    }
    for g in &groups {
        egui::CollapsingHeader::new(g.as_str()).id_salt(("style_group", para, g)).default_open(true).show(ui, |ui| {
            for n in names.iter().filter(|n| n.rsplit_once('/').is_some_and(|(x, _)| x == g)) {
                style_row(app, ui, para, n, current.as_deref(), &groups, &t);
            }
        });
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(crate::rtl::widget(
            ui,
            egui::RichText::new(crate::i18n::tr(&app.ui.language, "⌥-click clears overrides")).size(10.5).color(t.text_disabled),
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let plus_resp = icons::button(ui, "plus", 20.0, false, crate::i18n::tr(&app.ui.language, "Create New Style from Selection"));
            if plus_resp.clicked() {
                let cmd = if para { "style.paragraph.create" } else { "style.character.create" };
                let base = if para { "Paragraph Style 1" } else { "Character Style 1" };
                let alt = ui.input(|i| i.modifiers.alt);
                if let Ok(res) = app.run(cmd, json!({"name": base, "fromSelection": true}))
                    && alt
                {
                    let created = res.get("name").and_then(serde_json::Value::as_str).unwrap_or(base);
                    let dlg = if para { "paragraphStyleOptions" } else { "characterStyleOptions" };
                    app.ui.dialog = Some(crate::dialogs::Dialog::new(dlg, json!({"name": created})));
                }
            }
            ui.menu_button("☰", |ui| {
                let kind = if para { "paragraph" } else { "character" };
                let load_label = if para { "Load Paragraph Styles…" } else { "Load Character Styles…" };
                if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, load_label))).clicked() {
                    let _ = app.run("app.loadStyles", json!({"kinds": [kind]}));
                    ui.close();
                }
                if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Load All Styles…"))).clicked() {
                    let _ = app.run("app.loadStyles", json!({}));
                    ui.close();
                }
                ui.separator();
                let unused_label = if para { "Delete Unused Paragraph Styles" } else { "Delete Unused Character Styles" };
                if ui
                    .add_enabled(!app.session.editor_mode, egui::Button::new(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, unused_label))))
                    .clicked()
                {
                    run_delete_unused_styles(app, kind);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !app.session.editor_mode,
                        egui::Button::new(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Delete All Unused Styles"))),
                    )
                    .clicked()
                {
                    run_delete_unused_styles(app, "all");
                    ui.close();
                }
            });
        });
    });
}

pub fn paragraph(app: &mut DesignApp, ui: &mut egui::Ui) {
    list(app, ui, true);
}

pub fn character(app: &mut DesignApp, ui: &mut egui::Ui) {
    list(app, ui, false);
}

/// One style in the list: click applies (⌥ clears overrides), double-click edits (paragraph),
/// right-click: apply, break link, move to group.
fn style_row(app: &mut DesignApp, ui: &mut egui::Ui, para: bool, n: &str, current: Option<&str>, groups: &[String], t: &Tokens) {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if current == Some(n) {
        ui.painter().rect_filled(row, 0.0, t.row_selected);
    } else if resp.hovered() {
        ui.painter().rect_filled(row, 0.0, t.hover);
    }
    let shown = n.rsplit('/').next().unwrap_or(n);
    crate::rtl::paint(
        ui.painter(),
        row.min + vec2(8.0, 11.0),
        egui::Align2::LEFT_CENTER,
        if n.contains('/') { shown } else { crate::i18n::style_name(&app.ui.language, shown) },
        egui::FontId::proportional(12.5),
        t.text,
    );
    if resp.double_clicked() {
        let dlg = if para { "paragraphStyleOptions" } else { "characterStyleOptions" };
        app.ui.dialog = Some(crate::dialogs::Dialog::new(dlg, json!({"name": n})));
    } else if resp.clicked() {
        let cmd = if para { "style.paragraph.apply" } else { "style.character.apply" };
        let clear = ui.input(|i| i.modifiers.alt);
        let _ = app.run(cmd, json!({"name": n, "clearOverrides": clear}));
    }
    let kind = if para { "paragraph" } else { "character" };
    resp.context_menu(|ui| {
        if !n.starts_with('[')
            && ui
                .button(crate::rtl::widget(
                    ui,
                    format!(
                        "{} \"{}\"…",
                        crate::i18n::tr(&app.ui.language, "Edit"),
                        if n.contains('/') { shown } else { crate::i18n::style_name(&app.ui.language, shown) }
                    ),
                ))
                .clicked()
        {
            let dlg = if para { "paragraphStyleOptions" } else { "characterStyleOptions" };
            app.ui.dialog = Some(crate::dialogs::Dialog::new(dlg, json!({"name": n})));
            ui.close();
        }
        let label = if n.contains('/') { shown } else { crate::i18n::style_name(&app.ui.language, shown) };
        if ui.button(crate::rtl::widget(ui, format!("{} \"{}\"", crate::i18n::tr(&app.ui.language, "Apply"), label))).clicked() {
            let cmd = if para { "style.paragraph.apply" } else { "style.character.apply" };
            let _ = app.run(cmd, json!({"name": n, "applyNext": false}));
            ui.close();
        }
        let has_next = para
            && app.session.doc().is_ok_and(|s| {
                s.doc.styles.para(n).and_then(|st| st.next_style.as_ref()).is_some_and(|ns| ns != "[Same Style]" && !ns.is_empty() && ns != n)
            });
        if has_next
            && ui
                .button(crate::rtl::widget(
                    ui,
                    format!("{} \"{}\" {}", crate::i18n::tr(&app.ui.language, "Apply"), label, crate::i18n::tr(&app.ui.language, "then Next Style")),
                ))
                .clicked()
        {
            let _ = app.run("style.paragraph.apply", json!({"name": n, "applyNext": true}));
            ui.close();
        }
        if ui
            .button(crate::rtl::widget(
                ui,
                format!("{} \"{}\", {}", crate::i18n::tr(&app.ui.language, "Apply"), label, crate::i18n::tr(&app.ui.language, "Clear Overrides")),
            ))
            .clicked()
        {
            let cmd = if para { "style.paragraph.apply" } else { "style.character.apply" };
            let _ = app.run(cmd, json!({"name": n, "clearOverrides": true, "applyNext": false}));
            ui.close();
        }
        if has_next
            && ui
                .button(crate::rtl::widget(
                    ui,
                    format!(
                        "{} \"{}\" {}, {}",
                        crate::i18n::tr(&app.ui.language, "Apply"),
                        label,
                        crate::i18n::tr(&app.ui.language, "then Next Style"),
                        crate::i18n::tr(&app.ui.language, "Clear Overrides")
                    ),
                ))
                .clicked()
        {
            let _ = app.run("style.paragraph.apply", json!({"name": n, "applyNext": true, "clearOverrides": true}));
            ui.close();
        }
        if current == Some(n) && ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Break Link to Style"))).clicked() {
            let _ = app.run("style.breakLink", json!({"kind": kind}));
            ui.close();
        }
        if !n.starts_with('[') {
            ui.menu_button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Move to Group")), |ui| {
                if n.contains('/') && ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "[No Group]"))).clicked() {
                    let _ = app.run("style.group", json!({"kind": kind, "names": [n], "group": ""}));
                    ui.close();
                }
                for g in groups {
                    if n.rsplit_once('/').map(|(x, _)| x) != Some(g.as_str()) && ui.button(g).clicked() {
                        let _ = app.run("style.group", json!({"kind": kind, "names": [n], "group": g}));
                        ui.close();
                    }
                }
                ui.separator();
                let id = egui::Id::new(("new_style_group", para));
                let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "New group…")))
                        .desired_width(140.0),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty() {
                    let _ = app.run("style.group", json!({"kind": kind, "names": [n], "group": name.trim()}));
                    name.clear();
                    ui.close();
                }
                ui.data_mut(|d| d.insert_temp(id, name));
            });
            ui.separator();
            let del_cmd = if para { "style.paragraph.delete" } else { "style.character.delete" };
            if ui
                .add_enabled(
                    !app.session.editor_mode,
                    egui::Button::new(crate::rtl::widget(ui, format!("{} \"{}\"", crate::i18n::tr(&app.ui.language, "Delete"), label))),
                )
                .clicked()
            {
                let _ = app.run(del_cmd, json!({"name": n}));
                ui.close();
            }
        }
    });
}

fn run_delete_unused_styles(app: &mut DesignApp, kind: &str) {
    if let Ok(res) = app.run("style.deleteUnused", json!({"kind": kind})) {
        let count = res.get("deleted").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if count == 0 {
            app.status("No unused styles to delete.");
        } else {
            app.status(format!("Deleted {count} unused style(s)."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_panel_menu_deletes_unused_styles() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({})).unwrap();
        app.run("style.paragraph.create", json!({"name": "UsedPara"})).unwrap();
        app.run("style.paragraph.create", json!({"name": "UnusedPara"})).unwrap();
        app.run("style.character.create", json!({"name": "UnusedChar"})).unwrap();

        app.run("frame.create", json!({"rect": [10, 10, 200, 100], "content": "text", "text": "Hello"})).unwrap();
        app.run("style.paragraph.apply", json!({"name": "UsedPara"})).unwrap();

        let mut initialized = false;
        let mut h = Harness::builder().with_size(egui::vec2(320.0, 400.0)).build_ui_state(
            move |ui, app| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx(), "");
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                paragraph(app, ui);
            },
            app,
        );
        h.run();
        h.get_by_label("☰").click();
        h.run();
        h.get_by_label("Delete Unused Paragraph Styles").click();
        h.run();

        let doc = &h.state().session.doc().unwrap().doc;
        assert!(doc.styles.para("UsedPara").is_some());
        assert!(doc.styles.para("UnusedPara").is_none());
        assert!(doc.styles.char_style("UnusedChar").is_some());

        // Now delete all remaining unused styles (UnusedChar) from the menu
        h.get_by_label("☰").click();
        h.run();
        h.get_by_label("Delete All Unused Styles").click();
        h.run();

        let doc = &h.state().session.doc().unwrap().doc;
        assert!(doc.styles.char_style("UnusedChar").is_none());
    }
}
