//! Frame-driven asynchronous state machine for Photographic Retouching (AIE).
//!
//! Works seamlessly on both Native Desktop and WebAssembly (browser):
//! - Runs each mathematical phase of ISO 12647-3 in a separate UI tick,
//!   yielding back to the browser/egui event loop between stages so the UI
//!   never freezes and repaints smoothly at 60 FPS.
//! - Emits live progress events across all 5 phases with animated ProgressBar.
//! - Commits document assets atomically with full Undo/Redo (Ctrl+Z) support.

use serde_json::{Value, json};
use std::sync::Arc;

use designcraft_doc::{Asset, AssetId, Content, ItemId};
use designcraft_images::{
    PhotoHistogram, RotaryPreset, ToneCurve, apply_tone_curve, apply_unsharp_mask, decode_rgba, encode_cmyk_tiff, encode_png, measure_tac,
    separate_to_rotary_cmyk,
};

use crate::DesignApp;
use crate::theme::{Tokens, semibold};

#[derive(Clone, Debug)]
pub struct RetouchStats {
    pub preset_name: String,
    pub scene: String,
    pub width: u32,
    pub height: u32,
    pub max_tac: f32,
    pub avg_tac: f32,
    pub exceeding_pixels: u64,
    pub total_pixels: u64,
    pub is_cmyk: bool,
    pub asset_name: String,
    pub elapsed_micros: u128,
}

#[derive(Clone, Debug)]
pub struct RetouchProgressState {
    pub title: String,
    pub stage_index: usize,
    pub total_stages: usize,
    pub stage_name: String,
    pub progress: f32,
    pub elapsed_ms: f64,
    pub start_time: f64,
    pub done: bool,
    pub error: Option<String>,
    pub stats: Option<RetouchStats>,
}

pub struct RetouchTask {
    pub target_id: ItemId,
    pub orig_aid: AssetId,
    pub stem: String,
    pub asset_name: String,
    pub data: Arc<Vec<u8>>,
    pub preset: RotaryPreset,
    pub to_cmyk: bool,
    pub clean: bool,
    pub auto_levels: bool,
    pub sharpen: bool,
    pub step: u8,
    pub rgba: Option<designcraft_images::image::RgbaImage>,
    pub hist: Option<PhotoHistogram>,
}

fn parse_preset(val: Option<&str>) -> RotaryPreset {
    match val.unwrap_or("coldset").to_ascii_lowercase().as_str() {
        "improved" | "sc" | "improvednewspaper" | "improvednewspapersc" => RotaryPreset::ImprovedNewspaperSC,
        "heatset" | "magazine" | "heatsetmagazine" => RotaryPreset::HeatsetMagazine,
        "sheetfed" | "commercial" | "commercialsheetfed" | "offset" => RotaryPreset::CommercialSheetfed,
        _ => RotaryPreset::ColdsetNewspaper,
    }
}

/// Start an asynchronous retouch operation with frame-driven progress reporting.
pub fn start_retouch(app: &mut DesignApp, params: &Value) -> Option<Result<Value, String>> {
    let preset = parse_preset(params.get("preset").and_then(Value::as_str));
    let to_cmyk = !matches!(params.get("format").and_then(Value::as_str).unwrap_or("cmyk"), "rgba" | "png" | "rgb");
    let clean = params.get("clean").and_then(Value::as_bool).unwrap_or(false);
    let auto_levels =
        if clean { false } else { params.get("autoLevels").or_else(|| params.get("auto_levels")).and_then(Value::as_bool).unwrap_or(true) };
    let sharpen = if clean { false } else { params.get("sharpen").and_then(Value::as_bool).unwrap_or(true) };

    let st = match app.session.active() {
        Some(s) => s,
        None => return Some(Err("No hay ningún documento activo".into())),
    };

    let target_id = if let Some(id) = params.get("id").and_then(Value::as_u64) {
        ItemId(id)
    } else if let Some(ids) = params.get("ids").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_u64) {
        ItemId(ids)
    } else if let Some(first) = st.selection.items.first().copied() {
        first
    } else {
        return Some(Err("Selecciona una foto para retocar".into()));
    };

    let orig_aid = match st.doc.item(target_id).map(|it| &it.content) {
        Some(Content::Graphic(g)) => g.asset,
        _ => return Some(Err("El elemento seleccionado no contiene una imagen fotográfica".into())),
    };

    let asset = match st.doc.assets.get(&orig_aid).cloned() {
        Some(a) => a,
        None => return Some(Err(format!("No se encontró el asset de la foto {}", target_id.0))),
    };

    if asset.data.is_empty() {
        return Some(Err(format!("La foto {} no tiene datos cargados", asset.name)));
    }

    if designcraft_images::is_pdf(&asset.data) || designcraft_images::is_svg(&asset.data) {
        return Some(Err(format!("El archivo {} es un gráfico vectorial; el retoque AIE se aplica a fotos rasterizadas", asset.name)));
    }

    let stem = std::path::Path::new(&asset.name).file_stem().and_then(|s| s.to_str()).unwrap_or("foto").to_string();

    let now = crate::now_ms();
    let title = if clean { format!("Separación CMYK Pura: {}", asset.name) } else { format!("Retoque para Rotativa: {}", asset.name) };

    app.retouch_progress = Some(RetouchProgressState {
        title,
        stage_index: 1,
        total_stages: 5,
        stage_name: "Iniciando motor de retoque...".into(),
        progress: 0.05,
        elapsed_ms: 0.0,
        start_time: now,
        done: false,
        error: None,
        stats: None,
    });

    app.retouch_task = Some(RetouchTask {
        target_id,
        orig_aid,
        stem,
        asset_name: asset.name.clone(),
        data: asset.data.clone(),
        preset,
        to_cmyk,
        clean,
        auto_levels,
        sharpen,
        step: 0,
        rgba: None,
        hist: None,
    });

    Some(Ok(json!({ "status": "started", "target": target_id.0 })))
}

