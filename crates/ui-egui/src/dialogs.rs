//! Modal dialogs. Each dialog keeps its fields in a JSON map so the control channel can set them
//! (`ui.dialog.set {field, value}`) and confirm them (`ui.dialog.confirm`) like a user would.

use designcraft_geom::{Unit, format_measure, units::parse_measure};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::DesignApp;
use crate::theme::semibold;

#[derive(Clone, Debug, Serialize)]
pub struct Dialog {
    pub id: String,
    pub fields: Map<String, Value>,
}

impl Dialog {
    pub fn new(id: &str, params: Value) -> Self {
        let mut fields = match params {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let defaults = match id {
            "newDocument" => {
                json!({"preset": "Letter", "width": "51p0", "height": "66p0", "pages": 1, "facingPages": true, "columns": 1, "gutter": "1p0",
                "marginTop": "3p0", "marginBottom": "3p0", "marginInside": "3p0", "marginOutside": "3p0", "bleed": "0p0", "primaryTextFrame": false})
            }
            "goToPage" => json!({"page": "1"}),
            "qrCode" => json!({"type": "url", "content": "https://", "color": "[Black]"}),
            "insertTable" => json!({"bodyRows": 4, "columns": 4, "headerRows": 0, "footerRows": 0}),
            "insertXref" => json!({"linkTo": "paragraph", "style": "", "target": "", "format": ""}),
            "findChange" => json!({"find": "", "change": "", "grep": false, "caseSensitive": false, "wholeWord": false, "scope": "document"}),
            "textFrameOptions" => json!({"columns": 1, "gutter": "1p0", "inset": "0p0", "verticalJustification": "top"}),
            "documentSetup" => json!({}),
            "exportPdf" => {
                json!({"filename": "document.pdf", "preset": "highQuality", "range": "all", "pages": "1", "spreads": false, "bleed": true, "marks": false, "tagged": true, "compressImages": true, "standard": "none"})
            }
            "exportPng" => json!({"filename": "document-p1.png", "page": 1, "pageCount": 1, "scale": 2.0, "richBlack": false}),
            "saveAs" => json!({"name": "Untitled", "format": "designcraft", "target": "server"}),
            "saveCopy" => json!({"name": "Untitled copy", "format": "designcraft", "target": "server"}),
            "oversetWarning" => json!({"stories": []}),
            "editorialOverview" => json!({"filter": "all"}),
            _ => json!({}),
        };
        if let Value::Object(d) = defaults {
            for (k, v) in d {
                fields.entry(k).or_insert(v);
            }
        }
        if id == "footnoteOptions" {
            // Flatten the rule and show special characters as InDesign metacharacters.
            if let Some(Value::Object(r)) = fields.remove("rule") {
                for (k, v) in r {
                    fields.insert(format!("rule.{k}"), v);
                }
            }
            let sep = fields.get("separator").and_then(Value::as_str).unwrap_or("").to_string();
            fields.insert("separator".into(), json!(sep.replace('\t', "^t").replace('\u{2003}', "^m").replace('\u{2002}', "^>")));
            fields.entry("tab".to_string()).or_insert(json!("numbering"));
        }
        if id == "paragraphStyleOptions" || id == "characterStyleOptions" {
            fields.entry("section".to_string()).or_insert(json!("general"));
        }
        if id == "frameSize" {
            for k in ["width", "height"] {
                if let Some(v) = fields.get(k).and_then(Value::as_f64) {
                    fields.insert(k.into(), json!(format_measure(v, Unit::Picas)));
                }
            }
        }
        Dialog { id: id.into(), fields }
    }
    fn s(&self, k: &str) -> String {
        match self.fields.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
    fn m(&self, k: &str) -> Option<f64> {
        match self.fields.get(k) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => parse_measure(s, Unit::Picas).ok(),
            _ => None,
        }
    }
    /// A measure in points ("6", "6 pt", "0p6", "2 mm").
    fn pt(&self, k: &str) -> Option<f64> {
        match self.fields.get(k) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => parse_measure(s, Unit::Points).ok(),
            _ => None,
        }
    }
    fn b(&self, k: &str) -> bool {
        self.fields.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    fn n(&self, k: &str) -> Option<f64> {
        match self.fields.get(k) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        }
    }
}

fn text_field(ui: &mut egui::Ui, d: &mut Dialog, key: &str, w: f32) {
    let mut s = d.s(key);
    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(w).horizontal_align(egui::Align::Min)).changed() {
        d.fields.insert(key.into(), Value::String(s));
    }
}

/// Edit › Keyboard Shortcuts: every command and its shortcut; click a shortcut, then press the
/// new keys (Esc cancels, Backspace clears).
/// Edit › Spelling › User Dictionary: added words and hyphenation exceptions (applied at once).
fn user_dictionary(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Hyphenation Exceptions")).font(semibold(12.0)));
    ui.label(crate::rtl::widget(
        ui,
        egui::RichText::new(crate::i18n::tr(
            &app.ui.language,
            "Type a word with ~ at each allowed break (ex~am~ple); without ~ the word is never hyphenated.",
        ))
        .size(10.5),
    ));
    ui.horizontal(|ui| {
        text_field(ui, d, "word", 200.0);
        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Add"))).clicked() {
            let w = d.s("word");
            if !w.trim().is_empty() {
                match app.run("hyphenation.addException", json!({"word": w.trim()})) {
                    Ok(_) => {
                        d.fields.insert("word".into(), json!(""));
                    }
                    Err(e) => app.status(format!("User Dictionary: {e}")),
                }
            }
        }
    });
    let list = app.session.execute("hyphenation.list", &json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        for w in list {
            let w = w.as_str().unwrap_or("").to_string();
            ui.horizontal(|ui| {
                ui.label(&w);
                if ui.small_button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Remove"))).clicked() {
                    let _ = app.run("hyphenation.removeException", json!({"word": w}));
                }
            });
        }
    });
}

fn keyboard_shortcuts(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let mut q = d.s("query");
    ui.add(
        egui::TextEdit::singleline(&mut q)
            .hint_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Search commands")))
            .desired_width(f32::INFINITY),
    );
    d.fields.insert("query".into(), json!(q));
    let recording = d.s("recording");
    // Capture the next key press for the command being recorded.
    if !recording.is_empty() {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, m)) = pressed {
            let sc = match key {
                egui::Key::Escape => None,
                egui::Key::Backspace if !m.any() => Some(String::new()),
                _ => Some(crate::menus::shortcut_string(m, key)),
            };
            if let Some(sc) = sc {
                let msg = match app.run("window.setShortcut", json!({"id": recording, "shortcut": sc})) {
                    Ok(v) => match v["conflicts"].as_array().filter(|c| !c.is_empty()) {
                        Some(c) => format!("Also used by: {}", c.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")),
                        None => String::new(),
                    },
                    Err(e) => e,
                };
                d.fields.insert("message".into(), json!(msg));
            }
            d.fields.insert("recording".into(), json!(""));
        }
    }
    let ql = q.to_lowercase();
    let mut rows: Vec<(String, String, String)> = designcraft_engine::command_specs()
        .iter()
        .filter(|c| !c.menu.is_empty() || c.shortcut.is_some())
        .map(|c| (c.id.to_string(), c.label.to_string(), c.menu.join(" › ")))
        .chain(crate::menus::UI_COMMANDS.iter().map(|c| (c.0.to_string(), c.1.to_string(), String::new())))
        .filter(|(id, l, _)| ql.is_empty() || l.to_lowercase().contains(&ql) || id.to_lowercase().contains(&ql))
        .collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        egui::Grid::new("shortcuts").num_columns(3).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            for (id, label, menu) in &rows {
                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label.trim_end_matches('…')))
                    .on_hover_text(format!("{id}{}", if menu.is_empty() { String::new() } else { format!("  ({menu})") }));
                let cur = crate::menus::shortcut_of(app, id).map(|s| crate::menus::shortcut_text(&s)).unwrap_or_else(|| "—".into());
                let text = if recording == *id { "Press keys…".to_string() } else { cur };
                if ui.add(egui::Button::new(text).min_size(egui::vec2(110.0, 0.0))).clicked() {
                    d.fields.insert("recording".into(), json!(id));
                    d.fields.insert("message".into(), json!(""));
                }
                if app.ui.shortcuts.contains_key(id) {
                    if ui.small_button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Default"))).clicked() {
                        let _ = app.run("window.setShortcut", json!({"id": id, "shortcut": null}));
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    });
    let msg = d.s("message");
    if !msg.is_empty() {
        ui.label(egui::RichText::new(msg).color(egui::Color32::from_rgb(240, 180, 60)));
    }
    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Reset All to Defaults"))).clicked() {
        let _ = app.run("window.resetShortcuts", json!({}));
    }
}

/// File › Print, with the printers the system knows.
pub fn open_print(app: &mut DesignApp) {
    let p = app.session.execute("file.printers", &json!({})).unwrap_or_default();
    let printer = p["default"].as_str().map(str::to_string).or_else(|| p["printers"][0].as_str().map(str::to_string)).unwrap_or_default();
    app.ui.dialog = Some(Dialog::new(
        "print",
        json!({"printers": p["printers"], "printer": printer, "copies": 1, "range": "all", "pages": "1", "spreads": false, "marks": false, "bleed": false}),
    ));
}

fn print_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let printers: Vec<String> = d
        .fields
        .get("printers")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    egui::Grid::new("print").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Printer:"));
        let cur = d.s("printer");
        if printers.is_empty() {
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "System default")).italics());
        } else {
            egui::ComboBox::from_id_salt("printer").selected_text(&cur).width(220.0).show_ui(ui, |ui| {
                for p in &printers {
                    if ui.selectable_label(*p == cur, p).clicked() {
                        d.fields.insert("printer".into(), json!(p));
                    }
                }
            });
        }
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Copies:"));
        text_field(ui, d, "copies", 50.0);
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Pages:"));
        ui.vertical(|ui| {
            let range = d.s("range");
            if ui.radio(range == "all", "All").clicked() {
                d.fields.insert("range".into(), json!("all"));
            }
            ui.horizontal(|ui| {
                if ui.radio(range == "range", "Range:").clicked() {
                    d.fields.insert("range".into(), json!("range"));
                }
                text_field(ui, d, "pages", 120.0);
            });
        });
        ui.end_row();
        ui.label("");
        ui.vertical(|ui| {
            check(ui, d, "spreads", crate::i18n::tr(&app.ui.language, "Spreads"));
            check(ui, d, "marks", crate::i18n::tr(&app.ui.language, "Printer's Marks"));
            check(ui, d, "bleed", crate::i18n::tr(&app.ui.language, "Include Bleed"));
        });
        ui.end_row();
    });
}

fn export_pdf_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    egui::Grid::new("export_pdf_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Adobe PDF Preset:"));
        let cur_preset = d.s("preset");
        let preset_label = match cur_preset.as_str() {
            "pdfx1a" => "PDF/X-1a:2001",
            "pdfx3" => "PDF/X-3:2002",
            "pdfx4" => "PDF/X-4:2010",
            "pressQuality" => crate::i18n::tr(&app.ui.language, "Press Quality"),
            "smallestFile" => crate::i18n::tr(&app.ui.language, "Smallest File Size"),
            _ => crate::i18n::tr(&app.ui.language, "High Quality Print"),
        };
        egui::ComboBox::from_id_salt("pdf_preset").selected_text(preset_label).width(240.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_preset == "highQuality", crate::i18n::tr(&app.ui.language, "High Quality Print")).clicked() {
                d.fields.insert("preset".into(), json!("highQuality"));
                d.fields.insert("standard".into(), json!("none"));
            }
            if ui.selectable_label(cur_preset == "pdfx1a", "PDF/X-1a:2001").clicked() {
                d.fields.insert("preset".into(), json!("pdfx1a"));
                d.fields.insert("standard".into(), json!("x1a2001"));
            }
            if ui.selectable_label(cur_preset == "pdfx3", "PDF/X-3:2002").clicked() {
                d.fields.insert("preset".into(), json!("pdfx3"));
                d.fields.insert("standard".into(), json!("x32002"));
            }
            if ui.selectable_label(cur_preset == "pdfx4", "PDF/X-4:2010").clicked() {
                d.fields.insert("preset".into(), json!("pdfx4"));
                d.fields.insert("standard".into(), json!("x4"));
            }
            if ui.selectable_label(cur_preset == "pressQuality", crate::i18n::tr(&app.ui.language, "Press Quality")).clicked() {
                d.fields.insert("preset".into(), json!("pressQuality"));
                d.fields.insert("standard".into(), json!("x1a2001"));
            }

            if ui.selectable_label(cur_preset == "smallestFile", crate::i18n::tr(&app.ui.language, "Smallest File Size")).clicked() {
                d.fields.insert("preset".into(), json!("smallestFile"));
                d.fields.insert("standard".into(), json!("none"));
                d.fields.insert("compressImages".into(), json!(true));
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Standard:"));
        let cur_std = d.s("standard");
        let std_label = match cur_std.as_str() {
            "x1a" | "x1a2001" => "PDF/X-1a:2001",
            "x1a2003" => "PDF/X-1a:2003",
            "x3" | "x32002" => "PDF/X-3:2002",
            "x32003" => "PDF/X-3:2003",
            "x4" => "PDF/X-4:2010",
            "a2b" => "PDF/A-2b",
            _ => crate::i18n::tr(&app.ui.language, "None"),
        };
        egui::ComboBox::from_id_salt("pdf_std").selected_text(std_label).width(240.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_std == "none", crate::i18n::tr(&app.ui.language, "None")).clicked() {
                d.fields.insert("standard".into(), json!("none"));
            }
            if ui.selectable_label(cur_std == "x1a" || cur_std == "x1a2001", "PDF/X-1a:2001").clicked() {
                d.fields.insert("standard".into(), json!("x1a2001"));
            }
            if ui.selectable_label(cur_std == "x1a2003", "PDF/X-1a:2003").clicked() {
                d.fields.insert("standard".into(), json!("x1a2003"));
            }
            if ui.selectable_label(cur_std == "x3" || cur_std == "x32002", "PDF/X-3:2002").clicked() {
                d.fields.insert("standard".into(), json!("x32002"));
            }
            if ui.selectable_label(cur_std == "x32003", "PDF/X-3:2003").clicked() {
                d.fields.insert("standard".into(), json!("x32003"));
            }
            if ui.selectable_label(cur_std == "x4", "PDF/X-4:2010").clicked() {
                d.fields.insert("standard".into(), json!("x4"));
            }
            if ui.selectable_label(cur_std == "a2b", "PDF/A-2b").clicked() {
                d.fields.insert("standard".into(), json!("a2b"));
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Compatibility:"));
        let compat_label = match cur_std.as_str() {
            "x1a" | "x1a2001" | "x3" | "x32002" => "Acrobat 4 (PDF 1.3)",
            "x1a2003" | "x32003" => "Acrobat 5 (PDF 1.4)",
            "x4" => "Acrobat 7 (PDF 1.6)",
            _ => "Acrobat 8 (PDF 1.7)",
        };
        ui.label(compat_label);
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "File Name:"));
        text_field(ui, d, "filename", 240.0);
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Pages:"));
        ui.vertical(|ui| {
            let range = d.s("range");
            if ui.radio(range == "all", crate::i18n::tr(&app.ui.language, "All")).clicked() {
                d.fields.insert("range".into(), json!("all"));
            }
            ui.horizontal(|ui| {
                if ui.radio(range == "range", crate::i18n::tr(&app.ui.language, "Range:")).clicked() {
                    d.fields.insert("range".into(), json!("range"));
                }
                text_field(ui, d, "pages", 120.0);
            });
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Export As:"));
        ui.vertical(|ui| {
            check(ui, d, "spreads", crate::i18n::tr(&app.ui.language, "Spreads"));
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Marks and Bleed:"));
        ui.vertical(|ui| {
            check(ui, d, "bleed", crate::i18n::tr(&app.ui.language, "Include Bleed"));
            check(ui, d, "marks", crate::i18n::tr(&app.ui.language, "Crop Marks"));
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Options:"));
        ui.vertical(|ui| {
            check(ui, d, "tagged", crate::i18n::tr(&app.ui.language, "Create Tagged PDF"));
            check(ui, d, "compressImages", crate::i18n::tr(&app.ui.language, "Compress Images"));
        });
        ui.end_row();
    });
}

fn export_png_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let page_count = d.fields.get("pageCount").and_then(Value::as_u64).unwrap_or(1) as usize;
    egui::Grid::new("export_png_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "File Name:"));
        text_field(ui, d, "filename", 240.0);
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Page:"));
        ui.horizontal(|ui| {
            let mut cur_page = d.n("page").unwrap_or(1.0) as usize;
            if cur_page < 1 {
                cur_page = 1;
            }
            if cur_page > page_count {
                cur_page = page_count;
            }
            egui::ComboBox::from_id_salt("png_page").selected_text(format!("{} / {}", cur_page, page_count)).width(120.0).show_ui(ui, |ui| {
                for i in 1..=page_count {
                    if ui.selectable_label(cur_page == i, format!("Page {i}")).clicked() {
                        d.fields.insert("page".into(), json!(i));
                        let stem = app.session.active().map(|s| s.doc.title.as_str()).unwrap_or("document");
                        d.fields.insert("filename".into(), json!(format!("{stem}-p{i}.png")));
                    }
                }
            });
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Resolution:"));
        let cur_scale = d.n("scale").unwrap_or(2.0);
        let scale_label = match (cur_scale * 10.0).round() as i64 {
            10 => "72 DPI (1x)",
            20 => "144 DPI (2x - Retina)",
            30 => "216 DPI (3x)",
            40 => "300 DPI (4x - Print)",
            _ => "Custom",
        };
        egui::ComboBox::from_id_salt("png_scale").selected_text(scale_label).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_scale == 1.0, "72 DPI (1x)").clicked() {
                d.fields.insert("scale".into(), json!(1.0));
            }
            if ui.selectable_label(cur_scale == 2.0, "144 DPI (2x - Retina)").clicked() {
                d.fields.insert("scale".into(), json!(2.0));
            }
            if ui.selectable_label(cur_scale == 3.0, "216 DPI (3x)").clicked() {
                d.fields.insert("scale".into(), json!(3.0));
            }
            if ui.selectable_label(cur_scale == 4.0, "300 DPI (4x - Print)").clicked() {
                d.fields.insert("scale".into(), json!(4.0));
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Appearance:"));
        ui.vertical(|ui| {
            check(ui, d, "richBlack", crate::i18n::tr(&app.ui.language, "Rich Black Output"));
        });
        ui.end_row();
    });
}

fn save_as_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let is_web = app.services.download.is_some();
    egui::Grid::new("save_as_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Document Name:"));
        text_field(ui, d, "name", 260.0);
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Format:"));
        let cur_fmt = d.s("format");
        let fmt_label = if cur_fmt == "idml" { "InDesign Markup (*.idml)" } else { "DesignCraft Document (*.designcraft)" };
        egui::ComboBox::from_id_salt("save_as_format").selected_text(fmt_label).width(260.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_fmt != "idml", "DesignCraft Document (*.designcraft)").clicked() {
                d.fields.insert("format".into(), json!("designcraft"));
            }
            if ui.selectable_label(cur_fmt == "idml", "InDesign Markup (*.idml)").clicked() {
                d.fields.insert("format".into(), json!("idml"));
            }
        });
        ui.end_row();

        if is_web {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Destination:"));
            ui.vertical(|ui| {
                let target = d.s("target");
                if ui.radio(target != "download", crate::i18n::tr(&app.ui.language, "Save to Server")).clicked() {
                    d.fields.insert("target".into(), json!("server"));
                }
                if ui.radio(target == "download", crate::i18n::tr(&app.ui.language, "Download to Computer")).clicked() {
                    d.fields.insert("target".into(), json!("download"));
                }
            });
            ui.end_row();
        }
    });
}

fn save_copy_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let is_web = app.services.download.is_some();
    egui::Grid::new("save_copy_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Copy Name:"));
        text_field(ui, d, "name", 260.0);
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Format:"));
        let cur_fmt = d.s("format");
        let fmt_label = if cur_fmt == "idml" { "InDesign Markup (*.idml)" } else { "DesignCraft Document (*.designcraft)" };
        egui::ComboBox::from_id_salt("save_copy_format").selected_text(fmt_label).width(260.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_fmt != "idml", "DesignCraft Document (*.designcraft)").clicked() {
                d.fields.insert("format".into(), json!("designcraft"));
            }
            if ui.selectable_label(cur_fmt == "idml", "InDesign Markup (*.idml)").clicked() {
                d.fields.insert("format".into(), json!("idml"));
            }
        });
        ui.end_row();

        if is_web {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Destination:"));
            ui.vertical(|ui| {
                let target = d.s("target");
                if ui.radio(target != "download", crate::i18n::tr(&app.ui.language, "Save to Server")).clicked() {
                    d.fields.insert("target".into(), json!("server"));
                }
                if ui.radio(target == "download", crate::i18n::tr(&app.ui.language, "Download to Computer")).clicked() {
                    d.fields.insert("target".into(), json!("download"));
                }
            });
            ui.end_row();
        }
    });
}

fn overset_warning_dialog(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    ui.vertical(|ui| {
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(110, 24, 24))
            .corner_radius(6.0)
            .inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 10 })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚠").size(24.0).color(egui::Color32::from_rgb(255, 120, 120)));
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::tr(&app.ui.language, "Preflight Warning: Overset Text"))
                                .strong()
                                .color(egui::Color32::WHITE)
                                .size(14.5),
                        );
                        ui.label(
                            egui::RichText::new("Hay historias que exceden el marco maquetado. El texto cortado se perderá en el PDF de imprenta.")
                                .color(egui::Color32::from_rgb(240, 210, 210))
                                .size(11.5),
                        );
                    });
                });
            });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Historias afectadas:").strong().size(12.5));
        ui.add_space(4.0);

        let stories = d.fields.get("stories").and_then(Value::as_array).cloned().unwrap_or_default();

        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for item in &stories {
                let id = item["id"].as_u64().unwrap_or(0);
                let name = item["name"].as_str().unwrap_or("Historia");
                let lines = item["lines"].as_u64().unwrap_or(1);
                let chars = item["chars"].as_u64().unwrap_or(0);

                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_premultiplied(35, 35, 42, 220))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: 6 })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("📄 {name}")).strong().size(12.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui::RichText::new("✍ Corregir").color(egui::Color32::from_rgb(100, 220, 255))).clicked() {
                                    d.fields.insert("jump_to_story".into(), json!(id));
                                }
                                ui.label(
                                    egui::RichText::new(format!("+{lines} lín. (+{chars} chars)"))
                                        .color(egui::Color32::from_rgb(255, 120, 120))
                                        .size(11.0),
                                );
                            });
                        });
                    });
                ui.add_space(4.0);
            }
        });

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Pulsa 'Exportar de todos modos' para continuar o 'Cancelar' para regresar a editar.")
                .italics()
                .size(11.0)
                .color(egui::Color32::from_rgb(170, 170, 175)),
        );
    });
}

fn editorial_overview_dialog(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let overview_data = match app.session.execute("page.editorial.overview", &json!({})) {
        Ok(v) => v,
        Err(_) => {
            ui.label("No se pudo cargar el panorama de edición.");
            return;
        }
    };

    let page_count = overview_data["pageCount"].as_u64().unwrap_or(0);
    let approved_count = overview_data["approvedCount"].as_u64().unwrap_or(0);
    let overset_count = overview_data["oversetCount"].as_u64().unwrap_or(0);
    let pages = overview_data["pages"].as_array().cloned().unwrap_or_default();

    let cur_filter = d.s("filter");
    let active_filter = if cur_filter.is_empty() { "all" } else { &cur_filter };

    // --- Header Summary Bar ---
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("📰 Edición del Periódico").strong().size(15.0));
        ui.separator();
        ui.label(format!("Total Páginas: {page_count}"));
        ui.separator();

        let pct = if page_count > 0 { (approved_count as f32 / page_count as f32) * 100.0 } else { 0.0 };
        let app_color =
            if approved_count == page_count && page_count > 0 { egui::Color32::from_rgb(46, 160, 67) } else { egui::Color32::from_rgb(56, 139, 253) };
        ui.colored_label(app_color, format!("🟢 Aprobadas para CTP: {approved_count}/{page_count} ({pct:.0}%)"));

        if overset_count > 0 {
            ui.separator();
            ui.colored_label(egui::Color32::from_rgb(248, 81, 73), format!("🔴 Con Overset: {overset_count}"));
        }
    });

    ui.add_space(8.0);

    // --- Filter Buttons ---
    ui.horizontal(|ui| {
        let mut set_filter = |f: &str, label: &str| {
            let active = active_filter == f;
            if ui.selectable_label(active, label).clicked() {
                d.fields.insert("filter".into(), json!(f));
            }
        };
        set_filter("all", &format!("Todas ({page_count})"));
        set_filter("pending", &format!("Pendientes ({})", page_count.saturating_sub(approved_count)));
        if overset_count > 0 {
            set_filter("overset", &format!("⚠ Con Overset ({overset_count})"));
        }
        set_filter("approved", &format!("✓ Aprobadas ({approved_count})"));
    });

    ui.separator();
    ui.add_space(6.0);

    // --- Grid of Page Cards ---
    let mut jump_to_page: Option<usize> = None;
    let mut action_toggle_approve: Option<(usize, bool)> = None;
    let mut action_needs_review: Option<usize> = None;

    egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
        let card_w = 205.0;
        let thumb_h = 135.0;

        let filtered_pages: Vec<&Value> = pages
            .iter()
            .filter(|p| {
                let status = p["status"].as_str().unwrap_or("draft");
                let has_ov = p["hasOverset"].as_bool().unwrap_or(false);
                match active_filter {
                    "approved" => status == "approved",
                    "pending" => status != "approved",
                    "overset" => has_ov,
                    _ => true,
                }
            })
            .collect();

        if filtered_pages.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(egui::RichText::new("No hay páginas que coincidan con el filtro seleccionado.").italics());
            });
            return;
        }

        ui.horizontal_wrapped(|ui| {
            for page in filtered_pages {
                let p_idx = page["page"].as_u64().unwrap_or(0) as usize;
                let p_name = page["pageNumber"].as_str().unwrap_or("");
                let status_str = page["status"].as_str().unwrap_or("draft");
                let is_approved = status_str == "approved";
                let section = page["section"].as_str().unwrap_or("General");
                let assigned = page["assignedTo"].as_str();
                let has_overset = page["hasOverset"].as_bool().unwrap_or(false);

                let border_color = if has_overset {
                    egui::Color32::from_rgb(248, 81, 73)
                } else if is_approved {
                    egui::Color32::from_rgb(46, 160, 67)
                } else if status_str == "needsReview" {
                    egui::Color32::from_rgb(56, 139, 253)
                } else if status_str == "inProgress" {
                    egui::Color32::from_rgb(210, 153, 34)
                } else {
                    egui::Color32::from_gray(80)
                };

                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_premultiplied(30, 30, 36, 240))
                    .corner_radius(6.0)
                    .stroke(egui::Stroke::new(if has_overset || is_approved { 2.0 } else { 1.0 }, border_color))
                    .inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: 6 })
                    .show(ui, |ui| {
                        ui.set_width(card_w);

                        // Card Header
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Pág. {p_name}")).strong().size(12.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(section).size(11.0).color(egui::Color32::from_gray(180)));
                            });
                        });

                        ui.add_space(4.0);

                        // Thumbnail
                        let tex = crate::panels::pages::thumb(app, ui.ctx(), p_idx, thumb_h);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_w, thumb_h), egui::Sense::click());
                        if let Some(t) = tex {
                            let img_w = (t.size()[0] as f32 / t.size()[1] as f32) * thumb_h;
                            let fit_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(img_w.min(card_w), thumb_h));
                            ui.painter().image(
                                t.id(),
                                fit_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        } else {
                            ui.painter().rect_filled(rect, 4.0, egui::Color32::from_gray(40));
                        }

                        if resp.hovered() {
                            ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(2.0, egui::Color32::WHITE), egui::StrokeKind::Outside);
                        }
                        if resp.clicked() {
                            jump_to_page = Some(p_idx);
                        }

                        ui.add_space(4.0);

                        // Status pill
                        ui.horizontal(|ui| {
                            let (badge_txt, badge_col) = match status_str {
                                "approved" => ("🟢 APROBADA (CTP)", egui::Color32::from_rgb(46, 160, 67)),
                                "needsReview" => ("🔵 EN REVISIÓN", egui::Color32::from_rgb(56, 139, 253)),
                                "inProgress" => ("🟡 EN REDACCIÓN", egui::Color32::from_rgb(210, 153, 34)),
                                _ => ("⚪ BORRADOR", egui::Color32::from_gray(140)),
                            };
                            ui.colored_label(badge_col, egui::RichText::new(badge_txt).size(10.5).strong());
                        });

                        if has_overset {
                            ui.colored_label(egui::Color32::from_rgb(248, 81, 73), egui::RichText::new("⚠ TEXTO DESBORDADO").size(10.5).strong());
                        }

                        if let Some(who) = assigned {
                            ui.label(egui::RichText::new(format!("👤 {who}")).size(10.5).color(egui::Color32::from_gray(190)));
                        }

                        ui.add_space(4.0);

                        // Card Actions
                        ui.horizontal(|ui| {
                            if ui.button(egui::RichText::new("Ir a pág").size(11.0)).clicked() {
                                jump_to_page = Some(p_idx);
                            }

                            if !app.session.editor_mode {
                                if is_approved {
                                    if ui.button(egui::RichText::new("🔓 Reabrir").size(11.0)).clicked() {
                                        action_toggle_approve = Some((p_idx, false));
                                    }
                                } else {
                                    let btn = egui::Button::new(egui::RichText::new("✓ Aprobar").size(11.0).color(egui::Color32::WHITE))
                                        .fill(egui::Color32::from_rgb(35, 134, 54));
                                    if ui.add(btn).clicked() {
                                        action_toggle_approve = Some((p_idx, true));
                                    }
                                }
                            } else if !is_approved && status_str != "needsReview" && ui.button(egui::RichText::new("Listo").size(11.0)).clicked() {
                                action_needs_review = Some(p_idx);
                            }
                        });
                    });
                ui.add_space(8.0);
            }
        });
    });

    if let Some((p_idx, approve)) = action_toggle_approve {
        let _ = app.run("page.editorial.approve", json!({"page": p_idx, "approved": approve, "by": "Editor Jefe"}));
    }
    if let Some(p_idx) = action_needs_review {
        let _ = app.run("page.editorial.set", json!({"page": p_idx, "status": "needsReview"}));
    }
    if let Some(p_idx) = jump_to_page {
        crate::canvas::go_to_page(app, p_idx);
        app.ui.dialog = None;
    }
}

/// File › Document Setup, filled from the document.
pub fn open_document_setup(app: &mut DesignApp) {
    let Ok(cur) = app.session.execute("layout.documentSetup", &json!({})) else { return };
    let units = app.session.active().map(|s| s.doc.settings.horizontal_units).unwrap_or(Unit::Picas);
    let fm = |v: &Value| json!(format_measure(v.as_f64().unwrap_or(0.0), units));
    let mut f = json!({"intent": cur["intent"], "pages": cur["pages"], "startPage": cur["startPage"], "facingPages": cur["facingPages"],
        "binding": cur["binding"], "width": fm(&cur["width"]), "height": fm(&cur["height"])});
    for k in ["bleed", "slug"] {
        for (i, e) in ["Top", "Bottom", "Inside", "Outside"].iter().enumerate() {
            f[format!("{k}{e}")] = fm(&cur[k][i]);
        }
    }
    app.ui.dialog = Some(Dialog::new("documentSetup", f));
}

fn document_setup(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    egui::Grid::new("ds").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Intent:"));
        let cur = d.s("intent");
        egui::ComboBox::from_id_salt("ds_intent")
            .selected_text(match cur.as_str() {
                "web" => crate::i18n::tr(&app.ui.language, "Web"),
                "mobile" => crate::i18n::tr(&app.ui.language, "Mobile"),
                _ => crate::i18n::tr(&app.ui.language, "Print"),
            })
            .width(110.0)
            .show_ui(ui, |ui| {
                for (v, l) in [("print", "Print"), ("web", "Web"), ("mobile", "Mobile")] {
                    if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                        d.fields.insert("intent".into(), json!(v));
                    }
                }
            });
        ui.label("");
        ui.label("");
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Number of Pages:"));
        text_field(ui, d, "pages", 60.0);
        check(ui, d, "facingPages", crate::i18n::tr(&app.ui.language, "Facing Pages"));
        ui.label("");
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start Page #:"));
        text_field(ui, d, "startPage", 60.0);
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Binding:"));
        let rtl = d.s("binding") == "rightToLeft";
        egui::ComboBox::from_id_salt("ds_binding")
            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, if rtl { "Right to Left" } else { "Left to Right" })))
            .width(110.0)
            .show_ui(ui, |ui| {
                for (v, l) in [("leftToRight", "Left to Right"), ("rightToLeft", "Right to Left")] {
                    if ui.selectable_label((v == "rightToLeft") == rtl, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                        d.fields.insert("binding".into(), json!(v));
                    }
                }
            });
        ui.end_row();
    });
    ui.add_space(8.0);
    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Page Size")).font(semibold(12.0)));
    let (w, h) = (d.m("width").unwrap_or(612.0), d.m("height").unwrap_or(792.0));
    let preset =
        designcraft_doc::build::PRESETS.iter().find(|p| (p.width - w).abs() < 0.5 && (p.height - h).abs() < 0.5).map_or("Custom", |p| p.name);
    egui::Grid::new("ds_size").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Size:"));
        egui::ComboBox::from_id_salt("ds_preset").selected_text(preset).width(140.0).show_ui(ui, |ui| {
            for p in designcraft_doc::build::PRESETS {
                if ui.selectable_label(p.name == preset, p.name).clicked() {
                    d.fields.insert("width".into(), json!(format_measure(p.width, p.units)));
                    d.fields.insert("height".into(), json!(format_measure(p.height, p.units)));
                }
            }
        });
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Orientation:"));
        ui.horizontal(|ui| {
            for (portrait, l) in [(true, "Portrait"), (false, "Landscape")] {
                if ui.selectable_label((h >= w) == portrait, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked()
                    && (h >= w) != portrait
                {
                    let (fw, fh) = (d.s("width"), d.s("height"));
                    d.fields.insert("width".into(), json!(fh));
                    d.fields.insert("height".into(), json!(fw));
                }
            }
        });
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Width:"));
        text_field(ui, d, "width", 80.0);
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Height:"));
        text_field(ui, d, "height", 80.0);
        ui.end_row();
    });
    ui.add_space(8.0);
    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Bleed and Slug")).font(semibold(12.0)));
    egui::Grid::new("ds_bleed").num_columns(5).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("");
        for e in ["Top", "Bottom", "Inside", "Outside"] {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, e));
        }
        ui.end_row();
        for (k, l) in [("bleed", "Bleed:"), ("slug", "Slug:")] {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
            for e in ["Top", "Bottom", "Inside", "Outside"] {
                text_field(ui, d, &format!("{k}{e}"), 60.0);
            }
            ui.end_row();
        }
    });
    check(ui, d, "adjustLayout", crate::i18n::tr(&app.ui.language, "Adjust Layout (objects follow the new page size)"));
}

/// Preferences: a section list and the section's options. Application options always; units and
/// increments when a document is open (InDesign keeps those with the document).
fn preferences(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let has_doc = d.fields.contains_key("horizontalUnits");
    let sections: &[(&str, &str)] = if has_doc {
        &[
            ("general", "General"),
            ("interface", "Interface"),
            ("type", "Type"),
            ("advancedType", "Advanced Type"),
            ("composition", "Composition"),
            ("units", "Units & Increments"),
            ("grids", "Grids"),
            ("guides", "Guides & Pasteboard"),
            ("dictionary", "Dictionary"),
            ("spelling", "Spelling"),
            ("autocorrect", "Autocorrect"),
            ("notes", "Notes"),
            ("trackChanges", "Track Changes"),
            ("storyEditor", "Story Editor Display"),
            ("display", "Display Performance"),
            ("black", "Appearance of Black"),
            ("files", "File Handling"),
        ]
    } else {
        &[
            ("general", "General"),
            ("interface", "Interface"),
            ("type", "Type"),
            ("composition", "Composition"),
            ("spelling", "Spelling"),
            ("autocorrect", "Autocorrect"),
            ("notes", "Notes"),
            ("trackChanges", "Track Changes"),
            ("storyEditor", "Story Editor Display"),
            ("display", "Display Performance"),
            ("files", "File Handling"),
        ]
    };
    let cur = d.s("section");
    ui.horizontal_top(|ui| {
        // A fixed height: the separator would otherwise take the whole window.
        ui.set_min_height(240.0);
        ui.set_max_height(240.0);
        ui.vertical(|ui| {
            ui.set_width(150.0);
            for (id, label) in sections {
                if ui.selectable_label(cur == *id, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                    d.fields.insert("section".into(), json!(id));
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_min_width(300.0);
            match cur.as_str() {
                "dictionary" => {
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "User Dictionary (this document)")).font(semibold(12.0)),
                    ));
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Words, one per line:"));
                    let mut t = d.s("userWords");
                    if egui::ScrollArea::vertical()
                        .max_height(170.0)
                        .show(ui, |ui| ui.add(egui::TextEdit::multiline(&mut t).desired_rows(8).desired_width(280.0)))
                        .inner
                        .changed()
                    {
                        d.fields.insert("userWords".into(), json!(t));
                    }
                }
                "spelling" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Dynamic Spelling")).font(semibold(12.0)));
                    check(ui, d, "dynamicSpelling", crate::i18n::tr(&app.ui.language, "Enable Dynamic Spelling (underline misspelled words)"));
                }
                "autocorrect" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Options")).font(semibold(12.0)));
                    check(ui, d, "autocorrect", crate::i18n::tr(&app.ui.language, "Enable Autocorrect"));
                    ui.add_space(6.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Misspelled word → Correction (one per line):"));
                    let mut t = d.s("autocorrectText");
                    if egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .show(ui, |ui| ui.add(egui::TextEdit::multiline(&mut t).desired_rows(8).desired_width(280.0)))
                        .inner
                        .changed()
                    {
                        d.fields.insert("autocorrectText".into(), json!(t));
                    }
                }
                "notes" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Options")).font(semibold(12.0)));
                    check(ui, d, "showNoteAnchors", crate::i18n::tr(&app.ui.language, "Show Note Anchors in Layout View"));
                }
                "trackChanges" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Show")).font(semibold(12.0)));
                    check(ui, d, "showAddedText", crate::i18n::tr(&app.ui.language, "Added Text (highlighted)"));
                }
                "storyEditor" => {
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Text Display Options")).font(semibold(12.0)),
                    ));
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Font Size:"));
                        let mut v = d.n("storyEditorSize").unwrap_or(14.0);
                        if ui.add(egui::Slider::new(&mut v, 8.0..=36.0).step_by(1.0).suffix(" pt")).changed() {
                            d.fields.insert("storyEditorSize".into(), json!(v));
                        }
                    });
                }
                "files" => {
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Document Recovery Data")).font(semibold(12.0)),
                    ));
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Save recovery data every:"));
                        let mut v = d.n("recoveryMinutes").unwrap_or(0.5);
                        if ui.add(egui::DragValue::new(&mut v).range(0.1..=60.0).speed(0.1).suffix(" min")).changed() {
                            d.fields.insert("recoveryMinutes".into(), json!(v));
                        }
                    });
                }
                "type" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Type Options")).font(semibold(12.0)));
                    check(ui, d, "typographersQuotes", crate::i18n::tr(&app.ui.language, "Use Typographer's Quotes"));
                    check(ui, d, "showFontNamesInEnglish", crate::i18n::tr(&app.ui.language, "Show Font Names in English"));
                    ui.add_space(6.0);
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Smart Text Reflow")).font(semibold(12.0)),
                    ));
                    check(
                        ui,
                        d,
                        "smartTextReflow",
                        crate::i18n::tr(&app.ui.language, "Add and remove pages as the primary text frame's story grows and shrinks"),
                    );
                }
                "composition" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Highlight")).font(semibold(12.0)));
                    check(ui, d, "highlightKeeps", crate::i18n::tr(&app.ui.language, "Keep Violations"));
                    check(ui, d, "highlightHj", crate::i18n::tr(&app.ui.language, "H&J Violations"));
                    check(ui, d, "highlightCustomTracking", crate::i18n::tr(&app.ui.language, "Custom Tracking/Kerning"));
                    check(ui, d, "highlightSubstitutedFonts", crate::i18n::tr(&app.ui.language, "Substituted Fonts"));
                    if d.fields.contains_key("glyphFallback") {
                        ui.add_space(6.0);
                        crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Missing Glyphs")).font(semibold(12.0)));
                        check(ui, d, "glyphFallback", crate::i18n::tr(&app.ui.language, "Draw Missing Glyphs from Fallback Fonts"));
                    }
                }
                "advancedType" => {
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Character Settings")).font(semibold(12.0)),
                    ));
                    egui::Grid::new("pref_adv").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("");
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Size"));
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Position"));
                        ui.end_row();
                        for (k, l) in [("superscript", "Superscript:"), ("subscript", "Subscript:")] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
                            for f in ["Size", "Position"] {
                                ui.horizontal(|ui| {
                                    text_field(ui, d, &format!("adv.{k}{f}"), 50.0);
                                    ui.label("%");
                                });
                            }
                            ui.end_row();
                        }
                    });
                }
                "black" => {
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Options for Black on RGB and Grayscale Devices")).font(semibold(12.0)),
                    ));
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "On Screen:"));
                        let rich = d.b("richBlack");
                        egui::ComboBox::from_id_salt("pref_black")
                            .selected_text(crate::rtl::widget(
                                ui,
                                crate::i18n::tr(
                                    &app.ui.language,
                                    if rich { "Display All Blacks as Rich Black" } else { "Display All Blacks Accurately" },
                                ),
                            ))
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for (v, l) in [(false, "Display All Blacks Accurately"), (true, "Display All Blacks as Rich Black")] {
                                    if ui.selectable_label(v == rich, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                        d.fields.insert("richBlack".into(), json!(v));
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Printing / Exporting:"));
                        let rich = d.b("richBlackOutput");
                        egui::ComboBox::from_id_salt("pref_black_out")
                            .selected_text(crate::rtl::widget(
                                ui,
                                crate::i18n::tr(
                                    &app.ui.language,
                                    if rich { "Output All Blacks as Rich Black" } else { "Output All Blacks Accurately" },
                                ),
                            ))
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for (v, l) in [(false, "Output All Blacks Accurately"), (true, "Output All Blacks as Rich Black")] {
                                    if ui.selectable_label(v == rich, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                        d.fields.insert("richBlackOutput".into(), json!(v));
                                    }
                                }
                            });
                    });
                    if d.fields.contains_key("overprintBlack") {
                        ui.add_space(6.0);
                        ui.label(crate::rtl::widget(
                            ui,
                            egui::RichText::new(crate::i18n::tr(&app.ui.language, "[Black] Overprint")).font(semibold(12.0)),
                        ));
                        check(ui, d, "overprintBlack", crate::i18n::tr(&app.ui.language, "Overprint [Black] Swatch at 100%"));
                    }
                }
                "interface" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "UI Scaling")).font(semibold(12.0)));
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "UI Size:"));
                        let mut v = d.n("uiScale").unwrap_or(100.0);
                        if ui.add(egui::Slider::new(&mut v, 50.0..=200.0).step_by(5.0).suffix("%")).changed() {
                            d.fields.insert("uiScale".into(), json!(v));
                        }
                    });
                }
                "grids" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Baseline Grid")).font(semibold(12.0)));
                    egui::Grid::new("pref_bg").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                        color_field(ui, d, "bg.color");
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start:"));
                        text_field(ui, d, "bg.start", 80.0);
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Relative To:"));
                        let cur = d.s("bg.relativeTo");
                        egui::ComboBox::from_id_salt("bg_rel")
                            .selected_text(crate::rtl::widget(
                                ui,
                                crate::i18n::tr(&app.ui.language, if cur == "topMargin" { "Top Margin" } else { "Top of Page" }),
                            ))
                            .show_ui(ui, |ui| {
                                for (v, l) in [("topOfPage", "Top of Page"), ("topMargin", "Top Margin")] {
                                    if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                        d.fields.insert("bg.relativeTo".into(), json!(v));
                                    }
                                }
                            });
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Increment Every:"));
                        text_field(ui, d, "bg.increment", 80.0);
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "View Threshold:"));
                        ui.horizontal(|ui| {
                            text_field(ui, d, "bg.viewThreshold", 50.0);
                            ui.label("%");
                        });
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Document Grid")).font(semibold(12.0)));
                    egui::Grid::new("pref_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                        color_field(ui, d, "grid.color");
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Horizontal Gridline Every:"));
                        text_field(ui, d, "grid.horizontal", 80.0);
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Vertical Gridline Every:"));
                        text_field(ui, d, "grid.vertical", 80.0);
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Subdivisions:"));
                        text_field(ui, d, "grid.subdivisions", 50.0);
                        ui.end_row();
                    });
                    check(ui, d, "grid.inBack", crate::i18n::tr(&app.ui.language, "Grids in Back"));
                }
                "guides" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Color")).font(semibold(12.0)));
                    egui::Grid::new("pref_guides").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        for (k, l) in [("marginColor", "Margins:"), ("columnColor", "Columns:"), ("bleedColor", "Bleed:"), ("slugColor", "Slug:")] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
                            color_field(ui, d, k);
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Pasteboard Options")).font(semibold(12.0)),
                    ));
                    egui::Grid::new("pref_pb").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Horizontal Margins:"));
                        text_field(ui, d, "pasteboard.h", 80.0);
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Vertical Margins:"));
                        text_field(ui, d, "pasteboard.v", 80.0);
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    check(ui, d, "snap.alignEdges", "Align to Object Edges");
                    check(ui, d, "snap.alignCenters", "Align to Object Centers");
                    check(ui, d, "snap.dimensions", "Smart Dimensions");
                    check(ui, d, "snap.spacing", "Smart Spacing");
                    ui.horizontal(|ui| {
                        ui.label("Snap to Zone");
                        let mut zone = d.n("snap.zone").filter(|z| z.is_finite()).unwrap_or(4.0);
                        if ui.add(egui::DragValue::new(&mut zone).speed(0.1)).changed() {
                            d.fields.insert("snap.zone".into(), json!(zone));
                        }
                    });
                }
                "display" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Options")).font(semibold(12.0)));
                    let cur = d.s("displayQuality");
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Default View:"));
                        egui::ComboBox::from_id_salt("pref_dq")
                            .selected_text(match cur.as_str() {
                                "fast" => crate::i18n::tr(&app.ui.language, "Fast"),
                                "typical" => crate::i18n::tr(&app.ui.language, "Typical"),
                                _ => crate::i18n::tr(&app.ui.language, "High Quality"),
                            })
                            .show_ui(ui, |ui| {
                                for (v, l) in [("fast", "Fast"), ("typical", "Typical"), ("high", "High Quality")] {
                                    if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                        d.fields.insert("displayQuality".into(), json!(v));
                                    }
                                }
                            });
                    });
                }
                "units" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Ruler Units")).font(semibold(12.0)));
                    egui::Grid::new("pref_units").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        for (key, label) in [("horizontalUnits", "Horizontal:"), ("verticalUnits", "Vertical:")] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            let cur: Unit = d.fields.get(key).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                            egui::ComboBox::from_id_salt(key)
                                .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, cur.label())))
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for u in Unit::ALL {
                                        if ui
                                            .selectable_label(u == cur, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, u.label())))
                                            .clicked()
                                        {
                                            d.fields.insert(key.into(), json!(u));
                                        }
                                    }
                                });
                            ui.end_row();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(crate::rtl::widget(
                        ui,
                        egui::RichText::new(crate::i18n::tr(&app.ui.language, "Keyboard Increments")).font(semibold(12.0)),
                    ));
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Cursor Key:"));
                        text_field(ui, d, "keyboardIncrement", 80.0);
                    });
                }
                _ => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "When Scaling")).font(semibold(12.0)));
                    check(ui, d, "scaleStrokes", crate::i18n::tr(&app.ui.language, "Include Stroke Weight"));
                    ui.add_space(6.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Transform")).font(semibold(12.0)));
                    check(ui, d, "dimensionsIncludeStroke", crate::i18n::tr(&app.ui.language, "Dimensions Include Stroke Weight"));
                    check(ui, d, "transformationsAreTotals", crate::i18n::tr(&app.ui.language, "Transformations are Totals"));
                    ui.add_space(6.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Page Numbering")).font(semibold(12.0)));
                    check(ui, d, "absolutePageNumbers", crate::i18n::tr(&app.ui.language, "Absolute Numbering (instead of Section Numbering)"));
                }
            }
        });
    });
}

/// An `[r, g, b]` field edited with a colour button.
fn color_field(ui: &mut egui::Ui, d: &mut Dialog, key: &str) {
    let v: [u8; 3] = d.fields.get(key).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or([128, 128, 128]);
    let mut c = v;
    if egui::color_picker::color_edit_button_srgb(ui, &mut c).changed() {
        d.fields.insert(key.into(), json!(c));
    }
}

fn check(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str) {
    let mut b = d.b(key);
    if ui.checkbox(&mut b, crate::rtl::widget(ui, label)).changed() {
        d.fields.insert(key.into(), Value::Bool(b));
    }
}