/// Advance the retouch state machine by one step per frame, yielding between stages to maintain 60 FPS.
pub fn drain_retouch(app: &mut DesignApp, ctx: &egui::Context) {
    let Some(mut task) = app.retouch_task.take() else {
        return;
    };

    match task.step {
        0 => {
            // Step 0 -> prepare stage 1
            if let Some(ref mut p) = app.retouch_progress {
                p.stage_index = 1;
                p.progress = 0.15;
                p.stage_name = "1/5: Decodificando imagen y extrayendo canales de color...".into();
            }
            task.step = 1;
            app.retouch_task = Some(task);
            ctx.request_repaint();
        }
        1 => {
            // Step 1: Decode RGBA
            match decode_rgba(&task.data) {
                Some(rgba) => {
                    task.rgba = Some(rgba);
                    task.step = 2;
                    if let Some(ref mut p) = app.retouch_progress {
                        p.stage_index = 2;
                        p.progress = 0.35;
                        p.stage_name = "2/5: Analizando histograma de luminancia y contraste...".into();
                    }
                    app.retouch_task = Some(task);
                }
                None => {
                    if let Some(ref mut p) = app.retouch_progress {
                        p.error = Some(format!("No se pudo decodificar el formato de {}", task.asset_name));
                        p.done = true;
                        p.progress = 1.0;
                    }
                    app.status(format!("Error en retoque: no se pudo decodificar {}", task.asset_name));
                }
            }
            ctx.request_repaint();
        }
        2 => {
            // Step 2: PhotoHistogram
            let Some(ref rgba) = task.rgba else {
                return;
            };
            let hist = PhotoHistogram::from_rgba8(rgba);
            task.hist = Some(hist);
            task.step = 3;
            if let Some(ref mut p) = app.retouch_progress {
                p.stage_index = 3;
                p.progress = 0.55;
                p.stage_name = "3/5: Compensando ganancia de punto rotativa (TVI 28%)...".into();
            }
            app.retouch_task = Some(task);
            ctx.request_repaint();
        }
        3 => {
            // Step 3: Tone reproduction curve (auto-levels + dot-gain compensation)
            let (Some(hist), Some(rgba)) = (&task.hist, &mut task.rgba) else {
                return;
            };
            if task.auto_levels {
                let curve = ToneCurve::auto_levels_for_rotary(
                    hist,
                    task.preset.target_highlight_ink(),
                    task.preset.target_shadow_ink(),
                    task.preset.midtone_gamma(),
                );
                apply_tone_curve(rgba, &curve);
            }
            task.step = 4;
            if let Some(ref mut p) = app.retouch_progress {
                p.stage_index = 4;
                p.progress = 0.75;
                p.stage_name = if task.clean {
                    "4/5: Conservando enfoque original (modo puro sin sobreenfoque)...".into()
                } else if !task.sharpen {
                    "4/5: Enfoque omitido según configuración...".into()
                } else {
                    "4/5: Aplicando sobreenfoque sutil de luminancia (100–133 LPI)...".into()
                };
            }
            app.retouch_task = Some(task);
            ctx.request_repaint();
        }
        4 => {
            // Step 4: Unsharp masking
            let Some(ref mut rgba) = task.rgba else {
                return;
            };
            if task.sharpen {
                let usm = task.preset.unsharp_params();
                apply_unsharp_mask(rgba, &usm);
            }
            task.step = 5;
            if let Some(ref mut p) = app.retouch_progress {
                p.stage_index = 5;
                p.progress = 0.90;
                p.stage_name = if task.to_cmyk {
                    "5/5: Separando canales CMYK ISO 12647-3 (TAC ≤ 220%)...".into()
                } else {
                    "5/5: Generando derivado optimizado en memoria...".into()
                };
            }
            app.retouch_task = Some(task);
            ctx.request_repaint();
        }
        5 => {
            // Step 5: CMYK Separation / PNG Encoding and Commit Document
            let Some(ref rgba) = task.rgba else {
                return;
            };
            let (w, h) = rgba.dimensions();

            let (asset_bytes, tac_info) = if task.to_cmyk {
                let cmyk_raster = separate_to_rotary_cmyk(rgba, task.preset);
                let tac = measure_tac(&cmyk_raster, task.preset.max_tac_percent() as f32);
                match encode_cmyk_tiff(&cmyk_raster) {
                    Ok(b) => (b, Some(tac)),
                    Err(e) => {
                        if let Some(ref mut p) = app.retouch_progress {
                            p.error = Some(format!("Error al codificar TIFF CMYK: {e}"));
                            p.done = true;
                            p.progress = 1.0;
                        }
                        return;
                    }
                }
            } else {
                match encode_png(rgba) {
                    Ok(b) => (b, None),
                    Err(e) => {
                        if let Some(ref mut p) = app.retouch_progress {
                            p.error = Some(format!("Error al codificar PNG: {e}"));
                            p.done = true;
                            p.progress = 1.0;
                        }
                        return;
                    }
                }
            };

            let (ext, mime) = if task.to_cmyk { ("tif", "image/tiff") } else { ("png", "image/png") };
            let new_name = if task.to_cmyk { format!("{}_rotary_cmyk.{ext}", task.stem) } else { format!("{}_rotary.{ext}", task.stem) };

            // Atomically update document item and asset store with Undo/Redo
            let edit_result = app.session.edit(|d, _sel| {
                let aid = AssetId(d.alloc());
                let new_asset = Asset {
                    id: aid,
                    name: new_name.clone(),
                    mime: mime.into(),
                    link: None,
                    data: Arc::new(asset_bytes),
                    pixels: Some((w, h)),
                    page: 0,
                };
                d.assets.insert(aid, Arc::new(new_asset));
                if let Some(item) = d.item_mut(task.target_id)
                    && let Content::Graphic(ref mut g) = item.content
                {
                    g.asset = aid;
                }
                Ok(aid)
            });

            let total_ms = if let Some(ref p) = app.retouch_progress { crate::now_ms() - p.start_time } else { 0.0 };

            if let Some(ref mut p) = app.retouch_progress {
                p.progress = 1.0;
                p.stage_name = "¡Retoque completado con éxito!".into();
                p.done = true;
                p.elapsed_ms = total_ms;
                if let Ok(_new_aid) = edit_result {
                    p.stats = Some(RetouchStats {
                        preset_name: task.preset.name().to_string(),
                        scene: format!("{:?}", task.hist.as_ref().map(|h| h.scene).unwrap_or(designcraft_images::SceneType::Normal)),
                        width: w,
                        height: h,
                        max_tac: tac_info.as_ref().map(|t| t.max_tac).unwrap_or(0.0),
                        avg_tac: tac_info.as_ref().map(|t| t.avg_tac).unwrap_or(0.0),
                        exceeding_pixels: tac_info.as_ref().map(|t| t.exceeding_pixels).unwrap_or(0),
                        total_pixels: (w as u64) * (h as u64),
                        is_cmyk: task.to_cmyk,
                        asset_name: new_name,
                        elapsed_micros: (total_ms * 1000.0) as u128,
                    });
                }
            }

            app.canvas.shown = None;
            app.status(format!("✓ Retoque para rotativa finalizado en {:.1} ms", total_ms));
            ctx.request_repaint();
        }
        _ => {}
    }
}