pub fn show(app: &mut DesignApp, ctx: &egui::Context) {
    let Some(mut d) = app.ui.dialog.clone() else { return };
    let mut result: Option<bool> = None;
    let title = match d.id.as_str() {
        "ruby" => crate::i18n::tr(&app.ui.language, "Ruby"),
        "newDocument" => crate::i18n::tr(&app.ui.language, "New Document"),
        "frameSize" => crate::i18n::tr(&app.ui.language, "Rectangle"),
        "goToPage" => crate::i18n::tr(&app.ui.language, "Go to Page"),
        "insertTable" => crate::i18n::tr(&app.ui.language, "Create Table"),
        "textFrameOptions" => crate::i18n::tr(&app.ui.language, "Text Frame Options"),
        "documentSetup" => crate::i18n::tr(&app.ui.language, "Document Setup"),
        "findChange" => crate::i18n::tr(&app.ui.language, "Find/Change"),
        "paragraphStyleOptions" => crate::i18n::tr(&app.ui.language, "Paragraph Style Options"),
        "characterStyleOptions" => crate::i18n::tr(&app.ui.language, "Character Style Options"),
        "footnoteOptions" => crate::i18n::tr(&app.ui.language, "Footnote Options"),
        "insertXref" => crate::i18n::tr(&app.ui.language, "New Cross-Reference"),
        "findFont" => crate::i18n::tr(&app.ui.language, "Find/Replace Font"),
        "polygonSettings" => crate::i18n::tr(&app.ui.language, "Polygon Settings"),
        "userDictionary" => crate::i18n::tr(&app.ui.language, "User Dictionary"),
        "newWorkspace" => crate::i18n::tr(&app.ui.language, "New Workspace"),
        "menus" => crate::i18n::tr(&app.ui.language, "Menu Customization"),
        "importOptions" => crate::i18n::tr(&app.ui.language, "Import Options"),
        "fittingOptions" => crate::i18n::tr(&app.ui.language, "Frame Fitting Options"),
        "qrCode" => crate::i18n::tr(&app.ui.language, "Generate QR Code"),
        "preferences" => crate::i18n::tr(&app.ui.language, "Preferences"),
        "print" => crate::i18n::tr(&app.ui.language, "Print"),
        "pdfImport" => crate::i18n::tr(&app.ui.language, "Place PDF"),
        "colorSettings" => crate::i18n::tr(&app.ui.language, "Color Settings"),
        "layerOptions" => crate::i18n::tr(&app.ui.language, "Object Layer Options"),
        "layerProperties" => crate::i18n::tr(&app.ui.language, "Layer Options"),
        "renameObject" => crate::i18n::tr(&app.ui.language, "Rename Object"),
        "scriptLabel" => crate::i18n::tr(&app.ui.language, "Script Label"),
        "keyboardShortcuts" => crate::i18n::tr(&app.ui.language, "Keyboard Shortcuts"),
        "colorPicker" => crate::i18n::tr(&app.ui.language, "Color Picker"),
        "saveAs" => crate::i18n::tr(&app.ui.language, "Save As…"),
        "saveCopy" => crate::i18n::tr(&app.ui.language, "Save a Copy…"),
        "exportPdf" => crate::i18n::tr(&app.ui.language, "Export Adobe PDF"),
        "exportPng" => crate::i18n::tr(&app.ui.language, "Export PNG"),
        "oversetWarning" => crate::i18n::tr(&app.ui.language, "Preflight Warning: Overset Text"),
        "editorialOverview" => crate::i18n::tr(&app.ui.language, "Panorama de Edición — Flatplan"),
        id => match id.strip_prefix("cmd:").and_then(designcraft_engine::find_command) {
            Some(c) => crate::i18n::tr(&app.ui.language, c.label.trim_end_matches('…')),
            None => crate::i18n::tr(&app.ui.language, "Dialog"),
        },
    };
    let is_style_dialog = d.id == "paragraphStyleOptions" || d.id == "characterStyleOptions";
    let is_overview = d.id == "editorialOverview";
    egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
        let max_w: f32 = (ctx.input(|i| i.raw.screen_rect.map(|r| r.width()).unwrap_or(1200.0)) - 32.0).max(300.0);
        ui.set_min_width(if is_overview { 940.0_f32.min(max_w) } else if is_style_dialog { 760.0_f32.min(max_w) } else { 380.0_f32.min(max_w) });
        ui.set_max_width(if is_overview {
            960.0_f32.min(max_w)
        } else if is_style_dialog {
            780.0_f32.min(max_w)
        } else if d.id == "newDocument" && crate::i18n::is_rtl(&app.ui.language) {
            380.0_f32.min(max_w)
        } else {
            640.0_f32.min(max_w)
        });
        if crate::i18n::is_rtl(&app.ui.language) {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::rtl::label(ui, egui::RichText::new(title).font(semibold(16.0)));
                });
            });
        } else {
            crate::rtl::label(ui, egui::RichText::new(title).font(semibold(16.0)));
        }
        if is_style_dialog && app.session.editor_mode {
            ui.label(crate::i18n::tr(&app.ui.language,"Configure style limits in designer mode"));
        }
        ui.add_space(10.0);
        ui.add_enabled_ui(!(is_style_dialog && app.session.editor_mode), |ui| {
        match d.id.as_str() {
            "newDocument" => {
                ui.horizontal(|ui| {
                    ui.with_layout(if crate::i18n::is_rtl(&app.ui.language) { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) }, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Preset"));
                    let cur = d.s("preset");
                    egui::ComboBox::from_id_salt("preset").selected_text(&cur).width(180.0).show_ui(ui, |ui| {
                        for p in designcraft_doc::build::PRESETS {
                            if ui.selectable_label(cur == p.name, p.name).clicked() {
                                d.fields.insert("preset".into(), json!(p.name));
                                d.fields.insert("width".into(), json!(format_measure(p.width, p.units)));
                                d.fields.insert("height".into(), json!(format_measure(p.height, p.units)));
                                d.fields.insert("facingPages".into(), json!(p.intent == designcraft_doc::Intent::Print));
                            }
                        }
                    });
                    });
                });
                if crate::i18n::is_rtl(&app.ui.language) {
                    // egui Grid currently supports LTR only. Use explicit RTL rows;
                    // the numeric editors retain LTR alignment and logical values.
                    for (first, key, second, other, toggle) in [
                        ("Width", "width", "Height", "height", false),
                        ("Pages", "pages", "Facing Pages", "facingPages", true),
                        ("Columns", "columns", "Gutter", "gutter", false),
                        ("Top", "marginTop", "Bottom", "marginBottom", false),
                        ("Inside", "marginInside", "Outside", "marginOutside", false),
                        ("Bleed", "bleed", "Primary Text Frame", "primaryTextFrame", true),
                    ] {
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, first));
                                text_field(ui, &mut d, key, 80.0);
                                if toggle {
                                    check(ui, &mut d, other, crate::i18n::tr(&app.ui.language, second));
                                } else {
                                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, second));
                                    text_field(ui, &mut d, other, 80.0);
                                }
                            });
                        });
                    }
                } else {
                egui::Grid::new("nd").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Width"));
                    text_field(ui, &mut d, "width", 80.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Height"));
                    text_field(ui, &mut d, "height", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Pages"));
                    text_field(ui, &mut d, "pages", 80.0);
                    ui.label("");
                    check(ui, &mut d, "facingPages", crate::i18n::tr(&app.ui.language, "Facing Pages"));
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Columns"));
                    text_field(ui, &mut d, "columns", 80.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Gutter"));
                    text_field(ui, &mut d, "gutter", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Top"));
                    text_field(ui, &mut d, "marginTop", 80.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bottom"));
                    text_field(ui, &mut d, "marginBottom", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Inside"));
                    text_field(ui, &mut d, "marginInside", 80.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Outside"));
                    text_field(ui, &mut d, "marginOutside", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bleed"));
                    text_field(ui, &mut d, "bleed", 80.0);
                    ui.label("");
                    check(ui, &mut d, "primaryTextFrame", crate::i18n::tr(&app.ui.language, "Primary Text Frame"));
                    ui.end_row();
                });
                }
            }
            "frameSize" => {
                egui::Grid::new("fs").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Width"));
                    text_field(ui, &mut d, "width", 90.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Height"));
                    text_field(ui, &mut d, "height", 90.0);
                    ui.end_row();
                });
            }
            "findChange" => {
                ui.horizontal(|ui| {
                    for (label, grep) in [("Text", false), ("GREP", true)] {
                        if ui.selectable_label(!d.b("objectMode") && d.b("grep") == grep, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                            d.fields.insert("grep".into(), json!(grep));
                            d.fields.insert("objectMode".into(), json!(false));
                        }
                    }
                    if ui.selectable_label(d.b("objectMode"),crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Object"))).clicked() {
                        d.fields.insert("objectMode".into(), json!(true));
                    }
                });
                if d.b("objectMode") {
                    // Find/Change › Object: objects by fill and kind; change their fill or opacity.
                    let swatches: Vec<String> = app.session.active().map(|st| st.doc.swatches.iter().filter(|w| !w.hidden).map(|w| w.name.clone()).collect()).unwrap_or_default();
                    egui::Grid::new("fco").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        for (key, label, any) in [("objFill", "Find fill:", "(any)"), ("objChangeFill", "Change fill to:", "(unchanged)")] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            let cur = d.s(key);
                            egui::ComboBox::from_id_salt(key).selected_text(if cur.is_empty() { any } else { cur.as_str() }).width(180.0).show_ui(ui, |ui| {
                                if ui.selectable_label(cur.is_empty(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, any))).clicked() {
                                    d.fields.insert(key.into(), json!(""));
                                }
                                for w in &swatches {
                                    if ui.selectable_label(*w == cur, w).clicked() {
                                        d.fields.insert(key.into(), json!(w));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Kind:"));
                        let cur = d.s("objKind");
                        egui::ComboBox::from_id_salt("objKind").selected_text(if cur.is_empty() { "(any)" } else { cur.as_str() }).show_ui(ui, |ui| {
                            for v in ["", "text", "graphic", "shape", "line", "group"] {
                                if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, if v.is_empty() { "(any)" } else { v }))).clicked() {
                                    d.fields.insert("objKind".into(), json!(v));
                                }
                            }
                        });
                        ui.end_row();
                    });
                    let mut crit = json!({});
                    if !d.s("objFill").is_empty() {
                        crit["fill"] = json!(d.s("objFill"));
                    }
                    if !d.s("objKind").is_empty() {
                        crit["kind"] = json!(d.s("objKind"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Find All"))).clicked() {
                            let n = app.run("find.objects", crit.clone()).ok().and_then(|r| r["ids"].as_array().map(Vec::len)).unwrap_or(0);
                            d.fields.insert("status".into(), json!(format!("{n} found")));
                        }
                        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Change All"))).clicked() && !d.s("objChangeFill").is_empty() {
                            let mut p = crit.clone();
                            p["change"] = json!({"fill": d.s("objChangeFill")});
                            let n = app.run("find.changeObjects", p).ok().and_then(|r| r["changed"].as_u64()).unwrap_or(0);
                            d.fields.insert("status".into(), json!(format!("{n} changed")));
                        }
                    });
                    ui.label(d.s("status"));
                } else {
                egui::Grid::new("fc").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Find what:"));
                    text_field(ui, &mut d, "find", 260.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Change to:"));
                    text_field(ui, &mut d, "change", 260.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Search:"));
                    let cur = d.s("scope");
                    egui::ComboBox::from_id_salt("fcscope").selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &cur))).show_ui(ui, |ui| {
                        for v in ["document", "story", "selection"] {
                            if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, v))).clicked() {
                                d.fields.insert("scope".into(), json!(v));
                            }
                        }
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    check(ui, &mut d, "caseSensitive", crate::i18n::tr(&app.ui.language, "Case sensitive"));
                    check(ui, &mut d, "wholeWord", crate::i18n::tr(&app.ui.language, "Whole word"));
                });
                ui.horizontal(|ui| {
                    let params0 = json!({"find": d.s("find"), "change": d.s("change"), "grep": d.b("grep"), "caseSensitive": d.b("caseSensitive"), "wholeWord": d.b("wholeWord"), "scope": d.s("scope")});
                    let params = || params0.clone();
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Find Next"))).clicked() {
                        let r = app.run("find.next", params());
                        let msg = match r {
                            Ok(Value::Null) => "No matches".to_string(),
                            Ok(_) => "Found".to_string(),
                            Err(e) => e,
                        };
                        d.fields.insert("status".into(), json!(msg));
                    }
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Change All"))).clicked() {
                        let msg = match app.run("find.change", params()) {
                            Ok(v) => format!("{} replacement(s) made", v["count"]),
                            Err(e) => e,
                        };
                        d.fields.insert("status".into(), json!(msg));
                    }
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Count"))).clicked() {
                        let msg = match app.run("find.find", params()) {
                            Ok(v) => format!("{} match(es)", v.as_array().map(|a| a.len()).unwrap_or(0)),
                            Err(e) => e,
                        };
                        d.fields.insert("status".into(), json!(msg));
                    }
                });
                if let Some(st) = d.fields.get("status").and_then(Value::as_str) {
                    ui.label(egui::RichText::new(st).color(crate::theme::Tokens::get(ui.ctx()).text_dim));
                }
                }
            }
            "paragraphStyleOptions" => paragraph_style_options(app, ui, &mut d),
            "characterStyleOptions" => character_style_options(app, ui, &mut d),
            "footnoteOptions" => footnote_options(app, ui, &mut d),
            "insertXref" => insert_xref(app, ui, &mut d),
            "findFont" => find_font(app, ui, &mut d),
            "colorPicker" => color_picker(ui, &mut d),
            "preferences" => preferences(app, ui, &mut d),
            "print" => print_dialog(app, ui, &mut d),
            "exportPdf" => export_pdf_dialog(app, ui, &mut d),
            "exportPng" => export_png_dialog(app, ui, &mut d),
            "saveAs" => save_as_dialog(app, ui, &mut d),
            "saveCopy" => save_copy_dialog(app, ui, &mut d),
            "oversetWarning" => overset_warning_dialog(app, ui, &mut d),
            "editorialOverview" => editorial_overview_dialog(app, ui, &mut d),
            "pdfImport" => {
                ui.label(egui::RichText::new(d.s("path")).size(11.0));
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Page (1–{count}):").replace("{count}", &d.n("pages").unwrap_or(1.0).to_string()));
                    text_field(ui, &mut d, "page", 60.0);
                });
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Crop to:"));
                    let cur = d.s("crop");
                    let label = |v: &str| match v {
                        "trim" => crate::i18n::tr(&app.ui.language, "Trim"),
                        "bleed" => crate::i18n::tr(&app.ui.language, "Bleed"),
                        "art" => crate::i18n::tr(&app.ui.language, "Art"),
                        "media" => crate::i18n::tr(&app.ui.language, "Media"),
                        _ => crate::i18n::tr(&app.ui.language, "Crop"),
                    };
                    egui::ComboBox::from_id_salt("pdf_crop").selected_text(crate::rtl::widget(ui, label(&cur))).show_ui(ui, |ui| {
                        for v in ["crop", "trim", "bleed", "art", "media"] {
                            if ui.selectable_label(label(&cur) == label(v), label(v)).clicked() {
                                d.fields.insert("crop".into(), json!(v));
                            }
                        }
                    });
                });
            }
            "layerOptions" => {
                let mut layers = d.fields.get("layers").and_then(Value::as_array).cloned().unwrap_or_default();
                if layers.is_empty() {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "This PDF has no layers."));
                }
                for l in &mut layers {
                    let mut on = l["visible"].as_bool().unwrap_or(true);
                    if ui.checkbox(&mut on, l["name"].as_str().unwrap_or("")).changed() {
                        l["visible"] = json!(on);
                    }
                }
                d.fields.insert("layers".into(), json!(layers));
            }
            "layerProperties" => {
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Name:"));
                    text_field(ui, &mut d, "name", 220.0);
                });
                ui.add_space(8.0);
                check(ui, &mut d, "visible", crate::i18n::tr(&app.ui.language, "Show Layer"));
                check(ui, &mut d, "locked", crate::i18n::tr(&app.ui.language, "Lock Layer"));
                check(ui, &mut d, "printable", crate::i18n::tr(&app.ui.language, "Print Layer"));
            }
            "renameObject" => {
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Name:"));
                    text_field(ui, &mut d, "name", 220.0);
                });
            }
            "scriptLabel" => {
                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Script Label / Tag:"));
                ui.add_space(4.0);
                text_field(ui, &mut d, "label", 300.0);
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(crate::i18n::tr(
                        &app.ui.language,
                        "Labels identify objects in scripts and automations without affecting layout.",
                    ))
                    .size(11.0)
                    .color(crate::theme::Tokens::get(ui.ctx()).text_dim),
                );
            }
            "colorSettings" => {
                let profiles = d.fields.get("profiles").and_then(Value::as_array).cloned().unwrap_or_default();
                egui::Grid::new("color_settings").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    for (key, label, kind) in [("rgb", "RGB:", "rgb"), ("cmyk", "CMYK:", "cmyk")] {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                        let cur = d.s(key);
                        egui::ComboBox::from_id_salt(("ws", key)).selected_text(&cur).width(260.0).show_ui(ui, |ui| {
                            for pr in profiles.iter().filter(|p| p["kind"] == kind) {
                                let name = pr["name"].as_str().unwrap_or("");
                                if ui.selectable_label(name == cur, name).clicked() {
                                    d.fields.insert(key.into(), json!(name));
                                }
                            }
                        });
                        ui.end_row();
                    }
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Intent:"));
                    let cur = d.s("intent");
                    egui::ComboBox::from_id_salt("ws_intent")
                        .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, designcraft_color::cms::Intent::parse(&cur).map_or("", |i| i.label()))))
                        .show_ui(ui, |ui| {
                            for i in designcraft_color::cms::Intent::ALL {
                                if ui.selectable_label(i.id() == cur, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, i.label()))).clicked() {
                                    d.fields.insert("intent".into(), json!(i.id()));
                                }
                            }
                        });
                    ui.end_row();
                });
                check(ui, &mut d, "bpc", crate::i18n::tr(&app.ui.language, "Use Black Point Compensation"));
                if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Load Profile…"))).clicked()
                    && let Some(path) = app.services.pick_open.as_mut().and_then(|f| f("icc"))
                {
                    match app.session.execute("color.loadProfile", &json!({"path": path})) {
                        Ok(_) => {
                            let f = app.session.execute("color.settings", &json!({})).unwrap_or_default();
                            d.fields.insert("profiles".into(), f["profiles"].clone());
                        }
                        Err(e) => app.status(format!("Color Settings: {e}")),
                    }
                }
            }
            "keyboardShortcuts" => keyboard_shortcuts(app, ui, &mut d),
            "userDictionary" => user_dictionary(app, ui, &mut d),
            "menus" => {
                text_field(ui, &mut d, "query", 260.0);
                let q = d.s("query").to_lowercase();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (menu, label) in crate::menus::all_menu_items().into_iter().filter(|(m, l)| q.is_empty() || m.to_lowercase().contains(&q) || l.to_lowercase().contains(&q)) {
                        let key = crate::menus::menu_key(&menu, &label);
                        let mut visible = !app.ui.hidden_menu_items.contains(&key);
                        if ui.checkbox(&mut visible, crate::rtl::widget(ui, format!("{} › {}", crate::i18n::tr(&app.ui.language, &menu), crate::i18n::tr(&app.ui.language, &label)))).changed() {
                            let _ = app.run("window.hideMenuItem", json!({"item": key, "hidden": !visible}));
                        }
                    }
                });
            }
            "importOptions" => {
                ui.label(egui::RichText::new(d.s("path")).size(10.5));
                check(ui, &mut d, "removeStyles", crate::i18n::tr(&app.ui.language, "Remove Styles and Formatting from Text and Tables"));
                if !d.b("removeStyles") {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Style Name Conflicts")).font(semibold(12.0)));
                    let conflicts: Vec<String> = d.fields.get("conflicts").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    ui.label(if conflicts.is_empty() { crate::i18n::tr(&app.ui.language, "No conflicts").to_string() } else { format!("{} conflict(s): {}", conflicts.len(), conflicts.join(", ")) });
                    let cur = d.s("styleConflicts");
                    ui.horizontal(|ui| {
                        for (v, l) in [("useExisting", "Use Document Style Definition"), ("redefine", "Redefine Document Style"), ("autoRename", "Auto Rename")] {
                            if ui.radio(cur == v, l).clicked() {
                                d.fields.insert("styleConflicts".into(), json!(v));
                            }
                        }
                    });
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Style Mapping")).font(semibold(12.0)));
                    let names: Vec<String> = app.session.active().map(|s| s.doc.styles.paragraph.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
                    let imported: Vec<String> = d.fields.get("styles").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        egui::Grid::new("imp_map").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                            for w in &imported {
                                ui.label(w);
                                let mapped = d.fields.get("map").and_then(|m| m.get(w)).and_then(Value::as_str).unwrap_or("").to_string();
                                let shown = if mapped.is_empty() { "(import)".to_string() } else { mapped.clone() };
                                egui::ComboBox::from_id_salt(("imp_map", w)).selected_text(crate::rtl::widget(ui, &shown)).width(170.0).show_ui(ui, |ui| {
                                    if ui.selectable_label(mapped.is_empty(),crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "(import)"))).clicked()
                                        && let Some(m) = d.fields.get_mut("map").and_then(Value::as_object_mut)
                                    {
                                        m.remove(w);
                                    }
                                    for n in &names {
                                        if ui.selectable_label(*n == mapped, n).clicked()
                                            && let Some(m) = d.fields.get_mut("map").and_then(Value::as_object_mut)
                                        {
                                            m.insert(w.clone(), json!(n));
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    });
                }
            }
            "newWorkspace" => {
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Name:"));
                    text_field(ui, &mut d, "name", 220.0);
                });
                crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Saves which bars show and where the panels are.")).size(10.5));
            }
            "fittingOptions" => {
                check(ui, &mut d, "autoFit", crate::i18n::tr(&app.ui.language, "Auto-Fit"));
                ui.add_space(4.0);
                crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Content Fitting")).font(semibold(12.0)));
                let cur = d.s("fitting");
                let label = |v: &str| match v {
                    "fillProportionally" => crate::i18n::tr(&app.ui.language, "Fill Frame Proportionally"),
                    "fitProportionally" => crate::i18n::tr(&app.ui.language, "Fit Content Proportionally"),
                    "fitContentToFrame" => crate::i18n::tr(&app.ui.language, "Fit Content to Frame"),
                    "centerContent" => crate::i18n::tr(&app.ui.language, "Center Content"),
                    _ => crate::i18n::tr(&app.ui.language, "None"),
                };
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Fitting:"));
                    egui::ComboBox::from_id_salt("ff_fit").selected_text(crate::rtl::widget(ui, label(&cur))).width(200.0).show_ui(ui, |ui| {
                        for v in ["none", "fillProportionally", "fitProportionally", "fitContentToFrame", "centerContent"] {
                            if ui.selectable_label(cur == v, crate::rtl::widget(ui, label(v))).clicked() {
                                d.fields.insert("fitting".into(), json!(v));
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Align From:"));
                    let a = d.n("align").unwrap_or(4.0) as u8;
                    egui::Grid::new("ff_align").spacing([2.0, 2.0]).show(ui, |ui| {
                        for row in 0..3u8 {
                            for col in 0..3u8 {
                                let i = row * 3 + col;
                                let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                                let c = ui.visuals().text_color();
                                if a == i {
                                    ui.painter().rect_filled(r.shrink(2.0), 1.0, c);
                                } else {
                                    ui.painter().rect_stroke(r.shrink(3.0), 1.0, egui::Stroke::new(1.0, c), egui::StrokeKind::Inside);
                                }
                                if resp.clicked() {
                                    d.fields.insert("align".into(), json!(i));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
                crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Crop Amount")).font(semibold(12.0)));
                egui::Grid::new("ff_crop").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                    for (k, l) in [("cropTop", "Top:"), ("cropLeft", "Left:")] {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
                        text_field(ui, &mut d, k, 70.0);
                    }
                    ui.end_row();
                    for (k, l) in [("cropBottom", "Bottom:"), ("cropRight", "Right:")] {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
                        text_field(ui, &mut d, k, 70.0);
                    }
                    ui.end_row();
                });
            }
            "qrCode" => {
                let cur = d.s("type");
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Type:"));
                    let label = |v: &str| match v {
                        "text" => crate::i18n::tr(&app.ui.language, "Plain Text"),
                        "sms" => crate::i18n::tr(&app.ui.language, "Text Message"),
                        "email" => crate::i18n::tr(&app.ui.language, "Email"),
                        "vcard" => crate::i18n::tr(&app.ui.language, "Business Card"),
                        _ => crate::i18n::tr(&app.ui.language, "Web Hyperlink"),
                    };
                    egui::ComboBox::from_id_salt("qr_type").selected_text(crate::rtl::widget(ui, label(&cur))).width(160.0).show_ui(ui, |ui| {
                        for v in ["url", "text", "sms", "email", "vcard"] {
                            if ui.selectable_label(cur == v, crate::rtl::widget(ui, label(v))).clicked() {
                                d.fields.insert("type".into(), json!(v));
                            }
                        }
                    });
                });
                let fields: &[(&str, &str)] = match cur.as_str() {
                    "sms" => &[("number", "Cell Number:"), ("message", "Message:")],
                    "email" => &[("to", "Email Address:"), ("subject", "Subject:"), ("body", "Message:")],
                    "vcard" => &[("name", "Name:"), ("org", "Organization:"), ("phone", "Phone:"), ("email", "Email:"), ("url", "URL:")],
                    "text" => &[("content", "Text:")],
                    _ => &[("content", "URL:")],
                };
                egui::Grid::new("qr_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    for (k, l) in fields {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, l));
                        text_field(ui, &mut d, k, 260.0);
                        ui.end_row();
                    }
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                    text_field(ui, &mut d, "color", 160.0);
                    ui.end_row();
                });
            }
            "polygonSettings" => {
                egui::Grid::new("poly").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Number of Sides:"));
                    text_field(ui, &mut d, "sides", 60.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Star Inset:"));
                    ui.horizontal(|ui| {
                        text_field(ui, &mut d, "starInset", 60.0);
                        ui.label("%");
                    });
                    ui.end_row();
                });
            }
            id if id.starts_with("cmd:") => command_form(app, ui, &mut d),
            "goToPage" => {
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Page"));
                    text_field(ui, &mut d, "page", 80.0);
                });
            }
            "ruby" => {
                ui.horizontal(|ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Ruby:"));
                    text_field(ui, &mut d, "text", 200.0);
                });
                crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Set over the selected text; empty removes it.")).size(11.0));
            }
            "insertTable" => {
                crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Table Dimensions")).font(semibold(12.0)));
                egui::Grid::new("ins_table").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Body Rows"));
                    text_field(ui, &mut d, "bodyRows", 60.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Columns"));
                    text_field(ui, &mut d, "columns", 60.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Header Rows"));
                    text_field(ui, &mut d, "headerRows", 60.0);
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Footer Rows"));
                    text_field(ui, &mut d, "footerRows", 60.0);
                    ui.end_row();
                });
            }
            "textFrameOptions" => {
                egui::Grid::new("tfo").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Number of columns"));
                    text_field(ui, &mut d, "columns", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Gutter"));
                    text_field(ui, &mut d, "gutter", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Inset spacing"));
                    text_field(ui, &mut d, "inset", 80.0);
                    ui.end_row();
                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Vertical justification"));
                    let cur = d.s("verticalJustification");
                    egui::ComboBox::from_id_salt("vj").selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &cur))).show_ui(ui, |ui| {
                        for v in ["top", "center", "bottom", "justify"] {
                            if ui.selectable_label(cur == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, v))).clicked() {
                                d.fields.insert("verticalJustification".into(), json!(v));
                            }
                        }
                    });
                    ui.end_row();
                });
            }
            "documentSetup" => document_setup(app, ui, &mut d),
            _ => {}
        }
        });
        if let Some(error)=d.fields.get("error").and_then(Value::as_str) {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).copyfit_error(),crate::i18n::tr(&app.ui.language,error));
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if is_style_dialog {
                let mut prev = d.b("preview");
                if ui.checkbox(&mut prev, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Preview"))).changed() {
                    d.fields.insert("preview".into(), json!(prev));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_warning = d.id == "oversetWarning";
                let is_overview = d.id == "editorialOverview";
                let ok_text = if is_warning {
                    "Exportar de todos modos"
                } else if is_overview {
                    "Cerrar"
                } else {
                    crate::i18n::tr(&app.ui.language, "  OK  ")
                };
                let btn_color = if is_warning {
                    egui::Color32::from_rgb(180, 45, 45)
                } else if is_overview {
                    egui::Color32::from_rgb(45, 110, 180)
                } else {
                    crate::theme::Tokens::get(ui.ctx()).accent_strong
                };
                if ui
                    .add_enabled(!(is_style_dialog && app.session.editor_mode), egui::Button::new(crate::rtl::widget(ui, egui::RichText::new(ok_text).color(egui::Color32::WHITE))).fill(btn_color))
                    .clicked()
                    || (!(is_style_dialog && app.session.editor_mode) && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    result = Some(true);
                }
                if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Cancel"))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    result = Some(false);
                }
            });
        });
    });
    if let Some(target_sid) = d.fields.remove("jump_to_story").and_then(|v| v.as_u64()) {
        app.story_editor = Some(designcraft_doc::StoryId(target_sid));
        app.ui.dialog = None;
        return;
    }
    app.ui.dialog = Some(d);
    match result {
        Some(true) => {
            if let Err(e) = confirm(app) {
                app.status(format!("Error: {e}"));
            }
        }
        Some(false) => app.ui.dialog = None,
        None => {}
    }
}

/// Apply the open dialog.
pub fn confirm(app: &mut DesignApp) -> Result<Value, String> {
    let saved = app.ui.dialog.clone().filter(|d| matches!(d.id.as_str(), "paragraphStyleOptions" | "characterStyleOptions"));
    let result = confirm_inner(app);
    if let Err(error) = &result
        && let Some(mut saved) = saved
    {
        saved.fields.insert("error".into(), json!(error));
        app.ui.dialog = Some(saved);
    }
    result
}