/// Render the retouch progress modal dialog when an operation is active or finished.
pub fn show(app: &mut DesignApp, ctx: &egui::Context) {
    if app.retouch_progress.is_none() {
        return;
    }

    let t = Tokens::get(ctx);
    let mut close = false;

    egui::Modal::new(egui::Id::new("retouch_progress_modal")).show(ctx, |ui| {
        ui.set_min_width(420.0);
        ui.set_max_width(460.0);

        let Some(ref prog) = app.retouch_progress else { return };

        // Title and header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🛠").font(egui::FontId::proportional(20.0)));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Retoque Fotográfico para Prensa").font(semibold(15.0)).color(t.text));
                ui.label(egui::RichText::new("Norma ISO 12647-3 / WAN-IFRA Rotary").font(egui::FontId::proportional(11.0)).color(t.text_dim));
            });
        });

        ui.add_space(10.0);

        // Animated Progress Bar
        let bar_color = if prog.done && prog.error.is_none() {
            egui::Color32::from_rgb(46, 160, 67) // Green
        } else if prog.error.is_some() {
            egui::Color32::from_rgb(235, 50, 50) // Red
        } else {
            egui::Color32::from_rgb(33, 150, 243) // Blue
        };

        ui.add(egui::ProgressBar::new(prog.progress).show_percentage().animate(!prog.done).fill(bar_color));

        ui.add_space(8.0);

        // Current Stage and timer
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&prog.stage_name).font(semibold(12.0)).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let time_str =
                    if prog.elapsed_ms >= 1000.0 { format!("{:.2} s", prog.elapsed_ms / 1000.0) } else { format!("{:.0} ms", prog.elapsed_ms) };
                ui.label(egui::RichText::new(format!("⏱ {time_str}")).font(egui::FontId::proportional(11.0)).color(t.text_dim));
            });
        });

        ui.add_space(6.0);

        // 5-Phase Checklist visualization
        let stages = [
            ("Decodificación y extracción de canales", 1),
            ("Análisis de luminancia e histograma", 2),
            ("Compensación TVI (Ganancia de punto 28%)", 3),
            ("Enfoque Unsharp Mask (100–133 LPI)", 4),
            ("Separación CMYK y control de TAC ≤ 220%", 5),
        ];

        egui::Frame::group(ui.style()).fill(t.input).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            for (name, idx) in stages {
                ui.horizontal(|ui| {
                    let (icon, color) = if prog.stage_index > idx || (prog.done && prog.error.is_none()) {
                        ("✓", egui::Color32::from_rgb(46, 160, 67))
                    } else if prog.stage_index == idx && !prog.done {
                        ("▶", egui::Color32::from_rgb(33, 150, 243))
                    } else {
                        ("○", t.text_dim)
                    };
                    ui.label(egui::RichText::new(icon).color(color).font(semibold(12.0)));
                    ui.label(egui::RichText::new(name).font(egui::FontId::proportional(11.5)).color(if prog.stage_index == idx {
                        t.text
                    } else {
                        t.text_dim
                    }));
                });
            }
        });

        // Results Card when done
        if let Some(ref st) = prog.stats {
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).fill(t.panel).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.label(egui::RichText::new("📊 Resumen de Calidad para Prensa:").font(semibold(12.0)));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("• Cobertura Máx (TAC):").font(egui::FontId::proportional(11.0)).color(t.text_dim));
                    let tac_ok = st.max_tac <= 220.5;
                    let tac_color = if tac_ok { egui::Color32::from_rgb(46, 160, 67) } else { egui::Color32::from_rgb(235, 50, 50) };
                    ui.label(egui::RichText::new(format!("{:.1}% (Norma ≤ 220%)", st.max_tac)).color(tac_color).font(semibold(11.0)));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("• Escena detectada:").font(egui::FontId::proportional(11.0)).color(t.text_dim));
                    ui.label(egui::RichText::new(&st.scene).font(semibold(11.0)));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("• Dimensiones:").font(egui::FontId::proportional(11.0)).color(t.text_dim));
                    ui.label(egui::RichText::new(format!("{} × {} px", st.width, st.height)).font(egui::FontId::proportional(11.0)));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("• Formato generado:").font(egui::FontId::proportional(11.0)).color(t.text_dim));
                    ui.label(egui::RichText::new(if st.is_cmyk { "TIFF CMYK Prensa" } else { "PNG RGBA" }).font(egui::FontId::proportional(11.0)));
                });
            });
        }

        // Action Buttons
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if prog.done || prog.error.is_some() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui::RichText::new("Aceptar").font(semibold(12.0))).clicked() {
                        close = true;
                    }
                });
            } else {
                ui.label(
                    egui::RichText::new("Procesando paso a paso sin congelar la UI...").font(egui::FontId::proportional(11.0)).color(t.text_dim),
                );
            }
        });
    });

    if close {
        app.retouch_progress = None;
        app.retouch_task = None;
    }
}