fn confirm_inner(app: &mut DesignApp) -> Result<Value, String> {
    let Some(d) = app.ui.dialog.take() else { return Err("no dialog open".into()) };
    match d.id.as_str() {
        "newDocument" => {
            let margins = json!({"top": d.m("marginTop").unwrap_or(36.0), "bottom": d.m("marginBottom").unwrap_or(36.0), "inside": d.m("marginInside").unwrap_or(36.0), "outside": d.m("marginOutside").unwrap_or(36.0)});
            app.run(
                "file.new",
                json!({"width": d.m("width"), "height": d.m("height"), "pages": d.n("pages").unwrap_or(1.0) as u64, "facingPages": d.b("facingPages"),
                    "columns": d.n("columns").unwrap_or(1.0) as u64, "gutter": d.m("gutter").unwrap_or(12.0), "margins": margins, "bleed": d.m("bleed").unwrap_or(0.0),
                    "primaryTextFrame": d.b("primaryTextFrame")}),
            )
        }
        "frameSize" => {
            let x = d.fields.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = d.fields.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            let w = d.m("width").unwrap_or(72.0);
            let h = d.m("height").unwrap_or(72.0);
            app.run(
                "frame.create",
                json!({"spread": d.fields.get("spread").cloned().unwrap_or(json!(0)), "shape": d.s("shape"), "content": d.s("content"), "rect": [x, y, x + w, y + h]}),
            )
        }
        "ruby" => app.run("type.ruby", json!({"text": d.s("text")})),
        "goToPage" => {
            // A page name ("iv", "A-3"), a number, or "+n" for an absolute position.
            let typed = match d.fields.get("page") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "1".into(),
            };
            match app.session.resolve_page(&typed) {
                Some(abs) => {
                    crate::canvas::go_to_page(app, abs);
                    Ok(Value::Null)
                }
                None => Err(format!("no page \"{typed}\"")),
            }
        }
        "insertTable" => app.run(
            "table.insert",
            json!({"rows": d.n("bodyRows").unwrap_or(4.0).max(1.0) as u64, "cols": d.n("columns").unwrap_or(4.0).max(1.0) as u64,
                "headerRows": d.n("headerRows").unwrap_or(0.0).max(0.0) as u64, "footerRows": d.n("footerRows").unwrap_or(0.0).max(0.0) as u64}),
        ),
        "textFrameOptions" => app.run(
            "object.textFrameOptions",
            json!({"columns": d.n("columns").unwrap_or(1.0) as u64, "gutter": d.m("gutter").unwrap_or(12.0), "inset": d.m("inset").unwrap_or(0.0), "verticalJustification": d.s("verticalJustification")}),
        ),
        "layerProperties" => {
            let id = d.fields.get("id").and_then(Value::as_u64).unwrap_or(0);
            let name = d.s("name");
            let visible = d.b("visible");
            let locked = d.b("locked");
            let printable = d.b("printable");
            let mut params = json!({
                "id": id,
                "visible": visible,
                "locked": locked,
                "printable": printable,
            });
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                params["name"] = json!(trimmed);
            }
            app.run("layer.set", params)
        }
        "renameObject" => {
            let id = d.fields.get("id").and_then(Value::as_u64).unwrap_or(0);
            let name = d.s("name");
            app.run("object.rename", json!({"id": id, "name": name.trim()}))
        }
        "scriptLabel" => {
            let ids = d.fields.get("ids").cloned().unwrap_or(json!([]));
            let label = d.s("label");
            app.run("object.label", json!({"label": label.trim(), "ids": ids}))
        }
        "documentSetup" => {
            let edges = |k: &str| json!(["Top", "Bottom", "Inside", "Outside"].map(|e| d.m(&format!("{k}{e}")).unwrap_or(0.0)));
            app.run(
                "layout.documentSetup",
                json!({"intent": d.s("intent"), "pages": d.n("pages").unwrap_or(1.0).max(1.0) as u64, "startPage": d.n("startPage").unwrap_or(1.0).max(1.0) as u64,
                    "facingPages": d.b("facingPages"), "binding": d.s("binding"), "width": d.m("width"), "height": d.m("height"), "bleed": edges("bleed"), "slug": edges("slug"), "adjustLayout": d.b("adjustLayout")}),
            )
        }
        "paragraphStyleOptions" => {
            let name = d.s("name");
            let mut para = serde_json::Map::new();
            let mut chars = serde_json::Map::new();
            for (k, v) in &d.fields {
                if let Some(a) = k.strip_prefix("p.") {
                    para.insert(a.into(), v.clone());
                } else if let Some(a) = k.strip_prefix("c.") {
                    chars.insert(a.into(), v.clone());
                }
            }
            let mut params = json!({"name": name, "para": para, "chars": chars});
            if d.b("x.cf_init") {
                let value=copyfit_value(&d);
                if d.fields.get("x.cf_initial") != Some(&value) { params["copyfit"]=value; }
            }
            let based = d.s("basedOn");
            if !based.is_empty() {
                params["basedOn"] = if based == "[No Paragraph Style]" { Value::Null } else { json!(based) };
            }
            let rename = d.s("rename");
            if !rename.is_empty() && rename != name {
                params["rename"] = json!(rename);
            }
            let next = d.s("nextStyle");
            if !next.is_empty() {
                params["nextStyle"] = if next == "[Same Style]" { Value::Null } else { json!(next) };
            }
            if d.fields.contains_key("shortcut") {
                params["shortcut"] = json!(d.s("shortcut"));
            }
            let r = app.run("style.paragraph.edit", params)?;
            if d.fields.contains_key("x.tag") {
                let tag = d.s("x.tag");
                let tag = if tag == "[Automatic]" { String::new() } else { tag };
                let style = if !rename.is_empty() && rename != name { rename } else { name };
                app.run("style.exportTag", json!({"style": style, "tag": tag, "class": d.s("x.class")}))?;
            }
            Ok(r)
        }
        "characterStyleOptions" => {
            let name = d.s("name");
            let mut chars = serde_json::Map::new();
            for (k, v) in &d.fields {
                if let Some(a) = k.strip_prefix("c.") {
                    chars.insert(a.into(), v.clone());
                }
            }
            let mut params = json!({"name": name, "chars": chars});
            if d.b("x.cf_init") {
                let value=copyfit_value(&d);
                if d.fields.get("x.cf_initial") != Some(&value) { params["copyfit"]=value; }
            }
            let based = d.s("basedOn");
            if !based.is_empty() {
                params["basedOn"] = if based == designcraft_doc::NO_CHAR_STYLE { Value::Null } else { json!(based) };
            }
            let rename = d.s("rename");
            if !rename.is_empty() && rename != name {
                params["rename"] = json!(rename);
            }
            if d.fields.contains_key("shortcut") {
                params["shortcut"] = json!(d.s("shortcut"));
            }
            let r = app.run("style.character.edit", params)?;
            if d.fields.contains_key("x.tag") {
                let tag = d.s("x.tag");
                let tag = if tag == "[Automatic]" { String::new() } else { tag };
                let style = if !rename.is_empty() && rename != name { rename } else { name };
                app.run("style.exportTag", json!({"style": style, "tag": tag, "class": d.s("x.class"), "character": true}))?;
            }
            Ok(r)
        }
        "colorPicker" => {
            let hex = d.s("hex");
            app.run("object.color", json!({"color": hex, "target": d.s("target")}))
        }
        "layerOptions" => {
            let hidden: Vec<Value> = d.fields.get("layers").and_then(Value::as_array).map(|a| a.iter().filter(|l| l["visible"] == false).map(|l| l["name"].clone()).collect()).unwrap_or_default();
            app.run("object.layerOptions", json!({"hidden": hidden}))
        }
        "colorSettings" => {
            let r = app.run("color.settings", json!({"rgb": d.s("rgb"), "cmyk": d.s("cmyk"), "intent": d.s("intent"), "bpc": d.b("bpc")}))?;
            app.ui.color_settings = Some(designcraft_color::cms::active_settings());
            app.canvas.shown = None;
            Ok(r)
        }
        "pdfImport" => {
            let crop = d.s("crop");
            let mut p = json!({"path": d.s("path"), "pdfPage": d.n("page").unwrap_or(1.0).max(1.0) as u64, "pdfCrop": if crop.is_empty() { "crop".to_string() } else { crop }});
            if let Some(f) = d.fields.get("frame") {
                p["frame"] = f.clone();
            } else if let Some(fid) = app.pending_place_frame.take() {
                p["frame"] = json!(fid.0);
            }
            app.run("file.place", p)
        }
        "print" => {
            let pages = if d.s("range") == "all" { Value::Null } else { json!(d.s("pages")) };
            let printer = d.s("printer");
            let r = app.run(
                "file.print",
                json!({"printer": if printer.is_empty() { Value::Null } else { json!(printer) }, "copies": d.n("copies").unwrap_or(1.0).max(1.0) as u64,
                    "pages": pages, "spreads": d.b("spreads"), "marks": d.b("marks"), "bleed": d.b("bleed")}),
            )?;
            app.status(format!("Sent {} page(s) to {}", r["pages"], r["printer"].as_str().unwrap_or("the default printer")));
            Ok(r)
        }
        "exportPdf" => {
            let mut params = json!({
                "spreads": d.b("spreads"),
                "bleed": d.b("bleed"),
                "marks": d.b("marks"),
                "tagged": d.b("tagged"),
                "compressImages": d.b("compressImages"),
            });
            if d.s("range") == "range" && !d.s("pages").is_empty() {
                params["pages"] = json!(d.s("pages"));
            }
            let std = d.s("standard");
            if std != "none" && !std.is_empty() {
                params["standard"] = json!(std);
            }
            let preset = d.s("preset");
            if preset == "pressQuality" || preset == "pdfx1a" || preset == "pdfx3" {
                params["flatten"] = json!("high");
            } else if preset == "smallestFile" {
                params["flatten"] = json!("low");
            }
            let mut filename = d.s("filename");
            if filename.trim().is_empty() {
                filename = app.session.active().map(|s| format!("{}.pdf", s.doc.title)).unwrap_or_else(|| "document.pdf".into());
            } else if !filename.to_ascii_lowercase().ends_with(".pdf") {
                filename = format!("{filename}.pdf");
            }
            crate::menus::export_pdf_direct(app, &params, &filename)
        }
        "oversetWarning" => {
            let title = app.session.active().map(|s| s.doc.title.clone()).unwrap_or_else(|| "documento".into());
            let filename = format!("{title}-prensa.pdf");
            let params = json!({
                "bleed": true,
                "marks": true,
                "standard": "x1a",
                "tagged": true,
                "compressImages": true,
                "flatten": "high"
            });
            crate::menus::export_pdf_direct(app, &params, &filename)

        }
        "editorialOverview" => Ok(Value::Null),
        "exportPng" => {
            let page = d.n("page").unwrap_or(1.0).max(1.0) as usize - 1;
            let scale = d.n("scale").unwrap_or(2.0);
            let mut filename = d.s("filename");
            if filename.trim().is_empty() {
                filename = app.session.active().map(|s| format!("{}-p{}.png", s.doc.title, page + 1)).unwrap_or_else(|| "page.png".into());
            } else if !filename.to_ascii_lowercase().ends_with(".png") {
                filename = format!("{filename}.png");
            }
            let rich_black = d.b("richBlack");
            crate::menus::export_png_direct(app, page, scale, rich_black, &filename)
        }
        "saveAs" => {
            let name = d.s("name").trim().to_string();
            let name = if name.is_empty() { "Untitled".to_string() } else { name };
            let format = d.s("format");
            let target = d.s("target");
            crate::menus::save_as_direct(app, &name, &format, &target)
        }
        "saveCopy" => {
            let name = d.s("name").trim().to_string();
            let name = if name.is_empty() { "Untitled copy".to_string() } else { name };
            let format = d.s("format");
            let target = d.s("target");
            crate::menus::save_copy_direct(app, &name, &format, &target)
        }
        "preferences" => {
            app.run(
                "prefs.set",
                json!({"scaleStrokes": d.b("scaleStrokes"), "dimensionsIncludeStroke": d.b("dimensionsIncludeStroke"), "transformationsAreTotals": d.b("transformationsAreTotals"), "absolutePageNumbers": d.b("absolutePageNumbers"), "highlightHj": d.b("highlightHj"), "highlightKeeps": d.b("highlightKeeps"), "highlightCustomTracking": d.b("highlightCustomTracking"), "highlightSubstitutedFonts": d.b("highlightSubstitutedFonts"), "richBlackOutput": d.b("richBlackOutput"), "typographersQuotes": d.b("typographersQuotes"), "showFontNamesInEnglish": d.b("showFontNamesInEnglish"), "smartTextReflow": d.b("smartTextReflow"),
                    "autocorrect": d.b("autocorrect"), "showAddedText": d.b("showAddedText"), "showNoteAnchors": d.b("showNoteAnchors"),
                    "recoveryMinutes": d.n("recoveryMinutes").unwrap_or(0.5),
                    "autocorrectList": d.s("autocorrectText").lines().filter_map(|l| {
                        let (a, b) = l.split_once('→').or_else(|| l.split_once("->"))?;
                        let (a, b) = (a.trim().to_lowercase(), b.trim().to_string());
                        (!a.is_empty() && !b.is_empty()).then(|| json!([a, b]))
                    }).collect::<Vec<_>>()}),
            )?;
            app.ui.dynamic_spelling = d.b("dynamicSpelling");
            if d.fields.contains_key("userWords") {
                let words: Vec<String> = d.s("userWords").lines().map(str::to_string).collect();
                app.run("spelling.setWords", json!({"words": words}))?;
            }
            app.ui.story_editor_size = d.n("storyEditorSize").unwrap_or(14.0) as f32;
            // Highlight options change the screen view only.
            app.canvas.shown = None;
            let view = match d.s("displayQuality").as_str() {
                "fast" => "view.fastDisplay",
                "typical" => "view.typicalDisplay",
                _ => "view.highQualityDisplay",
            };
            app.run(view, json!({}))?;
            app.run("window.richBlack", json!({"on": d.b("richBlack")}))?;
            if let Some(v) = d.n("uiScale") {
                app.run("window.uiScale", json!({"scale": v / 100.0}))?;
            }
            if d.fields.contains_key("snap.alignEdges") {
                let mut snap = json!({
                    "alignEdges": d.b("snap.alignEdges"),
                    "alignCenters": d.b("snap.alignCenters"),
                    "smartDimensions": d.b("snap.dimensions"),
                    "smartSpacing": d.b("snap.spacing"),
                });
                if let Some(zone) = d.n("snap.zone") {
                    snap["zone"] = json!(zone);
                }
                app.run("view.snapPreferences", snap)?;
            }
            if !d.fields.contains_key("horizontalUnits") {
                return Ok(Value::Null);
            }
            let mut doc = json!({"horizontalUnits": d.fields["horizontalUnits"], "verticalUnits": d.fields["verticalUnits"]});
            let mut bg = json!({"relativeTo": d.s("bg.relativeTo"), "color": d.fields.get("bg.color").cloned().unwrap_or(json!([140, 205, 230]))});
            if let Some(v) = d.m("bg.start") {
                bg["start"] = json!(v.max(0.0));
            }
            if let Some(v) = d.m("bg.increment").filter(|v| *v > 0.0) {
                bg["increment"] = json!(v);
            }
            if let Some(v) = d.n("bg.viewThreshold") {
                bg["viewThreshold"] = json!((v / 100.0).clamp(0.05, 40.0));
            }
            doc["baselineGrid"] = bg;
            let mut grid = json!({"inBack": d.b("grid.inBack"), "color": d.fields.get("grid.color").cloned().unwrap_or(json!([200, 200, 200]))});
            for k in ["horizontal", "vertical"] {
                if let Some(v) = d.m(&format!("grid.{k}")).filter(|v| *v > 0.0) {
                    grid[k] = json!(v);
                }
            }
            if let Some(v) = d.n("grid.subdivisions") {
                grid["subdivisions"] = json!(v.clamp(1.0, 100.0) as u32);
            }
            doc["grid"] = grid;
            for k in ["marginColor", "columnColor", "bleedColor", "slugColor"] {
                if let Some(v) = d.fields.get(k) {
                    doc[k] = v.clone();
                }
            }
            if let (Some(h), Some(v)) = (d.m("pasteboard.h"), d.m("pasteboard.v")) {
                doc["pasteboard"] = json!([h.max(0.0), v.max(0.0)]);
            }
            if let Some(v) = d.fields.get("keyboardIncrement").and_then(Value::as_str).and_then(|s| parse_measure(s, Unit::Points).ok()) {
                doc["keyboardIncrement"] = json!(v.max(0.001));
            }
            let mut adv = json!({});
            for k in ["superscriptSize", "superscriptPosition", "subscriptSize", "subscriptPosition"] {
                if let Some(v) = d.n(&format!("adv.{k}")) {
                    adv[k] = json!(if k.ends_with("Size") { v.clamp(1.0, 200.0) } else { v.clamp(-500.0, 500.0) });
                }
            }
            doc["advancedType"] = adv;
            doc["overprintBlack"] = json!(d.b("overprintBlack"));
            doc["glyphFallback"] = json!(d.b("glyphFallback"));
            app.run("document.preferences", doc)
        }
        "newWorkspace" => app.run("window.newWorkspace", json!({"name": d.s("name")})),
        "importOptions" => app.run(
            "file.place",
            json!({"path": d.s("path"), "removeStyles": d.b("removeStyles"), "styleConflicts": d.s("styleConflicts"), "styleMap": d.fields.get("map").cloned().unwrap_or(json!({}))}),
        ),
        "fittingOptions" => {
            let m = |k: &str| d.fields.get(k).and_then(Value::as_str).and_then(|s| parse_measure(s, Unit::Points).ok()).unwrap_or(0.0);
            app.run(
                "object.fittingOptions",
                json!({"autoFit": d.b("autoFit"), "fitting": d.s("fitting"), "align": d.n("align").unwrap_or(4.0) as u64,
                    "crop": [m("cropTop"), m("cropLeft"), m("cropBottom"), m("cropRight")]}),
            )
        }
        "qrCode" => {
            let mut p = Value::Object(d.fields.clone());
            // Nothing selected: a 2-inch code at the top left of the page in view.
            if let Some(st) = app.session.active().filter(|s| s.selection.items.is_empty()) {
                let abs = crate::canvas::current_page(app).unwrap_or(0);
                let (si, pi) = st.doc.page_loc(abs).unwrap_or((0, 0));
                let x = st.doc.spreads.get(si).and_then(|sp| sp.pages.get(pi)).map_or(0.0, |pg| pg.x);
                p["rect"] = json!([x + 36.0, 36.0, x + 180.0, 180.0]);
                p["spread"] = json!(si);
            }
            app.run("object.qrCode", p)
        }
        "polygonSettings" => app.run("tool.polygonSettings", json!({"sides": d.n("sides").unwrap_or(6.0) as u64, "starInset": d.n("starInset").unwrap_or(0.0)})),
        "findFont" => {
            let (f, st) = (d.s("family"), d.s("style"));
            if f.is_empty() || d.s("toFamily").is_empty() {
                return Ok(Value::Null);
            }
            app.run("font.replace", json!({"family": f, "style": st, "toFamily": d.s("toFamily"), "toStyle": d.s("toStyle")}))
        }
        "insertXref" => {
            let format = d.s("format");
            let target = d.s("target");
            let params = match target.split_once(':') {
                Some(("a", id)) => json!({"anchor": id.parse::<u64>().unwrap_or(0), "format": format}),
                Some((sid, pi)) => json!({"story": sid.parse::<u64>().unwrap_or(0), "para": pi.parse::<u64>().unwrap_or(0), "format": format}),
                None => {
                    let mut d = d.clone();
                    d.fields.insert("status".into(), json!("Choose a destination paragraph or text anchor."));
                    app.ui.dialog = Some(d);
                    return Err("no destination chosen".into());
                }
            };
            app.run("xref.insert", params)
        }
        "footnoteOptions" => {
            let pt = |k: &str| d.pt(k).map_or(Value::Null, |v| json!(v));
            let mut p = json!({
                "style": d.s("style"), "startAt": d.n("startAt").unwrap_or(1.0).max(0.0) as u32, "restart": d.s("restart"),
                "prefix": d.s("prefix"), "suffix": d.s("suffix"), "affixIn": d.s("affixIn"), "refPosition": d.s("refPosition"),
                "refCharStyle": d.s("refCharStyle"), "paraStyle": d.s("paraStyle"),
                "separator": d.s("separator").replace("^t", "\t").replace("^m", "\u{2003}").replace("^>", "\u{2002}"),
                "spaceBefore": pt("spaceBefore"), "spaceBetween": pt("spaceBetween"), "firstBaseline": d.s("firstBaseline"),
                "firstBaselineMin": pt("firstBaselineMin"), "spanColumns": d.b("spanColumns"),
                "rule": {"on": d.b("rule.on"), "weight": pt("rule.weight"), "color": d.s("rule.color"), "width": pt("rule.width"),
                    "offset": pt("rule.offset"), "leftIndent": pt("rule.leftIndent")},
            });
            // Unparsable measures keep their current values.
            if let Some(o) = p.as_object_mut() {
                o.retain(|_, v| !v.is_null());
            }
            if let Some(r) = p.get_mut("rule").and_then(Value::as_object_mut) {
                r.retain(|_, v| !v.is_null());
            }
            app.run("footnote.options", p)
        }
        "findChange" if d.b("objectMode") => Ok(Value::Null),
        "findChange" => app.run("find.change", json!({"find": d.s("find"), "change": d.s("change"), "grep": d.b("grep"), "caseSensitive": d.b("caseSensitive"), "wholeWord": d.b("wholeWord"), "scope": d.s("scope")})),
        id if id.starts_with("cmd:") => {
            let cid = &id[4..];
            let doc = designcraft_engine::find_command(cid).map_or("", |c| c.params);
            let mut p = Map::new();
            for f in command_fields(doc) {
                match d.fields.get(&f.key) {
                    Some(Value::String(v)) if !v.trim().is_empty() => {
                        let v = v.trim();
                        // Numbers, booleans, arrays and objects as JSON; anything else is a string.
                        let parsed = serde_json::from_str::<Value>(v).ok().filter(|x| !x.is_string());
                        p.insert(f.key, parsed.unwrap_or_else(|| json!(v)));
                    }
                    Some(Value::Bool(b)) => {
                        p.insert(f.key, json!(b));
                    }
                    _ => {}
                }
            }
            let r = app.run(cid, Value::Object(p));
            if let Err(e) = &r {
                // Keep the dialog open with the error.
                let mut d = d.clone();
                d.fields.insert("status".into(), json!(e));
                app.ui.dialog = Some(d);
            }
            r
        }
        other => Err(format!("unknown dialog {other}")),
    }
}

/// Paragraph Style Options: sections in a left list (like InDesign), fields on the right.
/// Edited values are stored as `p.<attr>` / `c.<attr>` fields and applied on OK.
/// Is `p` a pattern the GREP styles can use? (Checked as the user types.)
fn regex_ok(p: &str) -> Result<(), ()> {
    if p.is_empty() {
        return Ok(());
    }
    regex::Regex::new(p).map(|_| ()).map_err(|_| ())
}

fn char_formatting_sections(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, cv: &Value, prefix: &str, _swatches: &[String]) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let fonts = crate::panels::fonts(app);
    let menu = crate::panels::font_menu(app);

    egui::Grid::new(format!("{prefix}_bsc")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Font Family:"));
        let fam = cur(d, "c.fontFamily", &cv["fontFamily"]).as_str().unwrap_or("").to_string();
        egui::ComboBox::from_id_salt(format!("{prefix}_fam")).selected_text(crate::panels::font_label(app, &menu, &fam)).width(200.0).show_ui(
            ui,
            |ui| {
                if let Some(f) = crate::panels::font_menu_rows(app, ui, &menu, &fam) {
                    d.fields.insert("c.fontFamily".into(), json!(f));
                }
            },
        );
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Font Style:"));
        let sty = cur(d, "c.fontStyle", &cv["fontStyle"]).as_str().unwrap_or("").to_string();
        egui::ComboBox::from_id_salt(format!("{prefix}_sty")).selected_text(&sty).width(200.0).show_ui(ui, |ui| {
            for s in fonts.styles(&fam) {
                if ui.selectable_label(s == sty, &s).clicked() {
                    d.fields.insert("c.fontStyle".into(), json!(s));
                }
            }
        });
        ui.end_row();

        for (label, key, suffix) in [("Size:", "size", " pt"), ("Tracking:", "tracking", "")] {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
            let v = cur(d, &format!("c.{key}"), &cv[key]).as_f64();
            if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_{key}"), v, suffix, 80.0, 2) {
                d.fields.insert(format!("c.{key}"), json!(n));
            }
            ui.end_row();
        }

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Leading:"));
        let cur_lead = cur(d, "c.leading", &cv["leading"]);
        let is_auto = cur_lead["kind"].as_str() != Some("points");
        let lv = if is_auto { None } else { cur_lead["value"].as_f64() };
        ui.horizontal(|ui| {
            if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_lead"), lv, " pt", 70.0, 2) {
                d.fields.insert("c.leading".into(), json!({"kind": "points", "value": n}));
            }
            if ui.selectable_label(is_auto, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Auto"))).clicked() {
                d.fields.insert("c.leading".into(), json!({"kind": "auto"}));
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Kerning:"));
        let cur_kern = cur(d, "c.kerning", &cv["kerning"]);
        let kern_kind = cur_kern.get("kind").and_then(Value::as_str).unwrap_or("metrics");
        let kern_label = match kern_kind {
            "optical" => crate::i18n::tr(&app.ui.language, "Optical"),
            "none" => "0",
            _ => crate::i18n::tr(&app.ui.language, "Metrics"),
        };
        egui::ComboBox::from_id_salt(format!("{prefix}_kerning")).selected_text(crate::rtl::widget(ui, kern_label)).width(160.0).show_ui(ui, |ui| {
            for (k, lbl) in
                [("metrics", crate::i18n::tr(&app.ui.language, "Metrics")), ("optical", crate::i18n::tr(&app.ui.language, "Optical")), ("none", "0")]
            {
                if ui.selectable_label(kern_kind == k, crate::rtl::widget(ui, lbl)).clicked() {
                    d.fields.insert("c.kerning".into(), json!({"kind": k}));
                }
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Case:"));
        let caps = cur(d, "c.capitalization", &cv["capitalization"]).as_str().unwrap_or("normal").to_string();
        egui::ComboBox::from_id_salt(format!("{prefix}_caps"))
            .selected_text(crate::rtl::widget(
                ui,
                match caps.as_str() {
                    "allCaps" => crate::i18n::tr(&app.ui.language, "All Caps"),
                    "smallCaps" => crate::i18n::tr(&app.ui.language, "Small Caps"),
                    "openTypeAllSmallCaps" => crate::i18n::tr(&app.ui.language, "OpenType All Small Caps"),
                    _ => crate::i18n::tr(&app.ui.language, "Normal"),
                },
            ))
            .width(160.0)
            .show_ui(ui, |ui| {
                for (v, lbl) in
                    [("normal", "Normal"), ("allCaps", "All Caps"), ("smallCaps", "Small Caps"), ("openTypeAllSmallCaps", "OpenType All Small Caps")]
                {
                    if ui.selectable_label(caps == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                        d.fields.insert("c.capitalization".into(), json!(v));
                    }
                }
            });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Position:"));
        let pos = cur(d, "c.position", &cv["position"]).as_str().unwrap_or("normal").to_string();
        egui::ComboBox::from_id_salt(format!("{prefix}_pos"))
            .selected_text(crate::rtl::widget(
                ui,
                match pos.as_str() {
                    "superscript" => crate::i18n::tr(&app.ui.language, "Superscript"),
                    "subscript" => crate::i18n::tr(&app.ui.language, "Subscript"),
                    _ => crate::i18n::tr(&app.ui.language, "Normal"),
                },
            ))
            .width(160.0)
            .show_ui(ui, |ui| {
                for (v, lbl) in [("normal", "Normal"), ("superscript", "Superscript"), ("subscript", "Subscript")] {
                    if ui.selectable_label(pos == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                        d.fields.insert("c.position".into(), json!(v));
                    }
                }
            });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Digits"));
        let digits = cur(d, "c.digits", &cv["digits"]).as_str().unwrap_or("default").to_string();
        let opts = [("default", "Default"), ("arabic", "Arabic"), ("hindi", "Hindi"), ("farsi", "Farsi"), ("native", "Native")];
        let cur_dig = opts.iter().find(|o| o.0 == digits).map_or("Default", |o| o.1);
        egui::ComboBox::from_id_salt(format!("{prefix}_digits"))
            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, cur_dig)))
            .width(160.0)
            .show_ui(ui, |ui| {
                for (k, l) in opts {
                    if ui.selectable_label(k == digits, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                        d.fields.insert("c.digits".into(), json!(k));
                    }
                }
            });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Language:"));
        let lang = cur(d, "c.language", &cv["language"]).as_str().unwrap_or("English: USA").to_string();
        let display = if lang.is_empty() { "[No Language]" } else { &lang };
        egui::ComboBox::from_id_salt(format!("{prefix}_char_language"))
            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, display)))
            .width(160.0)
            .show_ui(ui, |ui| {
                for l in crate::panels::properties::LANGUAGES {
                    if ui.selectable_label(*l == lang, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                        d.fields.insert("c.language".into(), json!(l));
                    }
                }
            });
        ui.end_row();
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (lbl, key) in [("Underline", "underline"), ("Strikethrough", "strikethrough"), ("Ligatures", "ligatures"), ("No Break", "noBreak")] {
            let mut b = cur(d, &format!("c.{key}"), &cv[key]).as_bool().unwrap_or(false);
            if ui.checkbox(&mut b, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).changed() {
                d.fields.insert(format!("c.{key}"), json!(b));
            }
        }
    });
}

fn adv_char_formatting_section(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, cv: &Value, prefix: &str) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    egui::Grid::new(format!("{prefix}_adv_grid")).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Horizontal Scale:"));
        let hs = cur(d, "c.hScale", &cv["hScale"]).as_f64().map(|x| x * 100.0).or(Some(100.0));
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_hscale"), hs, "%", 80.0, 0) {
            d.fields.insert("c.hScale".into(), json!(n / 100.0));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Vertical Scale:"));
        let vs = cur(d, "c.vScale", &cv["vScale"]).as_f64().map(|x| x * 100.0).or(Some(100.0));
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_vscale"), vs, "%", 80.0, 0) {
            d.fields.insert("c.vScale".into(), json!(n / 100.0));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Baseline Shift:"));
        let bs = cur(d, "c.baselineShift", &cv["baselineShift"]).as_f64().or(Some(0.0));
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_bshift"), bs, " pt", 80.0, 2) {
            d.fields.insert("c.baselineShift".into(), json!(n));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Skew:"));
        let sk = cur(d, "c.skew", &cv["skew"]).as_f64().or(Some(0.0));
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_skew"), sk, "°", 80.0, 1) {
            d.fields.insert("c.skew".into(), json!(n));
        }
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let mut kt = cur(d, "c.kenten", &cv["kenten"]).as_bool().unwrap_or(false);
        if ui.checkbox(&mut kt, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Kenten"))).changed() {
            d.fields.insert("c.kenten".into(), json!(kt));
        }
        let mut tcy = cur(d, "c.tateChuYoko", &cv["tateChuYoko"]).as_bool().unwrap_or(false);
        if ui.checkbox(&mut tcy, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Tate-Chu-Yoko"))).changed() {
            d.fields.insert("c.tateChuYoko".into(), json!(tcy));
        }
    });
}

fn underline_section(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, cv: &Value, prefix: &str, swatches: &[String]) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let mut u_on = cur(d, "c.underline", &cv["underline"]).as_bool().unwrap_or(false);
    if ui.checkbox(&mut u_on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Underline"))).changed() {
        d.fields.insert("c.underline".into(), json!(u_on));
    }
    ui.add_space(8.0);
    egui::Grid::new(format!("{prefix}_u_grid")).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
        let uw = cur(d, "c.underlineWeight", &cv["underlineWeight"]).as_f64();
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_uw"), uw, " pt", 80.0, 2) {
            d.fields.insert("c.underlineWeight".into(), json!(n.max(0.0)));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Offset:"));
        let uo = cur(d, "c.underlineOffset", &cv["underlineOffset"]).as_f64();
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_uo"), uo, " pt", 80.0, 2) {
            d.fields.insert("c.underlineOffset".into(), json!(n));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
        let cur_col = cur(d, "c.underlineColor", &cv["underlineColor"]).as_str().unwrap_or("").to_string();
        let disp_col = if cur_col.is_empty() { crate::i18n::tr(&app.ui.language, "(Text Color)") } else { &cur_col };
        egui::ComboBox::from_id_salt(format!("{prefix}_ucolor")).selected_text(crate::rtl::widget(ui, disp_col)).width(180.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_col.is_empty(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "(Text Color)"))).clicked() {
                d.fields.insert("c.underlineColor".into(), json!(""));
            }
            for sw in swatches {
                if ui.selectable_label(*sw == cur_col, sw).clicked() {
                    d.fields.insert("c.underlineColor".into(), json!(sw));
                }
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
        let mut ut = cur(d, "c.underlineTint", &cv["underlineTint"]).as_f64().unwrap_or(1.0) * 100.0;
        if ui.add(egui::Slider::new(&mut ut, 0.0..=100.0).suffix("%")).changed() {
            d.fields.insert("c.underlineTint".into(), json!((ut / 100.0) as f32));
        }
        ui.end_row();
    });
}

fn strikethrough_section(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, cv: &Value, prefix: &str, swatches: &[String]) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let mut st_on = cur(d, "c.strikethrough", &cv["strikethrough"]).as_bool().unwrap_or(false);
    if ui.checkbox(&mut st_on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Strikethrough"))).changed() {
        d.fields.insert("c.strikethrough".into(), json!(st_on));
    }
    ui.add_space(8.0);
    egui::Grid::new(format!("{prefix}_st_grid")).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
        let sw_val = cur(d, "c.strikethroughWeight", &cv["strikethroughWeight"]).as_f64();
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_stw"), sw_val, " pt", 80.0, 2) {
            d.fields.insert("c.strikethroughWeight".into(), json!(n.max(0.0)));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Offset:"));
        let so = cur(d, "c.strikethroughOffset", &cv["strikethroughOffset"]).as_f64();
        if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_sto"), so, " pt", 80.0, 2) {
            d.fields.insert("c.strikethroughOffset".into(), json!(n));
        }
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
        let cur_col = cur(d, "c.strikethroughColor", &cv["strikethroughColor"]).as_str().unwrap_or("").to_string();
        let disp_col = if cur_col.is_empty() { crate::i18n::tr(&app.ui.language, "(Text Color)") } else { &cur_col };
        egui::ComboBox::from_id_salt(format!("{prefix}_stcolor")).selected_text(crate::rtl::widget(ui, disp_col)).width(180.0).show_ui(ui, |ui| {
            if ui.selectable_label(cur_col.is_empty(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "(Text Color)"))).clicked() {
                d.fields.insert("c.strikethroughColor".into(), json!(""));
            }
            for sw in swatches {
                if ui.selectable_label(*sw == cur_col, sw).clicked() {
                    d.fields.insert("c.strikethroughColor".into(), json!(sw));
                }
            }
        });
        ui.end_row();

        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
        let mut st_tint = cur(d, "c.strikethroughTint", &cv["strikethroughTint"]).as_f64().unwrap_or(1.0) * 100.0;
        if ui.add(egui::Slider::new(&mut st_tint, 0.0..=100.0).suffix("%")).changed() {
            d.fields.insert("c.strikethroughTint".into(), json!((st_tint / 100.0) as f32));
        }
        ui.end_row();
    });
}

fn opentype_section(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, cv: &Value, prefix: &str) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let mut feats: Vec<String> = cur(d, "c.otfFeatures", &cv["otfFeatures"])
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut changed = false;

    ui.horizontal(|ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Figure Style:"));
        let cur_fig = designcraft_doc::otf::figures(&feats);
        let cur_fig_label = designcraft_doc::otf::FIGURES.iter().find(|f| f.0 == cur_fig).map_or("Default", |f| f.1);
        egui::ComboBox::from_id_salt(format!("{prefix}_fig"))
            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, cur_fig_label)))
            .width(180.0)
            .show_ui(ui, |ui| {
                for (id, label, ..) in designcraft_doc::otf::FIGURES {
                    if ui.selectable_label(cur_fig == *id, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                        designcraft_doc::otf::set_figures(&mut feats, id);
                        changed = true;
                    }
                }
            });
    });

    ui.add_space(8.0);
    egui::Grid::new(format!("{prefix}_otf_grid")).num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (i, (tag, label, _)) in designcraft_doc::otf::TOGGLES.iter().enumerate() {
            let mut on = designcraft_doc::otf::is_on(&feats, tag);
            if ui.checkbox(&mut on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).changed() {
                designcraft_doc::otf::set(&mut feats, tag, on);
                changed = true;
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });

    ui.add_space(8.0);
    let mask = designcraft_doc::otf::stylistic_sets(&feats);
    egui::CollapsingHeader::new(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Stylistic Sets")))
        .id_salt(format!("{prefix}_ss_hdr"))
        .show(ui, |ui| {
            egui::Grid::new(format!("{prefix}_ss_grid")).num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
                for n in 1..=20u32 {
                    let mut on = mask & (1 << (n - 1)) != 0;
                    if ui.checkbox(&mut on, crate::rtl::widget(ui, format!("{} {n}", crate::i18n::tr(&app.ui.language, "Set")))).changed() {
                        let new_mask = if on { mask | (1 << (n - 1)) } else { mask & !(1 << (n - 1)) };
                        designcraft_doc::otf::set_stylistic_sets(&mut feats, new_mask);
                        changed = true;
                    }
                    if n % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
        });

    if changed {
        d.fields.insert("c.otfFeatures".into(), Value::Array(feats.into_iter().map(Value::String).collect()));
    }
}

fn char_color_section(
    app: &DesignApp,
    ui: &mut egui::Ui,
    d: &mut Dialog,
    cv: &Value,
    prefix: &str,
    swatches: &[String],
    doc: &designcraft_doc::Document,
) {
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let mode_key = format!("x.{prefix}_color_stroke_mode");
    let is_stroke = d.b(&mode_key);

    ui.horizontal(|ui| {
        if ui.selectable_label(!is_stroke, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Fill"))).clicked() {
            d.fields.insert(mode_key.clone(), json!(false));
        }
        if ui.selectable_label(is_stroke, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Stroke"))).clicked() {
            d.fields.insert(mode_key, json!(true));
        }
    });
    ui.separator();

    if !is_stroke {
        ui.horizontal(|ui| {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
            let mut tint = cur(d, "c.fillTint", &cv["fillTint"]).as_f64().unwrap_or(1.0) * 100.0;
            if ui.add(egui::Slider::new(&mut tint, 0.0..=100.0).suffix("%")).changed() {
                d.fields.insert("c.fillTint".into(), json!(tint / 100.0));
            }
        });
        ui.add_space(4.0);
        let cur_fill = cur(d, "c.fill", &cv["fill"]).as_str().unwrap_or("").to_string();
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for sw in swatches {
                let (c, g) = crate::widgets::swatch_colors(doc, sw, 1.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    crate::widgets::paint_chip(ui.painter(), r, c, g);
                    if ui.selectable_label(*sw == cur_fill, sw).clicked() {
                        d.fields.insert("c.fill".into(), json!(sw));
                    }
                });
            }
        });
    } else {
        egui::Grid::new(format!("{prefix}_stroke_grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
            let sw = cur(d, "c.strokeWeight", &cv["strokeWeight"]).as_f64().or(Some(1.0));
            if let Some(n) = crate::widgets::number(ui, &format!("{prefix}_stk_weight"), sw, " pt", 70.0, 2) {
                d.fields.insert("c.strokeWeight".into(), json!(n.max(0.0)));
            }
            ui.end_row();

            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
            let mut tint = cur(d, "c.strokeTint", &cv["strokeTint"]).as_f64().unwrap_or(1.0) * 100.0;
            if ui.add(egui::Slider::new(&mut tint, 0.0..=100.0).suffix("%")).changed() {
                d.fields.insert("c.strokeTint".into(), json!(tint / 100.0));
            }
            ui.end_row();
        });
        ui.add_space(4.0);
        let cur_stroke = cur(d, "c.stroke", &cv["stroke"]).as_str().unwrap_or("[None]").to_string();
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for sw in swatches {
                let (c, g) = crate::widgets::swatch_colors(doc, sw, 1.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    crate::widgets::paint_chip(ui.painter(), r, c, g);
                    if ui.selectable_label(*sw == cur_stroke, sw).clicked() {
                        d.fields.insert("c.stroke".into(), json!(sw));
                    }
                });
            }
        });
    }
}

// Keep one form and one serialization path for both kinds of style.
fn copyfit_value(d: &Dialog) -> Value {
    let mut value = json!({"close_bottom":d.b("x.cf_bottom")});
    for (enabled, min, max, a, b) in [
        ("x.cf_size", "x.cf_sz_min", "x.cf_sz_max", "size_min", "size_max"),
        ("x.cf_trk", "x.cf_trk_min", "x.cf_trk_max", "trk_min", "trk_max"),
        ("x.cf_kern", "x.cf_kern_min", "x.cf_kern_max", "kern_min", "kern_max"),
        ("x.cf_lead", "x.cf_lead_min", "x.cf_lead_max", "leading_min", "leading_max"),
        ("x.cf_hscale", "x.cf_hscale_min", "x.cf_hscale_max", "hscale_min", "hscale_max"),
    ] {
        if d.b(enabled) {
            value[a] = json!(d.n(min));
            value[b] = json!(d.n(max));
        }
    }
    value
}

fn copyfit_options(
    ui: &mut egui::Ui,
    d: &mut Dialog,
    policy: Option<&designcraft_doc::CopyfitPolicy>,
    cp: &designcraft_doc::CharProps,
    language: &str,
) {
    if !d.b("x.cf_init") {
        d.fields.insert("x.cf_init".into(), json!(true));
        if let Some(policy) = policy {
            d.fields.insert("x.cf_bottom".into(), json!(policy.close_bottom));
            for (range, flag, min, max) in [
                (&policy.size_flexibility, "x.cf_size", "x.cf_sz_min", "x.cf_sz_max"),
                (&policy.tracking_flexibility, "x.cf_trk", "x.cf_trk_min", "x.cf_trk_max"),
                (&policy.kerning_allowance, "x.cf_kern", "x.cf_kern_min", "x.cf_kern_max"),
                (&policy.leading_flexibility, "x.cf_lead", "x.cf_lead_min", "x.cf_lead_max"),
                (&policy.h_scale_flexibility, "x.cf_hscale", "x.cf_hscale_min", "x.cf_hscale_max"),
            ] {
                if let Some(range) = range {
                    d.fields.insert(flag.into(), json!(true));
                    d.fields.insert(min.into(), json!(range.start));
                    d.fields.insert(max.into(), json!(range.end));
                }
            }
        }
        d.fields.entry("x.cf_sz_min").or_insert(json!(cp.size * 0.99));
        d.fields.entry("x.cf_sz_max").or_insert(json!(cp.size * 1.01));
        d.fields.entry("x.cf_trk_min").or_insert(json!(cp.tracking - 5.0));
        d.fields.entry("x.cf_trk_max").or_insert(json!(cp.tracking + 5.0));
        d.fields.entry("x.cf_kern_min").or_insert(json!(-20.0));
        d.fields.entry("x.cf_kern_max").or_insert(json!(20.0));
        let lead_val = match cp.leading {
            designcraft_doc::attrs::Leading::Points(p) => p,
            _ => cp.size * 1.2,
        };
        d.fields.entry("x.cf_lead_min").or_insert(json!((lead_val * 0.95 * 100.0).round() / 100.0));
        d.fields.entry("x.cf_lead_max").or_insert(json!((lead_val * 1.05 * 100.0).round() / 100.0));
        d.fields.entry("x.cf_hscale_min").or_insert(json!(0.98));
        d.fields.entry("x.cf_hscale_max").or_insert(json!(1.02));
        let initial = copyfit_value(d);
        d.fields.insert("x.cf_initial".into(), initial);
    }
    let mut close = d.b("x.cf_bottom");
    if ui.checkbox(&mut close, crate::i18n::tr(language, "Close small bottom gap")).changed() {
        d.fields.insert("x.cf_bottom".into(), json!(close));
    }
    for (flag, title, min, max, suffix, step, range) in [
        ("x.cf_size", "Size Flexibility:", "x.cf_sz_min", "x.cf_sz_max", " pt", 0.01, (0.1, 1296.0)),
        ("x.cf_trk", "Tracking Flexibility:", "x.cf_trk_min", "x.cf_trk_max", "", 0.1, (-1000.0, 10000.0)),
        ("x.cf_kern", "Kerning Allowance:", "x.cf_kern_min", "x.cf_kern_max", "", 0.1, (-1000.0, 1000.0)),
        ("x.cf_lead", "Leading Flexibility:", "x.cf_lead_min", "x.cf_lead_max", " pt", 0.01, (0.1, 1296.0)),
        ("x.cf_hscale", "Horizontal Scale Flexibility:", "x.cf_hscale_min", "x.cf_hscale_max", "", 0.01, (0.5, 2.0)),
    ] {
        let mut enabled = d.b(flag);
        if ui.checkbox(&mut enabled, crate::i18n::tr(language, title)).changed() {
            d.fields.insert(flag.into(), json!(enabled));
        }
        if enabled {
            ui.horizontal(|ui| {
                for (key, label) in [(min, "Min"), (max, "Max")] {
                    ui.label(crate::i18n::tr(language, label));
                    if let Some(value) =
                        crate::widgets::NumField::number(key, d.n(key), suffix, 2).width(108.0).spinner().step(step).range(range.0, range.1).show(ui)
                    {
                        d.fields.insert(key.into(), json!(value));
                    }
                }
            });
        }
    }
    ui.add_space(8.0);
    ui.label(crate::i18n::tr(language, "Automatic microfit also limits size to 1% and tracking to 5 units per attempt"));
}