/// Compact progress indicator rendered directly in the bottom status bar.
pub fn status_indicator(app: &DesignApp, ui: &mut egui::Ui) {
    let Some(ref prog) = app.retouch_progress else { return };
    let small = egui::FontId::proportional(10.5);

    ui.add_space(6.0);
    let color = if prog.done && prog.error.is_none() {
        egui::Color32::from_rgb(46, 160, 67)
    } else if prog.error.is_some() {
        egui::Color32::from_rgb(235, 50, 50)
    } else {
        egui::Color32::from_rgb(33, 150, 243)
    };

    let tooltip = if let Some(ref st) = prog.stats {
        format!(
            "{}\nFormato: {}\nTAC máx: {:.1}% (límite 220%)\nDimensiones: {}×{} px",
            prog.title,
            if st.is_cmyk { "TIFF CMYK (Prensa)" } else { "PNG RGBA" },
            st.max_tac,
            st.width,
            st.height
        )
    } else if let Some(ref err) = prog.error {
        format!("Error en retoque: {err}")
    } else {
        format!("{}\n{}: {:.0}%", prog.title, prog.stage_name, prog.progress * 100.0)
    };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("🛠 Retoque: {}", prog.stage_name)).font(small.clone()).color(color));
        ui.add(egui::ProgressBar::new(prog.progress).desired_width(80.0).show_percentage().fill(color));
    })
    .response
    .on_hover_text(tooltip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use designcraft_engine::Session;

    fn make_test_photo() -> Vec<u8> {
        let mut img = designcraft_images::image::RgbaImage::new(40, 40);
        for y in 0..40 {
            for x in 0..40 {
                let v = ((x + y) * 255 / 80) as u8;
                img.put_pixel(x, y, designcraft_images::image::Rgba([v, v, v, 255]));
            }
        }
        designcraft_images::encode_png(&img).unwrap()
    }

    fn base64_encode(data: &[u8]) -> String {
        const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(CHARS[(b0 >> 2) as usize] as char);
            out.push(CHARS[(((b0 & 3) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(CHARS[(b2 & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    #[test]
    fn retouch_state_machine_advances_and_commits_asset() {
        let mut session = Session::new();
        session.execute("file.new", &json!({})).unwrap();

        let png = make_test_photo();
        let placed = session
            .execute(
                "file.place",
                &json!({
                    "base64": base64_encode(&png),
                    "name": "editorial_test.png",
                    "x": 50,
                    "y": 50,
                    "width": 100,
                }),
            )
            .unwrap();

        let item_id = placed["id"].as_u64().unwrap();

        let mut app = DesignApp::new(session, crate::Services::default());
        app.session.active_mut().unwrap().selection.items = vec![ItemId(item_id)];

        // Start retouch through the intercepted UI command
        let res = app.run("image.retouchRotary", json!({"preset": "coldset", "format": "cmyk"}));
        assert!(res.is_ok());

        assert!(app.retouch_progress.is_some());
        assert!(app.retouch_task.is_some());
        let ctx = egui::Context::default();

        // Advance frames through the state machine
        for _ in 0..10 {
            drain_retouch(&mut app, &ctx);
            if let Some(ref p) = app.retouch_progress {
                if p.done {
                    assert!(p.stats.is_some());
                    let st = p.stats.as_ref().unwrap();
                    assert!(st.max_tac <= 220.5);
                    assert_eq!(st.exceeding_pixels, 0);
                    break;
                }
            }
        }

        assert!(app.retouch_progress.as_ref().unwrap().done, "Retouch task should be done");

        // Verify the document item was updated to the new CMYK asset
        let doc_st = app.session.active().unwrap();
        let item = doc_st.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = item.content {
            let asset = doc_st.doc.assets.get(&g.asset).unwrap();
            assert_eq!(asset.mime, "image/tiff");
        } else {
            panic!("Expected graphic item");
        }
    }
}