fn paragraph_style_options(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let name = d.s("name");
    let (style, names, all_para_names, pp, cp, units, swatches) = {
        let Some(st) = app.session.active() else { return };
        let Some(style) = st.doc.styles.para(&name).cloned() else {
            ui.label(format!("No style named {name}"));
            return;
        };
        let names: Vec<String> = st.doc.styles.paragraph.iter().map(|p| p.name.clone()).filter(|n| *n != name).collect();
        let all_para_names: Vec<String> = st.doc.styles.paragraph.iter().map(|p| p.name.clone()).collect();
        let (pp, cp) = st.doc.styles.resolve_para_style(&name);
        let units = st.doc.settings.horizontal_units;
        let swatches: Vec<String> = st.doc.swatches.iter().map(|s| s.name.clone()).collect();
        (style, names, all_para_names, pp, cp, units, swatches)
    };
    let pv = serde_json::to_value(&pp).unwrap_or_default();
    let cv = serde_json::to_value(&cp).unwrap_or_default();
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let cur_sec = d.s("section");
    let cur_sec = if cur_sec.is_empty() { "general" } else { cur_sec.as_str() };
    let sections = [
        ("general", "General"),
        ("chars", "Basic Character Formats"),
        ("adv_chars", "Advanced Character Formats"),
        ("indents", "Indents and Spacing"),
        ("span", "Span Columns"),
        ("keep", "Keep Options"),
        ("rules", "Paragraph Rules"),
        ("shading", "Paragraph Shading"),
        ("border", "Paragraph Border"),
        ("bullets", "Bullets and Numbering"),
        ("hyph", "Hyphenation"),
        ("justify", "Justification"),
        ("nested", "Drop Caps and Nested Styles"),
        ("grep", "GREP Style"),
        ("color", "Character Color"),
        ("opentype", "OpenType Features"),
        ("underline", "Underline Options"),
        ("strikethrough", "Strikethrough Options"),
        ("export", "Export Tagging"),
        ("copyfit", "Copyfitting"),
    ];

    ui.set_min_width(740.0);
    ui.horizontal_top(|ui| {
        ui.set_min_height(480.0);
        ui.set_max_height(480.0);
        ui.vertical(|ui| {
            ui.set_width(190.0);
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Categories:")).font(semibold(12.0)));
            ui.add_space(4.0);
            egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(2, 4)).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("ps_sidebar_scroll").max_height(440.0).show(ui, |ui| {
                    ui.set_width(180.0);
                    for (id, label) in sections {
                        let is_sel = cur_sec == id;
                        if ui.selectable_label(is_sel, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                            d.fields.insert("section".into(), json!(id));
                        }
                    }
                });
            });
        });
        ui.separator();
        ui.add_space(6.0);
        let cnames: Vec<String> = app.session.active().map(|s| s.doc.styles.character.iter().map(|c| c.name.clone()).collect()).unwrap_or_default();
        ui.vertical(|ui| {
            ui.set_min_width(520.0);
            let sec_title = sections.iter().find(|(id, _)| *id == cur_sec).map(|(_, l)| *l).unwrap_or("General");
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, sec_title)).font(semibold(14.0)));
            ui.separator();
            ui.add_space(8.0);
            egui::ScrollArea::vertical().id_salt("ps_content_scroll").max_height(430.0).show(ui, |ui| match cur_sec {
                "copyfit" => {
                    let policy = app.session.active().and_then(|st| st.doc.styles.resolve_copyfit(&name, designcraft_doc::NO_CHAR_STYLE));
                    if style.copyfit.is_none() && policy.is_some() {
                        ui.label(crate::i18n::tr(&app.ui.language, "Inherited copyfit limits; editing creates a local override"));
                    }
                    copyfit_options(ui, d, policy, &cp, &app.ui.language);
                }
                "export" => {
                    let et = app.session.active().and_then(|s| s.doc.styles.export_tag(&name, false).cloned()).unwrap_or_default();
                    if !d.fields.contains_key("x.tag") {
                        d.fields.insert("x.tag".into(), json!(if et.tag.is_empty() { "[Automatic]" } else { et.tag.as_str() }));
                        d.fields.insert("x.class".into(), json!(et.class));
                    }
                    egui::Grid::new("ps_exp_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tag:"));
                        let cur = d.s("x.tag");
                        egui::ComboBox::from_id_salt("export_tag")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &cur)))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for t in
                                    ["[Automatic]", "p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "li", "figcaption", "aside", "div"]
                                {
                                    if ui.selectable_label(cur == t, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, t))).clicked() {
                                        d.fields.insert("x.tag".into(), json!(t));
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Class:"));
                        text_field(ui, d, "x.class", 180.0);
                        ui.end_row();
                    });
                }
                "nested" => {
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Drop Caps")).font(semibold(12.0)));
                    ui.add_space(4.0);
                    egui::Grid::new("ps_dc_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Lines:"));
                        let mut dcl = cur(d, "p.dropCapLines", &pv["dropCapLines"]).as_u64().unwrap_or(0) as u32;
                        if ui.add(egui::DragValue::new(&mut dcl).range(0..=99)).changed() {
                            d.fields.insert("p.dropCapLines".into(), json!(dcl));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Characters:"));
                        let mut dcc = cur(d, "p.dropCapChars", &pv["dropCapChars"]).as_u64().unwrap_or(0) as u32;
                        if ui.add(egui::DragValue::new(&mut dcc).range(0..=99)).changed() {
                            d.fields.insert("p.dropCapChars".into(), json!(dcc));
                        }
                        ui.end_row();
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Nested Styles")).font(semibold(12.0)));
                    let mut list: Vec<Value> = cur(d, "p.nestedStyles", &pv["nestedStyles"]).as_array().cloned().unwrap_or_default();
                    let mut changed = false;
                    let mut remove = None;
                    for (i, ns) in list.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let style = ns["style"].as_str().unwrap_or("[None]").to_string();
                            egui::ComboBox::from_id_salt(("ns_style", i))
                                .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &style)))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for c in &cnames {
                                        if ui
                                            .selectable_label(*c == style, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, c)))
                                            .clicked()
                                        {
                                            ns["style"] = json!(c);
                                            changed = true;
                                        }
                                    }
                                });
                            let through = ns["through"].as_bool().unwrap_or(true);
                            egui::ComboBox::from_id_salt(("ns_thr", i))
                                .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, if through { "through" } else { "up to" })))
                                .width(70.0)
                                .show_ui(ui, |ui| {
                                    for (v, l) in [(true, "through"), (false, "up to")] {
                                        if ui.selectable_label(v == through, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                            ns["through"] = json!(v);
                                            changed = true;
                                        }
                                    }
                                });
                            let mut n = ns["count"].as_u64().unwrap_or(1) as u32;
                            if ui.add(egui::DragValue::new(&mut n).range(1..=999)).changed() {
                                ns["count"] = json!(n);
                                changed = true;
                            }
                            let kind = ns["until"]["kind"].as_str().unwrap_or("words").to_string();
                            egui::ComboBox::from_id_salt(("ns_until", i))
                                .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &kind)))
                                .width(100.0)
                                .show_ui(ui, |ui| {
                                    for k in [
                                        "sentences",
                                        "words",
                                        "characters",
                                        "letters",
                                        "digits",
                                        "tab",
                                        "forcedLineBreak",
                                        "emSpace",
                                        "enSpace",
                                        "chars",
                                    ] {
                                        if ui.selectable_label(k == kind, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, k))).clicked() {
                                            ns["until"] = if k == "chars" { json!({"kind": "chars", "chars": ":"}) } else { json!({"kind": k}) };
                                            changed = true;
                                        }
                                    }
                                });
                            if kind == "chars" {
                                let mut c = ns["until"]["chars"].as_str().unwrap_or("").to_string();
                                if ui.add(egui::TextEdit::singleline(&mut c).desired_width(40.0)).changed() {
                                    ns["until"]["chars"] = json!(c);
                                    changed = true;
                                }
                            }
                            if ui
                                .small_button("×")
                                .on_hover_ui(|ui| {
                                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Delete"));
                                })
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        list.remove(i);
                        changed = true;
                    }
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "New Nested Style"))).clicked() {
                        list.push(
                            json!({"style": cnames.get(1).cloned().unwrap_or_default(), "through": true, "count": 1, "until": {"kind": "words"}}),
                        );
                        changed = true;
                    }
                    if changed {
                        d.fields.insert("p.nestedStyles".into(), Value::Array(list));
                    }
                    ui.add_space(8.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Nested Line Styles")).font(semibold(12.0)));
                    let mut lines: Vec<Value> = cur(d, "p.nestedLineStyles", &pv["nestedLineStyles"]).as_array().cloned().unwrap_or_default();
                    let mut changed = false;
                    let mut remove = None;
                    for (i, l) in lines.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let style = l["style"].as_str().unwrap_or("").to_string();
                            egui::ComboBox::from_id_salt(("nls_style", i))
                                .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &style)))
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for c in &cnames {
                                        if ui
                                            .selectable_label(*c == style, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, c)))
                                            .clicked()
                                        {
                                            l["style"] = json!(c);
                                            changed = true;
                                        }
                                    }
                                });
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "for"));
                            let mut n = l["lines"].as_u64().unwrap_or(1) as u32;
                            if ui.add(egui::DragValue::new(&mut n).range(1..=999)).changed() {
                                l["lines"] = json!(n);
                                changed = true;
                            }
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, if n == 1 { "line" } else { "lines" }));
                            if ui
                                .small_button("×")
                                .on_hover_ui(|ui| {
                                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Delete"));
                                })
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        lines.remove(i);
                        changed = true;
                    }
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "New Line Style"))).clicked() {
                        lines.push(json!({"style": cnames.get(1).cloned().unwrap_or_default(), "lines": 1}));
                        changed = true;
                    }
                    if changed {
                        d.fields.insert("p.nestedLineStyles".into(), Value::Array(lines));
                    }
                }
                "grep" => {
                    let mut list: Vec<Value> = cur(d, "p.grepStyles", &pv["grepStyles"]).as_array().cloned().unwrap_or_default();
                    let mut changed = false;
                    let mut remove = None;
                    for (i, g) in list.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Apply Style:"));
                            let style = g["style"].as_str().unwrap_or("").to_string();
                            egui::ComboBox::from_id_salt(("gs_style", i))
                                .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &style)))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for c in &cnames {
                                        if ui
                                            .selectable_label(*c == style, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, c)))
                                            .clicked()
                                        {
                                            g["style"] = json!(c);
                                            changed = true;
                                        }
                                    }
                                });
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "To Text:"));
                            let mut pat = g["pattern"].as_str().unwrap_or("").to_string();
                            let bad = regex_ok(&pat).is_err();
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut pat)
                                    .desired_width(150.0)
                                    .text_color_opt(bad.then_some(egui::Color32::from_rgb(240, 90, 90))),
                            );
                            if r.changed() {
                                g["pattern"] = json!(pat);
                                changed = true;
                            }
                            if ui
                                .small_button("×")
                                .on_hover_ui(|ui| {
                                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Delete"));
                                })
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        list.remove(i);
                        changed = true;
                    }
                    if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "New GREP Style"))).clicked() {
                        list.push(json!({"style": cnames.get(1).cloned().unwrap_or_default(), "pattern": "\\d+"}));
                        changed = true;
                    }
                    if changed {
                        d.fields.insert("p.grepStyles".into(), Value::Array(list));
                    }
                }
                "chars" => {
                    char_formatting_sections(app, ui, d, &cv, "ps", &swatches);
                }
                "adv_chars" => {
                    adv_char_formatting_section(app, ui, d, &cv, "ps");
                }
                "opentype" => {
                    opentype_section(app, ui, d, &cv, "ps");
                }
                "indents" => {
                    egui::Grid::new("psi").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Alignment:"));
                        let a: designcraft_doc::Align = serde_json::from_value(cur(d, "p.align", &pv["align"])).unwrap_or_default();
                        egui::ComboBox::from_id_salt("psalign")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, a.label())))
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for al in designcraft_doc::Align::ALL {
                                    if ui.selectable_label(al == a, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, al.label()))).clicked() {
                                        d.fields.insert("p.align".into(), serde_json::to_value(al).unwrap_or_default());
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Direction:"));
                        let dir: designcraft_doc::TextDirection = serde_json::from_value(cur(d, "p.direction", &pv["direction"])).unwrap_or_default();
                        let dir_label = match dir {
                            designcraft_doc::TextDirection::LeftToRight => crate::i18n::tr(&app.ui.language, "Left to Right"),
                            designcraft_doc::TextDirection::RightToLeft => crate::i18n::tr(&app.ui.language, "Right to Left"),
                        };
                        egui::ComboBox::from_id_salt("ps_direction").selected_text(crate::rtl::widget(ui, dir_label)).width(200.0).show_ui(
                            ui,
                            |ui| {
                                for (v, lbl) in [
                                    (designcraft_doc::TextDirection::LeftToRight, "Left to Right"),
                                    (designcraft_doc::TextDirection::RightToLeft, "Right to Left"),
                                ] {
                                    if ui.selectable_label(dir == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                        d.fields.insert("p.direction".into(), serde_json::to_value(v).unwrap_or_default());
                                    }
                                }
                            },
                        );
                        ui.end_row();

                        for (label, key) in [
                            ("Left Indent:", "leftIndent"),
                            ("First Line Indent:", "firstLineIndent"),
                            ("Right Indent:", "rightIndent"),
                            ("Last Line Indent:", "lastLineIndent"),
                            ("Space Before:", "spaceBefore"),
                            ("Space After:", "spaceAfter"),
                        ] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            let v = cur(d, &format!("p.{key}"), &pv[key]).as_f64();
                            if let Some(n) = crate::widgets::measure(ui, &format!("ps{key}"), v, units, 90.0) {
                                d.fields.insert(format!("p.{key}"), json!(n));
                            }
                            ui.end_row();
                        }

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Align to Grid:"));
                        let ga: designcraft_doc::GridAlign = serde_json::from_value(cur(d, "p.gridAlign", &pv["gridAlign"])).unwrap_or_default();
                        egui::ComboBox::from_id_salt("ps_grid_align")
                            .selected_text(crate::rtl::widget(
                                ui,
                                match ga {
                                    designcraft_doc::GridAlign::None => crate::i18n::tr(&app.ui.language, "None"),
                                    designcraft_doc::GridAlign::AllLines => crate::i18n::tr(&app.ui.language, "All Lines"),
                                    designcraft_doc::GridAlign::FirstLineOnly => crate::i18n::tr(&app.ui.language, "First Line Only"),
                                },
                            ))
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for (v, lbl) in [
                                    (designcraft_doc::GridAlign::None, "None"),
                                    (designcraft_doc::GridAlign::AllLines, "All Lines"),
                                    (designcraft_doc::GridAlign::FirstLineOnly, "First Line Only"),
                                ] {
                                    if ui.selectable_label(ga == v, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                        d.fields.insert("p.gridAlign".into(), serde_json::to_value(v).unwrap_or_default());
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Balance Ragged Lines:"));
                        let mut br = cur(d, "p.balanceRagged", &pv["balanceRagged"]).as_bool().unwrap_or(false);
                        if ui.checkbox(&mut br, "").changed() {
                            d.fields.insert("p.balanceRagged".into(), json!(br));
                        }
                        ui.end_row();
                    });
                }
                "span" => {
                    let sc: designcraft_doc::SpanColumns = serde_json::from_value(cur(d, "p.spanColumns", &pv["spanColumns"])).unwrap_or_default();
                    let cur_mode = match sc {
                        designcraft_doc::SpanColumns::Single => "single",
                        designcraft_doc::SpanColumns::Span(_) => "span",
                        designcraft_doc::SpanColumns::Split(_) => "split",
                    };
                    egui::Grid::new("ps_span_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Span Columns:"));
                        let mode_label = match cur_mode {
                            "span" => crate::i18n::tr(&app.ui.language, "Span All Columns"),
                            "split" => crate::i18n::tr(&app.ui.language, "Split Column"),
                            _ => crate::i18n::tr(&app.ui.language, "Single Column"),
                        };
                        egui::ComboBox::from_id_salt("ps_span_mode").selected_text(crate::rtl::widget(ui, mode_label)).width(160.0).show_ui(
                            ui,
                            |ui| {
                                for (m, lbl) in [("single", "Single Column"), ("span", "Span All Columns"), ("split", "Split Column")] {
                                    if ui.selectable_label(cur_mode == m, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                        let new_sc = match m {
                                            "span" => designcraft_doc::SpanColumns::Span(0),
                                            "split" => designcraft_doc::SpanColumns::Split(2),
                                            _ => designcraft_doc::SpanColumns::Single,
                                        };
                                        d.fields.insert("p.spanColumns".into(), serde_json::to_value(new_sc).unwrap_or_default());
                                    }
                                }
                            },
                        );
                        ui.end_row();

                        if let designcraft_doc::SpanColumns::Split(mut n) = sc {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Sub-columns:"));
                            if ui.add(egui::DragValue::new(&mut n).range(2..=10)).changed() {
                                d.fields
                                    .insert("p.spanColumns".into(), serde_json::to_value(designcraft_doc::SpanColumns::Split(n)).unwrap_or_default());
                            }
                            ui.end_row();
                        }
                    });
                }
                "keep" => {
                    egui::Grid::new("ps_keep_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Keep with Next:"));
                        let mut kwn = cur(d, "p.keepWithNext", &pv["keepWithNext"]).as_u64().unwrap_or(0) as u32;
                        if ui.add(egui::DragValue::new(&mut kwn).range(0..=99).suffix(" lines")).changed() {
                            d.fields.insert("p.keepWithNext".into(), json!(kwn));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start Paragraph:"));
                        let sp: designcraft_doc::StartParagraph =
                            serde_json::from_value(cur(d, "p.startParagraph", &pv["startParagraph"])).unwrap_or_default();
                        let sp_label = |p: designcraft_doc::StartParagraph| match p {
                            designcraft_doc::StartParagraph::Anywhere => crate::i18n::tr(&app.ui.language, "Anywhere"),
                            designcraft_doc::StartParagraph::NextColumn => crate::i18n::tr(&app.ui.language, "Next Column"),
                            designcraft_doc::StartParagraph::NextFrame => crate::i18n::tr(&app.ui.language, "Next Frame"),
                            designcraft_doc::StartParagraph::NextPage => crate::i18n::tr(&app.ui.language, "Next Page"),
                            designcraft_doc::StartParagraph::NextOddPage => crate::i18n::tr(&app.ui.language, "Next Odd Page"),
                            designcraft_doc::StartParagraph::NextEvenPage => crate::i18n::tr(&app.ui.language, "Next Even Page"),
                        };
                        egui::ComboBox::from_id_salt("ps_start_para").selected_text(crate::rtl::widget(ui, sp_label(sp))).width(180.0).show_ui(
                            ui,
                            |ui| {
                                for p in [
                                    designcraft_doc::StartParagraph::Anywhere,
                                    designcraft_doc::StartParagraph::NextColumn,
                                    designcraft_doc::StartParagraph::NextFrame,
                                    designcraft_doc::StartParagraph::NextPage,
                                    designcraft_doc::StartParagraph::NextOddPage,
                                    designcraft_doc::StartParagraph::NextEvenPage,
                                ] {
                                    if ui.selectable_label(sp == p, crate::rtl::widget(ui, sp_label(p))).clicked() {
                                        d.fields.insert("p.startParagraph".into(), serde_json::to_value(p).unwrap_or_default());
                                    }
                                }
                            },
                        );
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    let mut klt = cur(d, "p.keepLinesTogether", &pv["keepLinesTogether"]).as_bool().unwrap_or(false);
                    if ui.checkbox(&mut klt, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Keep Lines Together"))).changed() {
                        d.fields.insert("p.keepLinesTogether".into(), json!(klt));
                    }
                    if klt {
                        ui.add_space(4.0);
                        let all = cur(d, "p.keepAllLines", &pv["keepAllLines"]).as_bool().unwrap_or(true);
                        ui.horizontal(|ui| {
                            if ui.selectable_label(all, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "All lines in paragraph"))).clicked()
                            {
                                d.fields.insert("p.keepAllLines".into(), json!(true));
                            }
                            if ui
                                .selectable_label(!all, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Start / End of paragraph")))
                                .clicked()
                            {
                                d.fields.insert("p.keepAllLines".into(), json!(false));
                            }
                        });
                        if !all {
                            ui.add_space(4.0);
                            egui::Grid::new("ps_keep_lines_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start:"));
                                let mut kf = cur(d, "p.keepFirst", &pv["keepFirst"]).as_u64().unwrap_or(2) as u32;
                                if ui.add(egui::DragValue::new(&mut kf).range(1..=99).suffix(" lines")).changed() {
                                    d.fields.insert("p.keepFirst".into(), json!(kf));
                                }
                                ui.end_row();

                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "End:"));
                                let mut kl = cur(d, "p.keepLast", &pv["keepLast"]).as_u64().unwrap_or(2) as u32;
                                if ui.add(egui::DragValue::new(&mut kl).range(1..=99).suffix(" lines")).changed() {
                                    d.fields.insert("p.keepLast".into(), json!(kl));
                                }
                                ui.end_row();
                            });
                        }
                    }
                }
                "rules" => {
                    let is_below = d.b("x.rule_below_mode");
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!is_below, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Rule Above"))).clicked() {
                            d.fields.insert("x.rule_below_mode".into(), json!(false));
                        }
                        if ui.selectable_label(is_below, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Rule Below"))).clicked() {
                            d.fields.insert("x.rule_below_mode".into(), json!(true));
                        }
                    });
                    ui.separator();
                    let (rule_key, base_rule) = if is_below { ("p.ruleBelow", &pv["ruleBelow"]) } else { ("p.ruleAbove", &pv["ruleAbove"]) };
                    let mut rule = cur(d, rule_key, base_rule);
                    let mut r_on = rule["on"].as_bool().unwrap_or(false);
                    let toggle_lbl = if is_below { "Rule Below On" } else { "Rule Above On" };
                    if ui.checkbox(&mut r_on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, toggle_lbl))).changed() {
                        rule["on"] = json!(r_on);
                        d.fields.insert(rule_key.into(), rule.clone());
                    }
                    if r_on {
                        ui.add_space(6.0);
                        egui::Grid::new("ps_rule_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
                            let rw = rule["weight"].as_f64().unwrap_or(1.0);
                            if let Some(n) = crate::widgets::number(ui, "ps_rule_weight", Some(rw), " pt", 70.0, 2) {
                                rule["weight"] = json!(n.max(0.0));
                                d.fields.insert(rule_key.into(), rule.clone());
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Offset:"));
                            let ro = rule["offset"].as_f64().unwrap_or(0.0);
                            if let Some(n) = crate::widgets::measure(ui, "ps_rule_offset", Some(ro), units, 70.0) {
                                rule["offset"] = json!(n);
                                d.fields.insert(rule_key.into(), rule.clone());
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Left Indent:"));
                            let rli = rule["leftIndent"].as_f64().unwrap_or(0.0);
                            if let Some(n) = crate::widgets::measure(ui, "ps_rule_left_indent", Some(rli), units, 70.0) {
                                rule["leftIndent"] = json!(n);
                                d.fields.insert(rule_key.into(), rule.clone());
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Right Indent:"));
                            let rri = rule["rightIndent"].as_f64().unwrap_or(0.0);
                            if let Some(n) = crate::widgets::measure(ui, "ps_rule_right_indent", Some(rri), units, 70.0) {
                                rule["rightIndent"] = json!(n);
                                d.fields.insert(rule_key.into(), rule.clone());
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Width:"));
                            let cw = rule["columnWidth"].as_bool().unwrap_or(true);
                            ui.horizontal(|ui| {
                                if ui.selectable_label(cw, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Column"))).clicked() {
                                    rule["columnWidth"] = json!(true);
                                    d.fields.insert(rule_key.into(), rule.clone());
                                }
                                if ui.selectable_label(!cw, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Text"))).clicked() {
                                    rule["columnWidth"] = json!(false);
                                    d.fields.insert(rule_key.into(), rule.clone());
                                }
                            });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                            let cur_rc = rule["color"].as_str().unwrap_or("[Black]").to_string();
                            egui::ComboBox::from_id_salt("ps_rule_col").selected_text(&cur_rc).width(140.0).show_ui(ui, |ui| {
                                for sw in &swatches {
                                    if ui.selectable_label(*sw == cur_rc, sw).clicked() {
                                        rule["color"] = json!(sw);
                                        d.fields.insert(rule_key.into(), rule.clone());
                                    }
                                }
                            });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
                            let mut rtint = rule["tint"].as_f64().unwrap_or(1.0) * 100.0;
                            if ui.add(egui::Slider::new(&mut rtint, 0.0..=100.0).suffix("%")).changed() {
                                rule["tint"] = json!(rtint / 100.0);
                                d.fields.insert(rule_key.into(), rule.clone());
                            }
                            ui.end_row();
                        });
                    }
                }
                "shading" => {
                    let mut sh_on = cur(d, "p.shadingOn", &pv["shadingOn"]).as_bool().unwrap_or(false);
                    if ui.checkbox(&mut sh_on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Shading"))).changed() {
                        d.fields.insert("p.shadingOn".into(), json!(sh_on));
                    }
                    if sh_on {
                        ui.add_space(6.0);
                        egui::Grid::new("ps_shading_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                            let cur_sc = cur(d, "p.shadingColor", &pv["shadingColor"]).as_str().unwrap_or("[Black]").to_string();
                            egui::ComboBox::from_id_salt("ps_shading_col").selected_text(&cur_sc).width(140.0).show_ui(ui, |ui| {
                                for sw in &swatches {
                                    if ui.selectable_label(*sw == cur_sc, sw).clicked() {
                                        d.fields.insert("p.shadingColor".into(), json!(sw));
                                    }
                                }
                            });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
                            let mut stint = cur(d, "p.shadingTint", &pv["shadingTint"]).as_f64().unwrap_or(0.2) * 100.0;
                            if ui.add(egui::Slider::new(&mut stint, 0.0..=100.0).suffix("%")).changed() {
                                d.fields.insert("p.shadingTint".into(), json!(stint / 100.0));
                            }
                            ui.end_row();

                            let cur_off = cur(d, "p.shadingOffsets", &pv["shadingOffsets"]);
                            let mut t = cur_off.get(0).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut l = cur_off.get(1).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut b = cur_off.get(2).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut r = cur_off.get(3).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut off_changed = false;

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Top Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_sh_off_t", Some(t), units, 80.0) {
                                t = n;
                                off_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bottom Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_sh_off_b", Some(b), units, 80.0) {
                                b = n;
                                off_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Left Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_sh_off_l", Some(l), units, 80.0) {
                                l = n;
                                off_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Right Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_sh_off_r", Some(r), units, 80.0) {
                                r = n;
                                off_changed = true;
                            }
                            ui.end_row();

                            if off_changed {
                                d.fields.insert("p.shadingOffsets".into(), json!([t, l, b, r]));
                            }
                        });
                    }
                }
                "border" => {
                    let mut bd_on = cur(d, "p.borderOn", &pv["borderOn"]).as_bool().unwrap_or(false);
                    if ui.checkbox(&mut bd_on, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Border"))).changed() {
                        d.fields.insert("p.borderOn".into(), json!(bd_on));
                    }
                    if bd_on {
                        ui.add_space(6.0);
                        egui::Grid::new("ps_border_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
                            let cur_bw = cur(d, "p.borderWeights", &pv["borderWeights"]);
                            let mut bw = cur_bw.get(0).and_then(Value::as_f64).unwrap_or(1.0);
                            if let Some(n) = crate::widgets::number(ui, "ps_bd_weight", Some(bw), " pt", 80.0, 2) {
                                bw = n.max(0.0);
                                d.fields.insert("p.borderWeights".into(), json!([bw, bw, bw, bw]));
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
                            let cur_bc = cur(d, "p.borderColor", &pv["borderColor"]).as_str().unwrap_or("[Black]").to_string();
                            egui::ComboBox::from_id_salt("ps_border_col").selected_text(&cur_bc).width(140.0).show_ui(ui, |ui| {
                                for sw in &swatches {
                                    if ui.selectable_label(*sw == cur_bc, sw).clicked() {
                                        d.fields.insert("p.borderColor".into(), json!(sw));
                                    }
                                }
                            });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tint:"));
                            let mut btint = cur(d, "p.borderTint", &pv["borderTint"]).as_f64().unwrap_or(1.0) * 100.0;
                            if ui.add(egui::Slider::new(&mut btint, 0.0..=100.0).suffix("%")).changed() {
                                d.fields.insert("p.borderTint".into(), json!(btint / 100.0));
                            }
                            ui.end_row();

                            let cur_boff = cur(d, "p.borderOffsets", &pv["borderOffsets"]);
                            let mut bt = cur_boff.get(0).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut bl = cur_boff.get(1).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut bb = cur_boff.get(2).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut br_val = cur_boff.get(3).and_then(Value::as_f64).unwrap_or(0.0);
                            let mut boff_changed = false;

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Top Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_bd_off_t", Some(bt), units, 80.0) {
                                bt = n;
                                boff_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bottom Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_bd_off_b", Some(bb), units, 80.0) {
                                bb = n;
                                boff_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Left Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_bd_off_l", Some(bl), units, 80.0) {
                                bl = n;
                                boff_changed = true;
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Right Offset:"));
                            if let Some(n) = crate::widgets::measure(ui, "ps_bd_off_r", Some(br_val), units, 80.0) {
                                br_val = n;
                                boff_changed = true;
                            }
                            ui.end_row();

                            if boff_changed {
                                d.fields.insert("p.borderOffsets".into(), json!([bt, bl, bb, br_val]));
                            }
                        });
                    }
                }
                "bullets" => {
                    let lt: designcraft_doc::ListType = serde_json::from_value(cur(d, "p.listType", &pv["listType"])).unwrap_or_default();
                    egui::Grid::new("ps_bullet_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "List Type:"));
                        let lt_label = match lt {
                            designcraft_doc::ListType::None => crate::i18n::tr(&app.ui.language, "None"),
                            designcraft_doc::ListType::Bullets => crate::i18n::tr(&app.ui.language, "Bullets"),
                            designcraft_doc::ListType::Numbers => crate::i18n::tr(&app.ui.language, "Numbers"),
                        };
                        egui::ComboBox::from_id_salt("ps_list_type").selected_text(crate::rtl::widget(ui, lt_label)).width(160.0).show_ui(ui, |ui| {
                            for (t, lbl) in [
                                (designcraft_doc::ListType::None, "None"),
                                (designcraft_doc::ListType::Bullets, "Bullets"),
                                (designcraft_doc::ListType::Numbers, "Numbers"),
                            ] {
                                if ui.selectable_label(lt == t, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                    d.fields.insert("p.listType".into(), serde_json::to_value(t).unwrap_or_default());
                                }
                            }
                        });
                        ui.end_row();

                        if lt == designcraft_doc::ListType::Bullets {
                            let cur_svg = cur(d, "p.bulletSvg", &pv["bulletSvg"]).as_str().unwrap_or("").to_string();
                            let is_custom = !cur_svg.is_empty()
                                && !["circle", "square", "diamond", "triangle", "arrow", "dash", "line", "star", "check"]
                                    .contains(&cur_svg.to_ascii_lowercase().as_str());

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bullet Graphic:"));
                            let selected_label = if cur_svg.is_empty() {
                                crate::i18n::tr(&app.ui.language, "Character / Glyph")
                            } else if is_custom {
                                crate::i18n::tr(&app.ui.language, "Custom Graphic (SVG/PDF)")
                            } else {
                                match cur_svg.to_ascii_lowercase().as_str() {
                                    "circle" => crate::i18n::tr(&app.ui.language, "Circle"),
                                    "square" => crate::i18n::tr(&app.ui.language, "Square"),
                                    "diamond" => crate::i18n::tr(&app.ui.language, "Diamond"),
                                    "triangle" | "arrow" => crate::i18n::tr(&app.ui.language, "Triangle"),
                                    "dash" | "line" => crate::i18n::tr(&app.ui.language, "Dash"),
                                    "star" => crate::i18n::tr(&app.ui.language, "Star"),
                                    "check" => crate::i18n::tr(&app.ui.language, "Checkmark"),
                                    _ => cur_svg.as_str(),
                                }
                            };

                            egui::ComboBox::from_id_salt("ps_bullet_graphic")
                                .selected_text(crate::rtl::widget(ui, selected_label))
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    let presets = [
                                        ("", "Character / Glyph"),
                                        ("circle", "Circle"),
                                        ("square", "Square"),
                                        ("diamond", "Diamond"),
                                        ("triangle", "Triangle"),
                                        ("dash", "Dash"),
                                        ("star", "Star"),
                                        ("check", "Checkmark"),
                                        ("custom", "Custom Graphic (SVG/PDF)"),
                                    ];
                                    for (val, lbl) in presets {
                                        let active = if val.is_empty() {
                                            cur_svg.is_empty()
                                        } else if val == "custom" {
                                            is_custom
                                        } else {
                                            cur_svg.eq_ignore_ascii_case(val)
                                        };
                                        if ui.selectable_label(active, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                            if val == "custom" {
                                                if cur_svg.is_empty() || !is_custom {
                                                    d.fields.insert(
                                                        "p.bulletSvg".into(),
                                                        json!(
                                                            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><circle cx=\"5\" cy=\"5\" r=\"4\" fill=\"currentColor\"/></svg>"
                                                        ),
                                                    );
                                                }
                                            } else {
                                                d.fields.insert("p.bulletSvg".into(), json!(val));
                                            }
                                        }
                                    }
                                });
                            ui.end_row();

                            if cur_svg.is_empty() {
                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Bullet Character:"));
                                let mut bc = cur(d, "p.bulletChar", &pv["bulletChar"]).as_str().unwrap_or("\u{2022}").to_string();
                                if ui.add(egui::TextEdit::singleline(&mut bc).desired_width(50.0)).changed() {
                                    d.fields.insert("p.bulletChar".into(), json!(bc));
                                }
                                ui.end_row();
                            } else {
                                if is_custom {
                                    crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Custom SVG / PDF:"));
                                    ui.horizontal(|ui| {
                                        let mut custom_val = cur_svg.clone();
                                        if ui.add(egui::TextEdit::singleline(&mut custom_val).desired_width(120.0)).changed() {
                                            let mut s = custom_val.replace(['\r', '\n'], " ");
                                            if s.contains("<svg") && !s.contains("xmlns=") {
                                                s = s.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1);
                                            }
                                            d.fields.insert("p.bulletSvg".into(), json!(s));
                                        }
                                        if ui.button(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Choose File…"))).clicked() {
                                            if let Some(pick) = app.services.pick_open.as_mut() {
                                                if let Some(path) = pick("bullet")
                                                    && let Some(bytes) = app.services.read.as_mut().and_then(|r| r(&path).ok())
                                                {
                                                    let lower = path.to_ascii_lowercase();
                                                    if lower.ends_with(".svg") {
                                                        if let Ok(mut s) = String::from_utf8(bytes) {
                                                            if !s.contains("xmlns=") {
                                                                s = s.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1);
                                                            }
                                                            d.fields.insert("p.bulletSvg".into(), json!(s));
                                                        }
                                                    } else if lower.ends_with(".pdf") || bytes.starts_with(b"%PDF-") {
                                                        let b64 = designcraft_engine::cmd::base64_encode(&bytes);
                                                        d.fields.insert("p.bulletSvg".into(), json!(format!("data:application/pdf;base64,{b64}")));
                                                    } else {
                                                        let mime = if bytes.starts_with(b"\x89PNG") {
                                                            "image/png"
                                                        } else if bytes.starts_with(b"\xff\xd8\xff") {
                                                            "image/jpeg"
                                                        } else if bytes.starts_with(b"GIF8") {
                                                            "image/gif"
                                                        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
                                                            "image/webp"
                                                        } else {
                                                            "image/png"
                                                        };
                                                        let b64 = designcraft_engine::cmd::base64_encode(&bytes);
                                                        d.fields.insert("p.bulletSvg".into(), json!(format!("data:{mime};base64,{b64}")));
                                                    }
                                                    let cur_sz = cur(d, "p.bulletSvgSize", &pv["bulletSvgSize"]);
                                                    if cur_sz.as_array().is_none_or(|a| a.iter().all(|v| v.as_f64().unwrap_or(0.0) <= 0.0)) {
                                                        d.fields.insert("p.bulletSvgSize".into(), json!([8.0, 8.0]));
                                                    }
                                                }
                                            } else if let Some(open) = app.services.open_async.as_mut() {
                                                open("bullet");
                                            }
                                        }
                                    });
                                    ui.end_row();
                                }
                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Graphic Size (W × H):"));
                                let cur_size: [f64; 2] =
                                    serde_json::from_value(cur(d, "p.bulletSvgSize", &pv["bulletSvgSize"])).unwrap_or([0.0, 0.0]);
                                let mut bw = cur_size[0];
                                let mut bh = cur_size[1];
                                let mut s_ch = false;
                                ui.horizontal(|ui| {
                                    if ui.add(egui::DragValue::new(&mut bw).range(0.0..=100.0).speed(0.5).suffix(" pt")).changed() {
                                        s_ch = true;
                                    }
                                    ui.label("×");
                                    if ui.add(egui::DragValue::new(&mut bh).range(0.0..=100.0).speed(0.5).suffix(" pt")).changed() {
                                        s_ch = true;
                                    }
                                });
                                if s_ch {
                                    d.fields.insert("p.bulletSvgSize".into(), json!([bw, bh]));
                                }
                                ui.end_row();

                                crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Baseline Offset:"));
                                let mut bo = cur(d, "p.bulletSvgOffset", &pv["bulletSvgOffset"]).as_f64().unwrap_or(0.0);
                                if ui.add(egui::DragValue::new(&mut bo).range(-50.0..=50.0).speed(0.5).suffix(" pt")).changed() {
                                    d.fields.insert("p.bulletSvgOffset".into(), json!(bo));
                                }
                                ui.end_row();
                            }
                        } else if lt == designcraft_doc::ListType::Numbers {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Number Style:"));
                            let ns: designcraft_doc::NumberStyle =
                                serde_json::from_value(cur(d, "p.numberStyle", &pv["numberStyle"])).unwrap_or_default();
                            egui::ComboBox::from_id_salt("ps_num_style")
                                .selected_text(crate::rtl::widget(
                                    ui,
                                    match ns {
                                        designcraft_doc::NumberStyle::Arabic => "1, 2, 3...",
                                        designcraft_doc::NumberStyle::ArabicLeadingZero => "01, 02, 03...",
                                        designcraft_doc::NumberStyle::UpperRoman => "I, II, III...",
                                        designcraft_doc::NumberStyle::LowerRoman => "i, ii, iii...",
                                        designcraft_doc::NumberStyle::UpperLetters => "A, B, C...",
                                        designcraft_doc::NumberStyle::LowerLetters => "a, b, c...",
                                        designcraft_doc::NumberStyle::Symbols => "*, †, ‡...",
                                    },
                                ))
                                .width(160.0)
                                .show_ui(ui, |ui| {
                                    for (v, lbl) in [
                                        (designcraft_doc::NumberStyle::Arabic, "1, 2, 3..."),
                                        (designcraft_doc::NumberStyle::ArabicLeadingZero, "01, 02, 03..."),
                                        (designcraft_doc::NumberStyle::UpperRoman, "I, II, III..."),
                                        (designcraft_doc::NumberStyle::LowerRoman, "i, ii, iii..."),
                                        (designcraft_doc::NumberStyle::UpperLetters, "A, B, C..."),
                                        (designcraft_doc::NumberStyle::LowerLetters, "a, b, c..."),
                                        (designcraft_doc::NumberStyle::Symbols, "*, †, ‡..."),
                                    ] {
                                        if ui.selectable_label(ns == v, lbl).clicked() {
                                            d.fields.insert("p.numberStyle".into(), serde_json::to_value(v).unwrap_or_default());
                                        }
                                    }
                                });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start At:"));
                            let cur_sa = cur(d, "p.startAt", &pv["startAt"]).as_u64().map(|x| x as u32);
                            let mut sa_num = cur_sa.unwrap_or(1);
                            if ui.add(egui::DragValue::new(&mut sa_num).range(1..=9999)).changed() {
                                d.fields.insert("p.startAt".into(), json!(sa_num));
                            }
                            ui.end_row();
                        }

                        if lt != designcraft_doc::ListType::None {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Character Style:"));
                            let cur_cs = cur(d, "p.bulletCharStyle", &pv["bulletCharStyle"]).as_str().unwrap_or("").to_string();
                            let cs_text = if cur_cs.is_empty() { crate::i18n::tr(&app.ui.language, "[None]") } else { &cur_cs };
                            egui::ComboBox::from_id_salt("ps_bullet_char_style").selected_text(crate::rtl::widget(ui, cs_text)).width(160.0).show_ui(
                                ui,
                                |ui| {
                                    if ui
                                        .selectable_label(cur_cs.is_empty(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "[None]")))
                                        .clicked()
                                    {
                                        d.fields.insert("p.bulletCharStyle".into(), json!(""));
                                    }
                                    for cn in &cnames {
                                        if ui.selectable_label(cur_cs == *cn, cn).clicked() {
                                            d.fields.insert("p.bulletCharStyle".into(), json!(cn));
                                        }
                                    }
                                },
                            );
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Separator:"));
                            let raw_sep = cur(d, "p.listSeparator", &pv["listSeparator"]).as_str().unwrap_or("\t").to_string();
                            let mut display_sep = raw_sep
                                .replace('\t', "^t")
                                .replace('\u{2003}', "^m")
                                .replace('\u{2002}', "^>")
                                .replace('\u{00A0}', "^s");
                            ui.horizontal(|ui| {
                                if ui.add(egui::TextEdit::singleline(&mut display_sep).desired_width(50.0)).changed() {
                                    let saved = display_sep
                                        .replace("^t", "\t")
                                        .replace("^m", "\u{2003}")
                                        .replace("^>", "\u{2002}")
                                        .replace("^s", "\u{00A0}");
                                    d.fields.insert("p.listSeparator".into(), json!(saved));
                                }
                                let cur_preset = if raw_sep == "\t" {
                                    "Tab (^t)"
                                } else if raw_sep == " " {
                                    "Space"
                                } else if raw_sep == "\u{2002}" {
                                    "En Space (^>)"
                                } else if raw_sep == "\u{2003}" {
                                    "Em Space (^m)"
                                } else if raw_sep.is_empty() {
                                    "[None]"
                                } else {
                                    "Custom"
                                };
                                egui::ComboBox::from_id_salt("ps_bullet_sep_preset")
                                    .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, cur_preset)))
                                    .width(105.0)
                                    .show_ui(ui, |ui| {
                                        for (val, lbl) in [
                                            ("\t", "Tab (^t)"),
                                            (" ", "Space"),
                                            ("\u{2002}", "En Space (^>)"),
                                            ("\u{2003}", "Em Space (^m)"),
                                            ("", "[None]"),
                                        ] {
                                            if ui
                                                .selectable_label(raw_sep == val, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl)))
                                                .clicked()
                                            {
                                                d.fields.insert("p.listSeparator".into(), json!(val));
                                            }
                                        }
                                    });
                            });
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Left Indent:"));
                            let li_val = cur(d, "p.leftIndent", &pv["leftIndent"]).as_f64();
                            if let Some(n) = crate::widgets::measure(ui, "ps_bullet_li", li_val, units, 80.0) {
                                d.fields.insert("p.leftIndent".into(), json!(n));
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "First Line Indent:"));
                            let fli_val = cur(d, "p.firstLineIndent", &pv["firstLineIndent"]).as_f64();
                            if let Some(n) = crate::widgets::measure(ui, "ps_bullet_fli", fli_val, units, 80.0) {
                                d.fields.insert("p.firstLineIndent".into(), json!(n));
                            }
                            ui.end_row();

                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tab Position:"));
                            let cur_tab_pos = cur(d, "p.tabs", &pv["tabs"])
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|t| t.get("position"))
                                .and_then(Value::as_f64);
                            if let Some(n) = crate::widgets::measure(ui, "ps_bullet_tp", cur_tab_pos, units, 80.0) {
                                let mut tabs_arr = cur(d, "p.tabs", &pv["tabs"]).as_array().cloned().unwrap_or_default();
                                if n > 0.0 {
                                    if tabs_arr.is_empty() {
                                        tabs_arr.push(json!({"position": n, "align": "left"}));
                                    } else {
                                        tabs_arr[0]["position"] = json!(n);
                                    }
                                    d.fields.insert("p.tabs".into(), json!(tabs_arr));
                                } else if !tabs_arr.is_empty() {
                                    tabs_arr.remove(0);
                                    d.fields.insert("p.tabs".into(), json!(tabs_arr));
                                }
                            }
                            ui.end_row();
                        }
                    });
                }
                "hyph" => {
                    let mut h = cur(d, "p.hyphenate", &pv["hyphenate"]).as_bool().unwrap_or(true);
                    if ui.checkbox(&mut h, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Hyphenate"))).changed() {
                        d.fields.insert("p.hyphenate".into(), json!(h));
                    }
                    egui::Grid::new("psh").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        for (label, key) in [
                            ("Words with at Least:", "hyphMinWord"),
                            ("After First:", "hyphAfterFirst"),
                            ("Before Last:", "hyphBeforeLast"),
                            ("Hyphen Limit:", "hyphLimit"),
                        ] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            let v = cur(d, &format!("p.{key}"), &pv[key]).as_f64();
                            if let Some(n) = crate::widgets::number(ui, &format!("ps{key}"), v, "", 60.0, 0) {
                                d.fields.insert(format!("p.{key}"), json!(n.max(0.0) as u64));
                            }
                            ui.end_row();
                        }
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Hyphenation Zone:"));
                        let hz = cur(d, "p.hyphZone", &pv["hyphZone"]).as_f64();
                        if let Some(n) = crate::widgets::measure(ui, "ps_hyph_zone", hz, units, 70.0) {
                            d.fields.insert("p.hyphZone".into(), json!(n));
                        }
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Language:"));
                        let lang = cur(d, "c.language", &cv["language"]).as_str().unwrap_or("English: USA").to_string();
                        egui::ComboBox::from_id_salt("ps_hyph_language")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &lang)))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for l in crate::panels::properties::LANGUAGES {
                                    if ui.selectable_label(*l == lang, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, l))).clicked() {
                                        d.fields.insert("c.language".into(), json!(l));
                                    }
                                }
                            });
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    for (key, lbl) in [
                        ("hyphCapitalized", "Hyphenate Capitalized Words"),
                        ("hyphLastWord", "Hyphenate Last Word"),
                        ("hyphAcrossColumn", "Hyphenate Across Column"),
                    ] {
                        let mut b = cur(d, &format!("p.{key}"), &pv[key]).as_bool().unwrap_or(true);
                        if ui.checkbox(&mut b, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).changed() {
                            d.fields.insert(format!("p.{key}"), json!(b));
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Hyphenation Weight:"));
                        let mut hw = cur(d, "p.hyphWeight", &pv["hyphWeight"]).as_f64().unwrap_or(0.5);
                        if ui.add(egui::Slider::new(&mut hw, 0.0..=1.0).show_value(false)).changed() {
                            d.fields.insert("p.hyphWeight".into(), json!(hw));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(crate::i18n::tr(&app.ui.language, "Better Spacing")).small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(crate::i18n::tr(&app.ui.language, "Fewer Hyphens")).small());
                        });
                    });
                }
                "justify" => {
                    egui::Grid::new("psj_matrix").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label("");
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Minimum"));
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Desired"));
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Maximum"));
                        ui.end_row();
                        for (label, base) in [("Word Spacing:", "wordSpace"), ("Letter Spacing:", "letterSpace"), ("Glyph Scaling:", "glyphScale")] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            for suffix in ["Min", "Desired", "Max"] {
                                let key = format!("{base}{suffix}");
                                let def = match (base, suffix) {
                                    ("wordSpace", "Min") => 80.0,
                                    ("wordSpace", "Desired") => 100.0,
                                    ("wordSpace", "Max") => 133.0,
                                    ("glyphScale", _) => 100.0,
                                    _ => 0.0,
                                };
                                let v = cur(d, &format!("p.{key}"), &pv[key.as_str()]).as_f64().map(|x| x * 100.0).or(Some(def));
                                if let Some(n) = crate::widgets::number(ui, &format!("ps{key}"), v, "%", 60.0, 0) {
                                    d.fields.insert(format!("p.{key}"), json!(n / 100.0));
                                }
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    egui::Grid::new("psj_options").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Auto Leading:"));
                        let al_val = cur(d, "p.autoLeading", &pv["autoLeading"]).as_f64().map(|x| x * 100.0).or(Some(120.0));
                        if let Some(n) = crate::widgets::number(ui, "ps_auto_leading", al_val, "%", 70.0, 0) {
                            d.fields.insert("p.autoLeading".into(), json!(n / 100.0));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Single Word Justification:"));
                        let swj_cur: designcraft_doc::Align = serde_json::from_value(cur(d, "p.singleWordJustify", &pv["singleWordJustify"]))
                            .unwrap_or(designcraft_doc::Align::FullyJustified);
                        let swj_label = |al: designcraft_doc::Align| match al {
                            designcraft_doc::Align::Left => crate::i18n::tr(&app.ui.language, "Align left"),
                            designcraft_doc::Align::Center => crate::i18n::tr(&app.ui.language, "Align center"),
                            designcraft_doc::Align::Right => crate::i18n::tr(&app.ui.language, "Align right"),
                            _ => crate::i18n::tr(&app.ui.language, "Fully justify"),
                        };
                        egui::ComboBox::from_id_salt("ps_single_word_justify")
                            .selected_text(crate::rtl::widget(ui, swj_label(swj_cur)))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for al in [
                                    designcraft_doc::Align::Left,
                                    designcraft_doc::Align::Center,
                                    designcraft_doc::Align::Right,
                                    designcraft_doc::Align::FullyJustified,
                                ] {
                                    if ui.selectable_label(al == swj_cur, crate::rtl::widget(ui, swj_label(al))).clicked() {
                                        d.fields.insert("p.singleWordJustify".into(), serde_json::to_value(al).unwrap_or_default());
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Composer:"));
                        let cur_comp = cur(d, "p.composer", &pv["composer"]).as_str().unwrap_or("paragraph").to_string();
                        let comp_label = |c: &str| match c {
                            "singleLine" => crate::i18n::tr(&app.ui.language, "Single-line Composer"),
                            _ => crate::i18n::tr(&app.ui.language, "Paragraph Composer"),
                        };
                        egui::ComboBox::from_id_salt("ps_composer")
                            .selected_text(crate::rtl::widget(ui, comp_label(&cur_comp)))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for c in ["paragraph", "singleLine"] {
                                    if ui.selectable_label(c == cur_comp, crate::rtl::widget(ui, comp_label(c))).clicked() {
                                        d.fields.insert("p.composer".into(), json!(c));
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Insert Kashidas:"));
                        let mut k = cur(d, "p.kashidas", &pv["kashidas"]).as_bool().unwrap_or(true);
                        if ui.checkbox(&mut k, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "In justified Arabic text"))).changed() {
                            d.fields.insert("p.kashidas".into(), json!(k));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Korean Line Breaks:"));
                        let mut kb = cur(d, "p.koreanCharBreaks", &pv["koreanCharBreaks"]).as_bool().unwrap_or(false);
                        if ui
                            .checkbox(&mut kb, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "Between syllables (not only at spaces)")))
                            .changed()
                        {
                            d.fields.insert("p.koreanCharBreaks".into(), json!(kb));
                        }
                        ui.end_row();
                    });
                }
                "color" => {
                    if let Some(st) = app.session.active() {
                        char_color_section(app, ui, d, &cv, "ps", &swatches, &st.doc);
                    }
                }
                "underline" => {
                    underline_section(app, ui, d, &cv, "ps", &swatches);
                }
                "strikethrough" => {
                    strikethrough_section(app, ui, d, &cv, "ps", &swatches);
                }
                _ => {
                    egui::Grid::new("psg").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Style Name:"));
                        if !d.fields.contains_key("rename") {
                            d.fields.insert("rename".into(), json!(name));
                        }
                        let mut rn = d.s("rename");
                        if ui.add(egui::TextEdit::singleline(&mut rn).desired_width(240.0)).changed() {
                            d.fields.insert("rename".into(), json!(rn));
                        }
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Based On:"));
                        let based = d
                            .fields
                            .get("basedOn")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or(style.based_on.clone())
                            .unwrap_or_else(|| "[No Paragraph Style]".into());
                        egui::ComboBox::from_id_salt("psbased")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &based)))
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for n in &names {
                                    if ui
                                        .selectable_label(*n == based, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, n)))
                                        .clicked()
                                    {
                                        d.fields.insert("basedOn".into(), json!(n));
                                    }
                                }
                            });
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Next Style:"));
                        let mut next_names = vec!["[Same Style]".to_string()];
                        next_names.extend(all_para_names.iter().cloned());
                        let next_style_val = d
                            .fields
                            .get("nextStyle")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or(style.next_style.clone())
                            .unwrap_or_else(|| "[Same Style]".into());
                        egui::ComboBox::from_id_salt("psnext")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &next_style_val)))
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for n in &next_names {
                                    if ui
                                        .selectable_label(*n == next_style_val, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, n)))
                                        .clicked()
                                    {
                                        d.fields.insert("nextStyle".into(), json!(n));
                                    }
                                }
                            });
                        ui.end_row();
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Shortcut:"));
                        if !d.fields.contains_key("shortcut") {
                            d.fields.insert("shortcut".into(), json!(style.shortcut));
                        }
                        let mut sc = d.s("shortcut");
                        if ui.add(egui::TextEdit::singleline(&mut sc).desired_width(240.0)).changed() {
                            d.fields.insert("shortcut".into(), json!(sc));
                        }
                        ui.end_row();
                    });
                    ui.add_space(12.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Style Settings:")).font(semibold(12.0)));
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let tokens = crate::theme::Tokens::get(ui.ctx());
                        let lead_str = match &cp.leading {
                            designcraft_doc::Leading::Auto => "Auto".to_string(),
                            designcraft_doc::Leading::Points(pt) => format!("{pt:.1} pt"),
                        };
                        let summary = format!(
                            "{} + {} {} {:.1} pt / {} + align: {} + hyphenate: {}",
                            style.name,
                            cp.font_family,
                            cp.font_style,
                            cp.size,
                            lead_str,
                            crate::i18n::tr(&app.ui.language, pp.align.label()),
                            pp.hyphenate
                        );
                        crate::rtl::label(ui, egui::RichText::new(summary).color(tokens.text_dim));
                    });
                }
            });
        });
    });
}

fn character_style_options(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let name = d.s("name");
    let Some(st) = app.session.active() else { return };
    let Some(style) = st.doc.styles.char_style(&name).cloned() else {
        ui.label(format!("No style named {name}"));
        return;
    };
    let mut names: Vec<String> = st.doc.styles.character.iter().map(|p| p.name.clone()).filter(|n| *n != name).collect();
    if !names.iter().any(|n| n == designcraft_doc::NO_CHAR_STYLE) {
        names.insert(0, designcraft_doc::NO_CHAR_STYLE.into());
    }
    let cp = st.doc.styles.resolve_char_style(&name);
    let cv = serde_json::to_value(&cp).unwrap_or_default();
    let cur = |d: &Dialog, k: &str, base: &Value| d.fields.get(k).cloned().unwrap_or_else(|| base.clone());
    let swatches: Vec<String> = st.doc.swatches.iter().map(|s| s.name.clone()).collect();
    let cur_sec = d.s("section");
    let cur_sec = if cur_sec.is_empty() { "general" } else { cur_sec.as_str() };
    let sections = [
        ("general", "General"),
        ("chars", "Basic Character Formats"),
        ("adv_chars", "Advanced Character Formats"),
        ("indents", "Indents and Spacing"),
        ("color", "Character Color"),
        ("opentype", "OpenType Features"),
        ("underline", "Underline Options"),
        ("strikethrough", "Strikethrough Options"),
        ("export", "Export Tagging"),
        ("copyfit", "Copyfitting"),
    ];

    ui.set_min_width(740.0);
    ui.horizontal_top(|ui| {
        ui.set_min_height(480.0);
        ui.set_max_height(480.0);

        ui.vertical(|ui| {
            ui.set_width(190.0);
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Categories:")).font(semibold(12.0)));
            ui.add_space(4.0);
            egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(2, 4)).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("cs_sidebar_scroll").max_height(440.0).show(ui, |ui| {
                    ui.set_width(180.0);
                    for (id, label) in sections {
                        let is_sel = cur_sec == id;
                        if ui.selectable_label(is_sel, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                            d.fields.insert("section".into(), json!(id));
                        }
                    }
                });
            });
        });
        ui.separator();
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.set_min_width(520.0);
            let sec_title = sections.iter().find(|(id, _)| *id == cur_sec).map(|(_, l)| *l).unwrap_or("General");
            crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, sec_title)).font(semibold(14.0)));
            ui.separator();
            ui.add_space(8.0);

            egui::ScrollArea::vertical().id_salt("cs_content_scroll").max_height(430.0).show(ui, |ui| match cur_sec {
                "copyfit" => {
                    let policy = app.session.active().and_then(|st| st.doc.styles.resolve_copyfit(designcraft_doc::NO_PARA_STYLE, &name));
                    if style.copyfit.is_none() && policy.is_some() {
                        ui.label(crate::i18n::tr(&app.ui.language, "Inherited copyfit limits; editing creates a local override"));
                    }
                    copyfit_options(ui, d, policy, &cp, &app.ui.language);
                }
                "export" => {
                    let et = app.session.active().and_then(|s| s.doc.styles.export_tag(&name, true).cloned()).unwrap_or_default();
                    if !d.fields.contains_key("x.tag") {
                        d.fields.insert("x.tag".into(), json!(if et.tag.is_empty() { "[Automatic]" } else { et.tag.as_str() }));
                        d.fields.insert("x.class".into(), json!(et.class));
                    }
                    egui::Grid::new("cs_exp_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Tag:"));
                        let cur_t = d.s("x.tag");
                        egui::ComboBox::from_id_salt("cs_export_tag")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, &cur_t)))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for t in ["[Automatic]", "span", "strong", "em", "code", "sub", "sup", "b", "i", "u", "mark", "small"] {
                                    if ui.selectable_label(cur_t == t, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, t))).clicked() {
                                        d.fields.insert("x.tag".into(), json!(t));
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Class:"));
                        text_field(ui, d, "x.class", 180.0);
                        ui.end_row();
                    });
                }
                "chars" => {
                    char_formatting_sections(app, ui, d, &cv, "cs", &swatches);
                }
                "adv_chars" => {
                    adv_char_formatting_section(app, ui, d, &cv, "cs");
                }
                "indents" => {
                    let units = st.doc.settings.horizontal_units;
                    egui::Grid::new("cs_indents_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Alignment:"));
                        let cur_al: Option<designcraft_doc::Align> = serde_json::from_value(cur(d, "c.align", &cv["align"])).ok();
                        let al_text = cur_al
                            .map(|a| crate::i18n::tr(&app.ui.language, a.label()))
                            .unwrap_or_else(|| crate::i18n::tr(&app.ui.language, "[Inherited]"));
                        egui::ComboBox::from_id_salt("cs_align").selected_text(crate::rtl::widget(ui, al_text)).width(200.0).show_ui(ui, |ui| {
                            if ui
                                .selectable_label(cur_al.is_none(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "[Inherited]")))
                                .clicked()
                            {
                                d.fields.insert("c.align".into(), Value::Null);
                            }
                            for al in designcraft_doc::Align::ALL {
                                if ui
                                    .selectable_label(cur_al == Some(al), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, al.label())))
                                    .clicked()
                                {
                                    d.fields.insert("c.align".into(), serde_json::to_value(al).unwrap_or_default());
                                }
                            }
                        });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Align to Grid:"));
                        let cur_ga: Option<designcraft_doc::GridAlign> = serde_json::from_value(cur(d, "c.gridAlign", &cv["gridAlign"])).ok();
                        let ga_text = cur_ga
                            .map(|g| match g {
                                designcraft_doc::GridAlign::None => crate::i18n::tr(&app.ui.language, "None"),
                                designcraft_doc::GridAlign::AllLines => crate::i18n::tr(&app.ui.language, "All Lines"),
                                designcraft_doc::GridAlign::FirstLineOnly => crate::i18n::tr(&app.ui.language, "First Line Only"),
                            })
                            .unwrap_or_else(|| crate::i18n::tr(&app.ui.language, "[Inherited]"));
                        egui::ComboBox::from_id_salt("cs_grid_align").selected_text(crate::rtl::widget(ui, ga_text)).width(200.0).show_ui(ui, |ui| {
                            if ui
                                .selectable_label(cur_ga.is_none(), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, "[Inherited]")))
                                .clicked()
                            {
                                d.fields.insert("c.gridAlign".into(), Value::Null);
                            }
                            for (v, lbl) in [
                                (designcraft_doc::GridAlign::None, "None"),
                                (designcraft_doc::GridAlign::AllLines, "All Lines"),
                                (designcraft_doc::GridAlign::FirstLineOnly, "First Line Only"),
                            ] {
                                if ui.selectable_label(cur_ga == Some(v), crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, lbl))).clicked() {
                                    d.fields.insert("c.gridAlign".into(), serde_json::to_value(v).unwrap_or_default());
                                }
                            }
                        });
                        ui.end_row();

                        for (label, key) in [
                            ("Left Indent:", "leftIndent"),
                            ("First Line Indent:", "firstLineIndent"),
                            ("Right Indent:", "rightIndent"),
                            ("Space Before:", "spaceBefore"),
                            ("Space After:", "spaceAfter"),
                        ] {
                            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, label));
                            let v = cur(d, &format!("c.{key}"), &cv[key]).as_f64();
                            if let Some(n) = crate::widgets::measure(ui, &format!("cs{key}"), v, units, 90.0) {
                                d.fields.insert(format!("c.{key}"), json!(n));
                            }
                            ui.end_row();
                        }

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Keep with Next:"));
                        let mut kwn = cur(d, "c.keepWithNext", &cv["keepWithNext"]).as_u64().unwrap_or(0) as u32;
                        if ui.add(egui::DragValue::new(&mut kwn).range(0..=5).suffix(" lines")).changed() {
                            d.fields.insert("c.keepWithNext".into(), json!(kwn));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Hyphenate:"));
                        let cur_hyph = cur(d, "c.hyphenate", &cv["hyphenate"]);
                        let mut hyph_checked = cur_hyph.as_bool().unwrap_or(true);
                        if ui.checkbox(&mut hyph_checked, "").changed() {
                            d.fields.insert("c.hyphenate".into(), json!(hyph_checked));
                        }
                        ui.end_row();
                    });
                }
                "opentype" => {
                    opentype_section(app, ui, d, &cv, "cs");
                }
                "underline" => {
                    underline_section(app, ui, d, &cv, "cs", &swatches);
                }
                "strikethrough" => {
                    strikethrough_section(app, ui, d, &cv, "cs", &swatches);
                }
                "color" => {
                    char_color_section(app, ui, d, &cv, "cs", &swatches, &st.doc);
                }
                _ => {
                    egui::Grid::new("csg").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Style Name:"));
                        if !d.fields.contains_key("rename") {
                            d.fields.insert("rename".into(), json!(name));
                        }
                        let mut rn = d.s("rename");
                        if ui.add(egui::TextEdit::singleline(&mut rn).desired_width(240.0)).changed() {
                            d.fields.insert("rename".into(), json!(rn));
                        }
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Based On:"));
                        let based = d
                            .fields
                            .get("basedOn")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or(style.based_on.clone())
                            .unwrap_or_else(|| designcraft_doc::NO_CHAR_STYLE.into());
                        egui::ComboBox::from_id_salt("csbased")
                            .selected_text(crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, &based)))
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for n in &names {
                                    if ui
                                        .selectable_label(*n == based, crate::rtl::widget(ui, crate::i18n::style_name(&app.ui.language, n)))
                                        .clicked()
                                    {
                                        d.fields.insert("basedOn".into(), json!(n));
                                    }
                                }
                            });
                        ui.end_row();

                        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Shortcut:"));
                        if !d.fields.contains_key("shortcut") {
                            d.fields.insert("shortcut".into(), json!(style.shortcut));
                        }
                        let mut sc = d.s("shortcut");
                        if ui.add(egui::TextEdit::singleline(&mut sc).desired_width(240.0)).changed() {
                            d.fields.insert("shortcut".into(), json!(sc));
                        }
                        ui.end_row();
                    });

                    ui.add_space(12.0);
                    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Style Settings:")).font(semibold(12.0)));
                    ui.add_space(4.0);
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let tokens = crate::theme::Tokens::get(ui.ctx());
                        let summary = format!("{} + {} {} {:.1} pt · {}", style.name, cp.font_family, cp.font_style, cp.size, cp.fill);
                        crate::rtl::label(ui, egui::RichText::new(summary).color(tokens.text_dim));
                    });
                }
            });
        });
    });
}

fn combo(ui: &mut egui::Ui, d: &mut Dialog, key: &str, opts: &[(&str, &str)]) {
    let cur = d.s(key);
    let shown = opts.iter().find(|o| o.0 == cur).map_or(cur.as_str(), |o| o.1).to_string();
    egui::ComboBox::from_id_salt(key).selected_text(crate::rtl::widget(ui, &shown)).width(170.0).show_ui(ui, |ui| {
        for (v, label) in opts {
            if ui.selectable_label(cur == *v, crate::rtl::widget(ui, *label)).clicked() {
                d.fields.insert(key.into(), json!(v));
            }
        }
    });
}

/// Translate enum captions without changing serialized values or user-defined names.
fn translated_combo(lang: &str, ui: &mut egui::Ui, d: &mut Dialog, key: &str, opts: &[(&str, &str)]) {
    let opts: Vec<_> = opts.iter().map(|(value, label)| (*value, crate::i18n::tr(lang, label))).collect();
    combo(ui, d, key, &opts);
}

/// Style name picker (paragraph or character styles of the active document).
fn style_combo(app: &DesignApp, ui: &mut egui::Ui, d: &mut Dialog, key: &str, character: bool) {
    let names: Vec<String> = app
        .session
        .active()
        .map(|st| {
            if character {
                st.doc.styles.character.iter().map(|s| s.name.clone()).collect()
            } else {
                st.doc.styles.paragraph.iter().map(|s| s.name.clone()).collect()
            }
        })
        .unwrap_or_default();
    let mut opts: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), crate::i18n::style_name(&app.ui.language, n))).collect();
    if character && !names.iter().any(|n| n == designcraft_doc::NO_CHAR_STYLE) {
        opts.insert(0, (designcraft_doc::NO_CHAR_STYLE, crate::i18n::style_name(&app.ui.language, designcraft_doc::NO_CHAR_STYLE)));
    }
    combo(ui, d, key, &opts);
}

/// Document Footnote Options: Numbering and Formatting / Layout tabs.
fn footnote_options(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    ui.horizontal(|ui| {
        for (tab, label) in [("numbering", "Numbering and Formatting"), ("layout", "Layout")] {
            if ui.selectable_label(d.s("tab") == tab, crate::rtl::widget(ui, crate::i18n::tr(&app.ui.language, label))).clicked() {
                d.fields.insert("tab".into(), json!(tab));
            }
        }
    });
    ui.separator();
    let head = |ui: &mut egui::Ui, t: &str| {
        ui.add_space(4.0);
        crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, t)).font(semibold(12.0)));
    };
    if d.s("tab") == "layout" {
        head(ui, "Spacing Options");
        egui::Grid::new("fn_sp").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Minimum Space Before First Footnote:"));
            text_field(ui, d, "spaceBefore", 70.0);
            ui.end_row();
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Space Between Footnotes:"));
            text_field(ui, d, "spaceBetween", 70.0);
            ui.end_row();
        });
        head(ui, "First Baseline");
        egui::Grid::new("fn_fb").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Offset:"));
            translated_combo(
                &app.ui.language,
                ui,
                d,
                "firstBaseline",
                &[("ascent", "Ascent"), ("capHeight", "Cap Height"), ("leading", "Leading"), ("xHeight", "x Height"), ("fixed", "Fixed")],
            );
            ui.end_row();
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Min:"));
            text_field(ui, d, "firstBaselineMin", 70.0);
            ui.end_row();
        });
        check(ui, d, "spanColumns", crate::i18n::tr(&app.ui.language, "Span Footnotes Across Columns"));
        head(ui, "Rule Above");
        check(ui, d, "rule.on", crate::i18n::tr(&app.ui.language, "Rule On"));
        egui::Grid::new("fn_rule").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Weight:"));
            text_field(ui, d, "rule.weight", 60.0);
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Color:"));
            let swatches: Vec<String> = app.session.active().map(|st| st.doc.swatches.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
            let opts: Vec<(&str, &str)> = swatches.iter().map(|n| (n.as_str(), n.as_str())).collect();
            combo(ui, d, "rule.color", &opts);
            ui.end_row();
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Width:"));
            text_field(ui, d, "rule.width", 60.0);
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Offset:"));
            text_field(ui, d, "rule.offset", 60.0);
            ui.end_row();
            crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Left Indent:"));
            text_field(ui, d, "rule.leftIndent", 60.0);
            ui.end_row();
        });
        return;
    }
    head(ui, "Numbering");
    egui::Grid::new("fn_num").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Style:"));
        translated_combo(
            &app.ui.language,
            ui,
            d,
            "style",
            &[
                ("arabic", "1, 2, 3, 4..."),
                ("upperRoman", "I, II, III, IV..."),
                ("lowerRoman", "i, ii, iii, iv..."),
                ("upperLetters", "A, B, C, D..."),
                ("lowerLetters", "a, b, c, d..."),
                ("arabicLeadingZero", "01, 02, 03..."),
                ("symbols", "*, †, ‡, §..."),
            ],
        );
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Start at:"));
        text_field(ui, d, "startAt", 60.0);
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Restart Numbering Every:"));
        translated_combo(
            &app.ui.language,
            ui,
            d,
            "restart",
            &[("never", "Never (continuous)"), ("page", "Page"), ("spread", "Spread"), ("section", "Section")],
        );
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Show Prefix/Suffix in:"));
        translated_combo(
            &app.ui.language,
            ui,
            d,
            "affixIn",
            &[("none", "None"), ("reference", "Footnote Reference"), ("text", "Footnote Text"), ("both", "Both")],
        );
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Prefix:"));
        text_field(ui, d, "prefix", 60.0);
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Suffix:"));
        text_field(ui, d, "suffix", 60.0);
        ui.end_row();
    });
    head(ui, "Footnote Reference Number in Text");
    egui::Grid::new("fn_ref").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Position:"));
        translated_combo(
            &app.ui.language,
            ui,
            d,
            "refPosition",
            &[
                ("superscript", "Apply Superscript"),
                ("subscript", "Apply Subscript"),
                ("normal", "Apply Normal"),
                ("otSuperscript", "OpenType Superscript"),
            ],
        );
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Character Style:"));
        style_combo(app, ui, d, "refCharStyle", true);
        ui.end_row();
    });
    head(ui, "Footnote Formatting");
    egui::Grid::new("fn_fmt").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Paragraph Style:"));
        style_combo(app, ui, d, "paraStyle", false);
        ui.end_row();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Separator:"));
        text_field(ui, d, "separator", 60.0);
        ui.end_row();
    });
}

/// One parameter of a command, parsed from its params documentation
/// (`{name, type?: a|b|c, count?, flag?: bool, …}`).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandField {
    pub key: String,
    pub optional: bool,
    /// Enumerated values (`a|b|c`).
    pub choices: Vec<String>,
    pub boolean: bool,
    /// The documented value spec (shown as a hint).
    pub hint: String,
}

/// Parameters of a command from its documentation string: the top-level keys of the first `{…}`.
pub fn command_fields(doc: &str) -> Vec<CommandField> {
    let Some(start) = doc.find('{') else { return vec![] };
    let mut depth = 0i32;
    let mut end = doc.len();
    for (i, c) in doc[start..].char_indices() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &doc[start + 1..end];
    let mut parts = Vec::new();
    let (mut depth, mut from) = (0i32, 0usize);
    for (i, c) in body.char_indices() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[from..]);
    let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit());
    let mut out = Vec::new();
    for part in parts {
        // `key`, `key?`, `key: spec` or `key (spec)`: the key is the leading word.
        let part = part.trim();
        let k_end = part.find(|c: char| c.is_whitespace() || c == ':' || c == '(').unwrap_or(part.len());
        let (k, rest) = part.split_at(k_end);
        let rest = rest.trim_start();
        let spec = match rest.strip_prefix(':') {
            Some(v) => v.trim(),
            None => rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')).unwrap_or(rest).trim(),
        };
        let optional = k.ends_with('?');
        let key = k.trim_end_matches('?');
        if !ident(key) || out.iter().any(|f: &CommandField| f.key == key) {
            continue;
        }
        let word = spec.split_whitespace().next().unwrap_or("");
        let choices: Vec<String> =
            if word.contains('|') && word.split('|').all(ident) { word.split('|').map(str::to_string).collect() } else { vec![] };
        let boolean = spec.starts_with("bool") || spec.starts_with("true|false") || spec.starts_with("true") && spec.len() <= 5;
        out.push(CommandField { key: key.into(), optional, choices: if boolean { vec![] } else { choices }, boolean, hint: spec.into() });
    }
    out
}

fn humanize(key: &str) -> String {
    let mut s = String::new();
    for (i, c) in key.chars().enumerate() {
        if i == 0 {
            s.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            s.push(' ');
            s.extend(c.to_lowercase());
        } else {
            s.push(c);
        }
    }
    s
}

/// A form for any command, from its parameter documentation.
fn command_form(app: &crate::DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let doc = designcraft_engine::find_command(&d.id[4..]).map_or("", |c| c.params);
    let fields = command_fields(doc);
    let dim = crate::theme::Tokens::get(ui.ctx()).text_dim;
    egui::Grid::new("cmdform").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        for f in &fields {
            ui.label(crate::rtl::widget(
                ui,
                format!("{}{}", crate::i18n::tr(&app.ui.language, &humanize(&f.key)), if f.optional { "" } else { " *" }),
            ));
            if f.boolean {
                check(ui, d, &f.key, "");
            } else if !f.choices.is_empty() {
                let mut opts: Vec<(&str, &str)> = vec![("", "—")];
                let labels: Vec<_> = f.choices.iter().map(|c| humanize(c)).collect();
                opts.extend(f.choices.iter().zip(&labels).map(|(value, label)| (value.as_str(), crate::i18n::tr(&app.ui.language, label))));
                combo(ui, d, &f.key, &opts);
            } else {
                let mut s = d.s(&f.key);
                let hint = if f.hint.is_empty() { String::new() } else { f.hint.clone() };
                if ui.add(egui::TextEdit::singleline(&mut s).hint_text(egui::RichText::new(hint).color(dim)).desired_width(240.0)).changed() {
                    d.fields.insert(f.key.clone(), Value::String(s));
                }
            }
            ui.end_row();
        }
    });
    if let Some(st) = d.fields.get("status").and_then(Value::as_str) {
        ui.label(egui::RichText::new(st).color(crate::theme::Tokens::get(ui.ctx()).text_dim));
    }
}

/// New Cross-Reference: link to a paragraph (paragraph styles on the left, their paragraphs on
/// the right) or a text anchor, with a cross-reference format.
fn insert_xref(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    let Some(st) = app.session.active() else { return };
    let doc = &st.doc;
    let all = "[All Paragraphs]";
    ui.horizontal(|ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Link To:"));
        combo(ui, d, "linkTo", &[("paragraph", "Paragraph"), ("anchor", "Text Anchor")]);
    });
    ui.add_space(6.0);
    let dim = crate::theme::Tokens::get(ui.ctx()).text_dim;
    if d.s("linkTo") == "anchor" {
        egui::ScrollArea::vertical().id_salt("xr_anchors").max_height(220.0).max_width(460.0).show(ui, |ui| {
            let mut any = false;
            for st in doc.stories.values() {
                for a in &st.anchors {
                    any = true;
                    let key = format!("a:{}", a.id);
                    if ui.selectable_label(d.s("target") == key, &a.name).clicked() {
                        d.fields.insert("target".into(), json!(key));
                    }
                }
            }
            if !any {
                ui.label(crate::rtl::widget(
                    ui,
                    egui::RichText::new(crate::i18n::tr(&app.ui.language, "No text anchors in this document.")).color(dim),
                ));
            }
        });
    } else {
        let mut styles: Vec<String> = vec![all.to_string()];
        styles.extend(doc.styles.paragraph.iter().map(|s| s.name.clone()));
        let cur_style = if d.s("style").is_empty() { all.to_string() } else { d.s("style") };
        ui.horizontal_top(|ui| {
            let col = |w: f32| (egui::vec2(w, 240.0), egui::Layout::top_down(egui::Align::Min));
            let (sz, lay) = col(180.0);
            ui.allocate_ui_with_layout(sz, lay, |ui| {
                egui::ScrollArea::vertical().id_salt("xr_styles").auto_shrink([false, false]).show(ui, |ui| {
                    for name in &styles {
                        if ui.selectable_label(*name == cur_style, name).clicked() {
                            d.fields.insert("style".into(), json!(name));
                        }
                    }
                })
            });
            ui.add_space(8.0);
            let (sz, lay) = col(330.0);
            ui.allocate_ui_with_layout(sz, lay, |ui| {
                egui::ScrollArea::vertical().id_salt("xr_paras").auto_shrink([false, false]).show(ui, |ui| {
                    for st in doc.stories.values().filter(|s| !s.frames.is_empty()) {
                        for (pi, r) in st.para_ranges().into_iter().enumerate() {
                            if cur_style != all && st.paras[pi].style != cur_style {
                                continue;
                            }
                            let text = designcraft_doc::xref::clean(&st.text[r]);
                            if text.is_empty() {
                                continue;
                            }
                            let short: String = text.chars().take(52).collect();
                            let key = format!("{}:{pi}", st.id.0);
                            if ui.selectable_label(d.s("target") == key, short).clicked() {
                                d.fields.insert("target".into(), json!(key));
                            }
                        }
                    }
                })
            });
        });
    }
    ui.add_space(8.0);
    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Cross-Reference Format")).font(semibold(12.0)));
    let names: Vec<String> = doc.xref_formats.iter().map(|f| f.name.clone()).collect();
    if d.s("format").is_empty() {
        d.fields.insert("format".into(), json!(names.first().cloned().unwrap_or_default()));
    }
    let opts: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), n.as_str())).collect();
    ui.horizontal(|ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Format:"));
        combo(ui, d, "format", &opts);
    });
    if let Some(m) = d.fields.get("status").and_then(Value::as_str) {
        ui.label(egui::RichText::new(m).color(dim));
    }
}

/// Color Picker: saturation/brightness field and hue slider, with RGB, CMYK and hex fields.
fn color_picker(ui: &mut egui::Ui, d: &mut Dialog) {
    let hex = d.s("hex");
    let mut c = designcraft_color::Color::from_hex(&hex).map(|c| c.to_rgb()).map_or(egui::Color32::BLACK, |[r, g, b]| {
        egui::Color32::from_rgb((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
    });
    let before = c;
    ui.horizontal_top(|ui| {
        egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque);
        ui.vertical(|ui| {
            let mut rgb = [c.r() as f32, c.g() as f32, c.b() as f32];
            egui::Grid::new("cp_rgb").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                for (k, l) in ["R", "G", "B"].iter().enumerate() {
                    ui.label(*l);
                    ui.add(egui::DragValue::new(&mut rgb[k]).range(0.0..=255.0));
                    ui.end_row();
                }
            });
            c = egui::Color32::from_rgb(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8);
            let col = designcraft_color::Color::rgb8(c.r(), c.g(), c.b());
            let [cc, m, y, k] = col.to_cmyk();
            ui.add_space(6.0);
            ui.label(format!("C {:.0}%  M {:.0}%  Y {:.0}%  K {:.0}%", cc * 100.0, m * 100.0, y * 100.0, k * 100.0));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("#");
                let mut h = format!("{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
                if ui.add(egui::TextEdit::singleline(&mut h).desired_width(70.0)).changed()
                    && let Some(n) = designcraft_color::Color::from_hex(&h)
                {
                    let [r, g, b] = n.to_rgb();
                    c = egui::Color32::from_rgb((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8);
                }
            });
        });
    });
    if c != before {
        d.fields.insert("hex".into(), json!(format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())));
    }
}

/// Find/Replace Font: the document's fonts (missing ones first, flagged) and a replacement.
fn find_font(app: &mut DesignApp, ui: &mut egui::Ui, d: &mut Dialog) {
    // Listed once when the dialog opens (it scans every story).
    if !d.fields.contains_key("_fonts") {
        let list = app.session.execute("font.list", &json!({})).unwrap_or_default();
        d.fields.insert("_fonts".into(), list);
    }
    let fonts = d.fields.get("_fonts").and_then(Value::as_array).cloned().unwrap_or_default();
    let missing = fonts.iter().filter(|f| f["missing"] == true || f["styleMissing"] == true).count();
    crate::rtl::label(
        ui,
        crate::i18n::tr(&app.ui.language, "Fonts in Document: {count}    Missing: {missing}")
            .replace("{count}", &fonts.len().to_string())
            .replace("{missing}", &missing.to_string()),
    );
    egui::ScrollArea::vertical().id_salt("ff_list").max_height(180.0).show(ui, |ui| {
        for f in &fonts {
            let (fam, st) = (f["family"].as_str().unwrap_or(""), f["style"].as_str().unwrap_or(""));
            let warn = f["missing"] == true || f["styleMissing"] == true;
            let label = format!("{}{fam} {st}", if warn { "\u{26A0} " } else { "" });
            let on = d.s("family") == fam && d.s("style") == st;
            let mut text = egui::RichText::new(label);
            if warn {
                text = text.color(egui::Color32::from_rgb(0xe5, 0x4b, 0x4b));
            }
            if ui.selectable_label(on, text).clicked() {
                d.fields.insert("family".into(), json!(fam));
                d.fields.insert("style".into(), json!(st));
            }
        }
    });
    ui.add_space(8.0);
    crate::rtl::label(ui, egui::RichText::new(crate::i18n::tr(&app.ui.language, "Replace With")).font(semibold(12.0)));
    let db = crate::panels::fonts(app);
    let menu = crate::panels::font_menu(app);
    let english = app.session.prefs.show_font_names_in_english;
    let fam_opts: Vec<(&str, &str)> = menu.iter().map(|f| (f.family.as_str(), f.label(english))).collect();
    egui::Grid::new("ff_to").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Font Family:"));
        combo(ui, d, "toFamily", &fam_opts);
        ui.end_row();
        let styles = db.styles(&d.s("toFamily"));
        let st_opts: Vec<(&str, &str)> = styles.iter().map(|s| (s.as_str(), s.as_str())).collect();
        crate::rtl::label(ui, crate::i18n::tr(&app.ui.language, "Font Style:"));
        combo(ui, d, "toStyle", &st_opts);
        ui.end_row();
    });
    ui.label(crate::rtl::widget(
        ui,
        egui::RichText::new(crate::i18n::tr(&app.ui.language, "OK changes all: text and paragraph/character styles."))
            .color(crate::theme::Tokens::get(ui.ctx()).text_dim)
            .size(11.0),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_params() {
        let f = command_fields("{name, type: custom|lastPageNumber|chapterNumber, text?, rule?: {on, weight}, flag?: bool} — creates");
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["name", "type", "text", "rule", "flag"]);
        assert_eq!(f[1].choices, ["custom", "lastPageNumber", "chapterNumber"]);
        assert!(!f[0].optional && f[2].optional);
        assert!(f[4].boolean);
        assert!(command_fields("{}").is_empty());
        assert!(command_fields("no params").is_empty());
        // A parenthetical after the key describes the value.
        let f = command_fields("{angle (degrees, CCW), ids?}");
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["angle", "ids"]);
        assert!(!f[0].optional && f[1].optional);
        assert_eq!(f[0].hint, "degrees, CCW");
        let f = command_fields("{scaleX? (%), ref?: 0..8, ids?: parent items (default: all)}");
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["scaleX", "ref", "ids"]);
        assert!(f.iter().all(|f| f.optional));
        assert_eq!(f[0].hint, "%");
        assert_eq!(f[2].hint, "parent items (default: all)");
        // Every command with a "…" label parses without panicking.
        for c in designcraft_engine::command_specs() {
            let _ = command_fields(c.params);
        }
    }

    #[test]
    fn character_style_options_dialog_edits_style() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.character.create", json!({"name": "TestChar"})).unwrap();

        let mut d = Dialog::new("characterStyleOptions", json!({"name": "TestChar"}));
        d.fields.insert("rename".into(), json!("RedHeader"));
        d.fields.insert("c.size".into(), json!(16.0));
        d.fields.insert("c.capitalization".into(), json!("allCaps"));
        d.fields.insert("c.fill".into(), json!("[Black]"));
        d.fields.insert("x.tag".into(), json!("strong"));
        d.fields.insert("x.class".into(), json!("callout"));
        app.ui.dialog = Some(d);

        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let cs = st.doc.styles.char_style("RedHeader").expect("style should be renamed");
        assert_eq!(cs.chars.size, Some(16.0));
        assert_eq!(cs.chars.capitalization, Some(designcraft_doc::Capitalization::AllCaps));
        assert_eq!(cs.chars.fill.as_deref(), Some("[Black]"));

        let et = st.doc.styles.export_tag("RedHeader", true).expect("export tag should be set");
        assert_eq!(et.tag, "strong");
        assert_eq!(et.class, "callout");
    }

    #[test]
    fn paragraph_style_options_dialog_edits_style_language() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.paragraph.create", json!({"name": "CuerpoNoticia"})).unwrap();

        let mut d = Dialog::new("paragraphStyleOptions", json!({"name": "CuerpoNoticia"}));
        d.fields.insert("c.language".into(), json!("Spanish"));
        d.fields.insert("p.hyphenate".into(), json!(true));
        app.ui.dialog = Some(d);

        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let ps = st.doc.styles.para("CuerpoNoticia").expect("style should exist");
        assert_eq!(ps.chars.language.as_deref(), Some("Spanish"));
        assert_eq!(ps.para.hyphenate, Some(true));
    }

    #[test]
    fn paragraph_style_options_dialog_edits_justification_and_alignment() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.paragraph.create", json!({"name": "EditorialBody"})).unwrap();

        let mut d = Dialog::new("paragraphStyleOptions", json!({"name": "EditorialBody"}));
        d.fields.insert("p.align".into(), serde_json::to_value(designcraft_doc::Align::LeftJustified).unwrap());
        d.fields.insert("p.wordSpaceMin".into(), json!(0.85));
        d.fields.insert("p.wordSpaceDesired".into(), json!(1.0));
        d.fields.insert("p.wordSpaceMax".into(), json!(1.25));
        d.fields.insert("p.letterSpaceMin".into(), json!(-0.02));
        d.fields.insert("p.letterSpaceDesired".into(), json!(0.0));
        d.fields.insert("p.letterSpaceMax".into(), json!(0.05));
        d.fields.insert("p.glyphScaleMin".into(), json!(0.98));
        d.fields.insert("p.glyphScaleDesired".into(), json!(1.0));
        d.fields.insert("p.glyphScaleMax".into(), json!(1.02));
        d.fields.insert("p.autoLeading".into(), json!(1.25));
        d.fields.insert("p.singleWordJustify".into(), serde_json::to_value(designcraft_doc::Align::Left).unwrap());
        d.fields.insert("p.composer".into(), json!("paragraph"));
        d.fields.insert("p.kashidas".into(), json!(false));
        d.fields.insert("p.koreanCharBreaks".into(), json!(true));
        app.ui.dialog = Some(d);

        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let ps = st.doc.styles.para("EditorialBody").expect("style should exist");
        assert_eq!(ps.para.align, Some(designcraft_doc::Align::LeftJustified));
        assert_eq!(ps.para.word_space_min, Some(0.85));
        assert_eq!(ps.para.word_space_desired, Some(1.0));
        assert_eq!(ps.para.word_space_max, Some(1.25));
        assert_eq!(ps.para.letter_space_min, Some(-0.02));
        assert_eq!(ps.para.letter_space_desired, Some(0.0));
        assert_eq!(ps.para.letter_space_max, Some(0.05));
        assert_eq!(ps.para.glyph_scale_min, Some(0.98));
        assert_eq!(ps.para.glyph_scale_desired, Some(1.0));
        assert_eq!(ps.para.glyph_scale_max, Some(1.02));
        assert_eq!(ps.para.auto_leading, Some(1.25));
        assert_eq!(ps.para.single_word_justify, Some(designcraft_doc::Align::Left));
        assert_eq!(ps.para.composer, Some(designcraft_doc::Composer::Paragraph));
        assert_eq!(ps.para.kashidas, Some(false));
        assert_eq!(ps.para.korean_char_breaks, Some(true));
    }

    #[test]
    fn character_style_options_dialog_edits_advanced_formats_and_opentype() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.character.create", json!({"name": "FullCharStyle"})).unwrap();

        let mut d = Dialog::new("characterStyleOptions", json!({"name": "FullCharStyle"}));
        d.fields.insert("c.hScale".into(), json!(1.1));
        d.fields.insert("c.vScale".into(), json!(0.9));
        d.fields.insert("c.baselineShift".into(), json!(3.0));
        d.fields.insert("c.skew".into(), json!(15.0));
        d.fields.insert("c.underline".into(), json!(true));
        d.fields.insert("c.underlineWeight".into(), json!(1.5));
        d.fields.insert("c.underlineOffset".into(), json!(2.0));
        d.fields.insert("c.underlineColor".into(), json!("[Black]"));
        d.fields.insert("c.underlineTint".into(), json!(0.8));
        d.fields.insert("c.strikethrough".into(), json!(true));
        d.fields.insert("c.strikethroughWeight".into(), json!(1.0));
        d.fields.insert("c.strikethroughOffset".into(), json!(4.0));
        d.fields.insert("c.stroke".into(), json!("[Black]"));
        d.fields.insert("c.strokeWeight".into(), json!(0.75));
        d.fields.insert("c.strokeTint".into(), json!(0.5));
        d.fields.insert("c.otfFeatures".into(), json!(["smcp", "ss01"]));
        app.ui.dialog = Some(d);

        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let cs = st.doc.styles.char_style("FullCharStyle").expect("style should exist");
        assert_eq!(cs.chars.h_scale, Some(1.1));
        assert_eq!(cs.chars.v_scale, Some(0.9));
        assert_eq!(cs.chars.baseline_shift, Some(3.0));
        assert_eq!(cs.chars.skew, Some(15.0));
        assert_eq!(cs.chars.underline, Some(true));
        assert_eq!(cs.chars.underline_weight, Some(Some(1.5)));
        assert_eq!(cs.chars.underline_offset, Some(Some(2.0)));
        assert_eq!(cs.chars.underline_color.as_deref(), Some("[Black]"));
        assert_eq!(cs.chars.strikethrough, Some(true));
        assert_eq!(cs.chars.strikethrough_weight, Some(Some(1.0)));
        assert_eq!(cs.chars.strikethrough_offset, Some(Some(4.0)));
        assert_eq!(cs.chars.stroke.as_deref(), Some("[Black]"));
        assert_eq!(cs.chars.stroke_weight, Some(0.75));
        assert_eq!(cs.chars.otf_features, Some(vec!["smcp".to_string(), "ss01".to_string()]));
    }

    #[test]
    fn paragraph_style_options_dialog_edits_all_extended_paragraph_options() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.paragraph.create", json!({"name": "FullParaStyle"})).unwrap();

        let mut d = Dialog::new("paragraphStyleOptions", json!({"name": "FullParaStyle"}));
        d.fields.insert("p.direction".into(), serde_json::to_value(designcraft_doc::TextDirection::RightToLeft).unwrap());
        d.fields.insert("p.lastLineIndent".into(), json!(12.0));
        d.fields.insert("p.gridAlign".into(), serde_json::to_value(designcraft_doc::GridAlign::AllLines).unwrap());
        d.fields.insert("p.balanceRagged".into(), json!(true));
        d.fields.insert("p.keepWithNext".into(), json!(3));
        d.fields.insert("p.keepLinesTogether".into(), json!(true));
        d.fields.insert("p.keepAllLines".into(), json!(false));
        d.fields.insert("p.keepFirst".into(), json!(3));
        d.fields.insert("p.keepLast".into(), json!(3));
        d.fields.insert("p.startParagraph".into(), serde_json::to_value(designcraft_doc::StartParagraph::NextColumn).unwrap());
        d.fields.insert(
            "p.ruleAbove".into(),
            json!({
                "on": true,
                "weight": 2.0,
                "offset": 4.0,
                "leftIndent": 10.0,
                "rightIndent": 10.0,
                "columnWidth": true,
                "color": "[Black]",
                "tint": 1.0
            }),
        );
        d.fields.insert("p.shadingOn".into(), json!(true));
        d.fields.insert("p.shadingColor".into(), json!("[Black]"));
        d.fields.insert("p.shadingTint".into(), json!(0.25));
        d.fields.insert("p.shadingOffsets".into(), json!([2.0, 4.0, 2.0, 4.0]));
        d.fields.insert("p.borderOn".into(), json!(true));
        d.fields.insert("p.borderColor".into(), json!("[Black]"));
        d.fields.insert("p.borderWeights".into(), json!([1.0, 1.0, 1.0, 1.0]));
        d.fields.insert("p.borderTint".into(), json!(1.0));
        d.fields.insert("p.spanColumns".into(), serde_json::to_value(designcraft_doc::SpanColumns::Span(0)).unwrap());
        d.fields.insert("p.listType".into(), serde_json::to_value(designcraft_doc::ListType::Numbers).unwrap());
        d.fields.insert("p.numberStyle".into(), serde_json::to_value(designcraft_doc::NumberStyle::UpperRoman).unwrap());
        d.fields.insert("p.startAt".into(), json!(5));
        d.fields.insert("p.bulletCharStyle".into(), json!("RedBullet"));
        d.fields.insert("p.bulletSvg".into(), json!("circle"));
        d.fields.insert("p.bulletSvgSize".into(), json!([10.0, 10.0]));
        d.fields.insert("p.bulletSvgOffset".into(), json!(2.0));
        d.fields.insert("p.hyphZone".into(), json!(18.0));
        d.fields.insert("p.hyphCapitalized".into(), json!(false));
        d.fields.insert("p.hyphLastWord".into(), json!(false));
        d.fields.insert("p.hyphAcrossColumn".into(), json!(false));
        d.fields.insert("p.hyphWeight".into(), json!(0.8));
        app.ui.dialog = Some(d);

        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let ps = st.doc.styles.para("FullParaStyle").expect("style should exist");
        assert_eq!(ps.para.direction, Some(designcraft_doc::TextDirection::RightToLeft));
        assert_eq!(ps.para.last_line_indent, Some(12.0));
        assert_eq!(ps.para.grid_align, Some(designcraft_doc::GridAlign::AllLines));
        assert_eq!(ps.para.balance_ragged, Some(true));
        assert_eq!(ps.para.keep_with_next, Some(3));
        assert_eq!(ps.para.keep_lines_together, Some(true));
        assert_eq!(ps.para.keep_all_lines, Some(false));
        assert_eq!(ps.para.keep_first, Some(3));
        assert_eq!(ps.para.keep_last, Some(3));
        assert_eq!(ps.para.start_paragraph, Some(designcraft_doc::StartParagraph::NextColumn));
        assert_eq!(ps.para.rule_above.as_ref().map(|r| r.on), Some(true));
        assert_eq!(ps.para.rule_above.as_ref().map(|r| r.weight), Some(2.0));
        assert_eq!(ps.para.shading_on, Some(true));
        assert_eq!(ps.para.shading_color.as_deref(), Some("[Black]"));
        assert_eq!(ps.para.shading_offsets, Some([2.0, 4.0, 2.0, 4.0]));
        assert_eq!(ps.para.border_on, Some(true));
        assert_eq!(ps.para.border_color.as_deref(), Some("[Black]"));
        assert_eq!(ps.para.span_columns, Some(designcraft_doc::SpanColumns::Span(0)));
        assert_eq!(ps.para.list_type, Some(designcraft_doc::ListType::Numbers));
        assert_eq!(ps.para.number_style, Some(designcraft_doc::NumberStyle::UpperRoman));
        assert_eq!(ps.para.start_at, Some(Some(5)));
        assert_eq!(ps.para.bullet_char_style.as_deref(), Some("RedBullet"));
        assert_eq!(ps.para.bullet_svg.as_deref(), Some("circle"));
        assert_eq!(ps.para.bullet_svg_size, Some([10.0, 10.0]));
        assert_eq!(ps.para.bullet_svg_offset, Some(2.0));
        assert_eq!(ps.para.hyph_zone, Some(18.0));
        assert_eq!(ps.para.hyph_capitalized, Some(false));
        assert_eq!(ps.para.hyph_last_word, Some(false));
        assert_eq!(ps.para.hyph_across_column, Some(false));
        assert_eq!(ps.para.hyph_weight, Some(0.8));
    }

    #[test]
    fn style_options_dialog_edits_shortcut() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.paragraph.create", json!({"name": "ShortcutPara"})).unwrap();
        app.run("style.character.create", json!({"name": "ShortcutChar"})).unwrap();

        let mut dp = Dialog::new("paragraphStyleOptions", json!({"name": "ShortcutPara"}));
        dp.fields.insert("shortcut".into(), json!("Ctrl+Num 1"));
        app.ui.dialog = Some(dp);
        confirm(&mut app).unwrap();

        let mut dc = Dialog::new("characterStyleOptions", json!({"name": "ShortcutChar"}));
        dc.fields.insert("shortcut".into(), json!("Ctrl+Shift+Num 2"));
        app.ui.dialog = Some(dc);
        confirm(&mut app).unwrap();

        let st = app.session.active().unwrap();
        let ps = st.doc.styles.para("ShortcutPara").expect("style should exist");
        assert_eq!(ps.shortcut, "Ctrl+Num 1");
        let cs = st.doc.styles.char_style("ShortcutChar").expect("char style should exist");
        assert_eq!(cs.shortcut, "Ctrl+Shift+Num 2");
    }

    #[test]
    fn paragraph_style_options_dialog_renders_all_sections_cleanly() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.paragraph.create", json!({"name": "TestPara"})).unwrap();

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        let sections = [
            "general",
            "chars",
            "adv_chars",
            "indents",
            "span",
            "keep",
            "rules",
            "shading",
            "border",
            "bullets",
            "hyph",
            "justify",
            "nested",
            "grep",
            "color",
            "opentype",
            "underline",
            "strikethrough",
            "export",
            "copyfit",
        ];

        for sec in sections {
            let mut d = Dialog::new("paragraphStyleOptions", json!({"name": "TestPara"}));
            d.fields.insert("section".into(), json!(sec));
            if sec == "rules" {
                d.fields.insert(
                    "p.ruleAbove".into(),
                    json!({"on": true, "weight": 1.0, "offset": 0.0, "leftIndent": 0.0, "rightIndent": 0.0, "columnWidth": true, "color": "[Black]", "tint": 1.0}),
                );
            } else if sec == "shading" {
                d.fields.insert("p.shadingOn".into(), json!(true));
            } else if sec == "border" {
                d.fields.insert("p.borderOn".into(), json!(true));
            } else if sec == "bullets" {
                d.fields.insert("p.listType".into(), json!("bullets"));
                d.fields.insert("p.bulletSvg".into(), json!("custom"));
            } else if sec == "keep" {
                d.fields.insert("p.keepLinesTogether".into(), json!(true));
                d.fields.insert("p.keepAllLines".into(), json!(false));
            } else if sec == "span" {
                d.fields.insert("p.spanColumns".into(), serde_json::to_value(designcraft_doc::SpanColumns::Split(3)).unwrap());
            }
            app.ui.dialog = Some(d);

            let input =
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                super::show(&mut app, ui.ctx());
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn character_style_options_dialog_renders_all_sections_cleanly() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();
        app.run("style.character.create", json!({"name": "TestChar"})).unwrap();

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        let sections = ["general", "chars", "adv_chars", "indents", "color", "opentype", "underline", "strikethrough", "export", "copyfit"];

        for sec in sections {
            let mut d = Dialog::new("characterStyleOptions", json!({"name": "TestChar"}));
            d.fields.insert("section".into(), json!(sec));
            app.ui.dialog = Some(d);

            let input =
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                super::show(&mut app, ui.ctx());
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn export_pdf_dialog_opens_renders_and_confirms() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();

        // Calling app.exportPdf without path opens the dialog
        app.run("app.exportPdf", json!({})).unwrap();
        assert!(app.ui.dialog.is_some());
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.id, "exportPdf");

        // Renders cleanly
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut out = ctx.run_ui(input.clone(), |ui| {
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();

        // Confirm executes without panicking
        let res = super::confirm(&mut app);
        assert!(res.is_ok());

        // Test with PDF/X-1a:2001
        app.run("app.exportPdf", json!({})).unwrap();
        if let Some(d) = app.ui.dialog.as_mut() {
            d.fields.insert("standard".into(), json!("x1a2001"));
            d.fields.insert("preset".into(), json!("pdfx1a"));
        }
        let mut out = ctx.run_ui(input, |ui| {
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();
        let res = super::confirm(&mut app);
        assert!(res.is_ok());

        // Test with pick_save desktop behavior and presets
        let picked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let picked_clone = picked.clone();
        app.services.pick_save = Some(Box::new(move |name: &str| {
            picked_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Some(format!("/tmp/{name}"))
        }));

        // Calling app.exportPdfPreset with pdfx1a opens the dialog with pdfx1a preset
        app.run("app.exportPdfPreset", json!({"preset": "pdfx1a"})).unwrap();
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.s("preset"), "pdfx1a");
        assert_eq!(d.s("standard"), "x1a2001");
        let res = super::confirm(&mut app);
        assert!(res.is_ok());
        assert!(picked.load(std::sync::atomic::Ordering::SeqCst));

        // Test with PDF/X-4
        picked.store(false, std::sync::atomic::Ordering::SeqCst);
        app.run("app.exportPdfPreset", json!({"preset": "pdfx4"})).unwrap();
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.s("preset"), "pdfx4");
        assert_eq!(d.s("standard"), "x4");
        let res = super::confirm(&mut app);
        assert!(res.is_ok());
        assert!(picked.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn export_png_dialog_opens_renders_and_confirms() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();

        // Calling app.exportPng without path opens the dialog
        app.run("app.exportPng", json!({})).unwrap();
        assert!(app.ui.dialog.is_some());
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.id, "exportPng");

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();

        let res = super::confirm(&mut app);
        assert!(res.is_ok());
    }

    #[test]
    fn save_as_and_save_copy_dialogs_open_render_and_confirm() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 612.0, "height": 792.0})).unwrap();

        // Save As
        app.run("app.saveDialog", json!({})).unwrap();
        assert!(app.ui.dialog.is_some());
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.id, "saveAs");

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut out = ctx.run_ui(input.clone(), |ui| {
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();

        let res = super::confirm(&mut app);
        assert!(res.is_ok());

        // Save a Copy
        app.run("app.saveCopyDialog", json!({})).unwrap();
        assert!(app.ui.dialog.is_some());
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.id, "saveCopy");

        let mut out = ctx.run_ui(input, |ui| {
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();

        let res = super::confirm(&mut app);
        assert!(res.is_ok());
    }
    #[test]
    fn copyfit_dialog_saves_displayed_limits_and_bottom_opt_in_for_both_style_kinds() {
        for kind in ["paragraph", "character"] {
            let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({})).unwrap();
            app.run(&format!("style.{kind}.create"), json!({"name":"Body", "chars":{"size":12}})).unwrap();
            let mut dialog = Dialog::new(&format!("{kind}StyleOptions"), json!({"name":"Body"}));
            dialog.fields.insert("section".into(), json!("copyfit"));
            app.ui.dialog = Some(dialog);
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx, "");
            let input =
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut output = ctx.run_ui(input, |ui| super::show(&mut app, ui.ctx()));
            output.textures_delta.clear();
            let dialog = app.ui.dialog.as_mut().unwrap();
            dialog.fields.insert("x.cf_size".into(), json!(true));
            dialog.fields.insert("x.cf_trk".into(), json!(true));
            dialog.fields.insert("x.cf_bottom".into(), json!(true));
            confirm(&mut app).unwrap();
            let styles = &app.session.active().unwrap().doc.styles;
            let cf = if kind == "paragraph" {
                styles.para("Body").unwrap().copyfit.as_ref().unwrap()
            } else {
                styles.char_style("Body").unwrap().copyfit.as_ref().unwrap()
            };
            assert!(cf.close_bottom);
            let size = cf.size_flexibility.as_ref().unwrap();
            assert!((size.start - 11.88).abs() < 1e-9);
            assert!((size.end - 12.12).abs() < 1e-9);
            assert_eq!(cf.tracking_flexibility, Some(-5.0..5.0));
        }
    }
    #[test]
    fn copyfit_parameters_typed_in_real_dialog_survive_ok_and_reopening() {
        use egui_kittest::{Harness, kittest::Queryable};
        for kind in ["paragraph", "character"] {
            let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({})).unwrap();
            app.run(&format!("style.{kind}.create"), json!({"name":"Body","chars":{"size":12}})).unwrap();
            app.run(&format!("style.{kind}.edit"), json!({"name":"Body","copyfit":{"size_min":11.88,"size_max":12.12,"trk_min":-5,"trk_max":5}}))
                .unwrap();
            app.ui.dialog = Some(Dialog::new(&format!("{kind}StyleOptions"), json!({"name":"Body","section":"copyfit"})));
            let mut initialized = false;
            let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
                move |ui, app| {
                    if !initialized {
                        crate::theme::install_fonts(ui.ctx(), "");
                        initialized = true;
                        ui.ctx().request_repaint();
                        return;
                    }
                    super::show(app, ui.ctx());
                },
                app,
            );
            h.run();
            for (old, new) in [(11.88, "11.93"), (12.12, "12.07"), (-5.0, "-2.5"), (5.0, "2.5")] {
                let expected = if old > 10.0 { format!("{old} pt") } else { format!("{old}") };
                h.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.value().as_deref() == Some(expected.as_str())).focus();
                h.run();
                h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
                h.run();
                h.event(egui::Event::Text(new.into()));
                h.run();
            }
            h.get_by_label("  OK  ").click();
            h.run();
            assert!(h.state().ui.dialog.is_none());
            let styles = &h.state().session.active().unwrap().doc.styles;
            let policy = if kind == "paragraph" {
                styles.para("Body").unwrap().copyfit.as_ref().unwrap()
            } else {
                styles.char_style("Body").unwrap().copyfit.as_ref().unwrap()
            };
            assert_eq!(policy.size_flexibility, Some(11.93..12.07));
            assert_eq!(policy.tracking_flexibility, Some(-2.5..2.5));
            h.state_mut().ui.dialog = Some(Dialog::new(&format!("{kind}StyleOptions"), json!({"name":"Body","section":"copyfit"})));
            h.run();
            assert_eq!(h.state().ui.dialog.as_ref().unwrap().n("x.cf_sz_min"), Some(11.93));
            assert_eq!(h.state().ui.dialog.as_ref().unwrap().n("x.cf_trk_max"), Some(2.5));
        }
    }
    #[test]
    fn rejected_editor_style_save_retains_entered_parameters_and_document() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({})).unwrap();
        app.run("style.paragraph.create", json!({"name":"Body"})).unwrap();
        let original = app.session.active().unwrap().doc.clone();
        app.session.editor_mode = true;
        app.ui.dialog = Some(Dialog::new(
            "paragraphStyleOptions",
            json!({"name":"Body","x.cf_init":true,"x.cf_size":true,"x.cf_sz_min":11.91,"x.cf_sz_max":12.09}),
        ));
        assert!(confirm(&mut app).is_err());
        let dialog = app.ui.dialog.as_ref().unwrap();
        assert_eq!(dialog.n("x.cf_sz_min"), Some(11.91));
        assert!(dialog.fields.get("error").and_then(Value::as_str).is_some());
        assert!(std::sync::Arc::ptr_eq(&original, &app.session.active().unwrap().doc));
    }
    #[test]
    fn editorial_style_dialog_explains_readonly_and_does_not_discard_on_ok() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({})).unwrap();
        app.run("style.paragraph.create", json!({"name":"Body"})).unwrap();
        app.session.restrict_to_editor();
        let original = app.session.active().unwrap().doc.clone();
        app.ui.dialog = Some(Dialog::new("paragraphStyleOptions", json!({"name":"Body","section":"copyfit"})));
        let mut initialized = false;
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
            move |ui, app| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx(), "");
                    initialized = true;
                    ui.ctx().request_repaint();
                    return;
                }
                super::show(app, ui.ctx());
            },
            app,
        );
        h.run();
        h.get_by_label("Configure style limits in designer mode");
        h.get_by_label("  OK  ").click();
        h.run();
        assert!(h.state().ui.dialog.is_some());
        assert!(std::sync::Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
    }
    #[test]
    fn inherited_copyfit_form_keeps_inheritance_until_edited_and_can_disable_ranges() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({})).unwrap();
        app.run("style.paragraph.create", json!({"name":"Parent","chars":{"size":12}})).unwrap();
        app.run("style.paragraph.edit", json!({"name":"Parent","copyfit":{"size_min":11.8,"size_max":12.2,"trk_min":-2,"trk_max":2}})).unwrap();
        app.run("style.paragraph.create", json!({"name":"Child","basedOn":"Parent"})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        app.ui.dialog = Some(Dialog::new("paragraphStyleOptions", json!({"name":"Child","section":"copyfit"})));
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() },
            |ui| super::show(&mut app, ui.ctx()),
        );
        output.textures_delta.clear();
        assert_eq!(app.ui.dialog.as_ref().unwrap().n("x.cf_sz_min"), Some(11.8));
        let dialog = app.ui.dialog.clone();
        confirm(&mut app).unwrap();
        assert!(app.session.active().unwrap().doc.styles.para("Child").unwrap().copyfit.is_none());
        app.ui.dialog = dialog;
        let d = app.ui.dialog.as_mut().unwrap();
        d.fields.insert("x.cf_size".into(), json!(false));
        d.fields.insert("x.cf_trk".into(), json!(false));
        confirm(&mut app).unwrap();
        let policy = app.session.active().unwrap().doc.styles.resolve_copyfit("Child", designcraft_doc::NO_CHAR_STYLE).unwrap();
        assert!(policy.size_flexibility.is_none());
        assert!(policy.tracking_flexibility.is_none());
    }

    #[test]
    fn editorial_overview_dialog_renders_and_filters() {
        let mut app = crate::DesignApp::new(designcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"pages": 4})).unwrap();
        app.run("page.editorial.set", json!({"page": 1, "section": "Portada", "assignedTo": "Redactor1", "status": "needsReview"})).unwrap();
        app.run("page.editorial.approve", json!({"page": 2, "approved": true, "by": "JefeDeCierre"})).unwrap();

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, "");
        app.ui.dialog = Some(Dialog::new("editorialOverview", json!({"filter": "all"})));

        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() },
            |ui| super::show(&mut app, ui.ctx()),
        );
        output.textures_delta.clear();
        assert!(app.ui.dialog.is_some());

        // Switch filter to "approved" and render again
        if let Some(d) = app.ui.dialog.as_mut() {
            d.fields.insert("filter".into(), json!("approved"));
        }
        let mut output2 = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() },
            |ui| super::show(&mut app, ui.ctx()),
        );
        output2.textures_delta.clear();
        assert!(app.ui.dialog.is_some());
    }
}
