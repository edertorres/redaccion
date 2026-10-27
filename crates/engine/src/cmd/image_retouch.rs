//! Photographic Retouching and Rotary Press Optimization (ISO 12647-3 / WAN-IFRA).
//!
//! Provides commands for Automatic Image Enhancement (AIE):
//! - Scene and histogram dynamic range analysis.
//! - Tone reproduction curves (auto-levels + TVI dot-gain compensation).
//! - Coarse halftone screen unsharp masking.
//! - ISO 12647-3 CMYK separation with strict TAC <= 220% clamping.

use serde_json::{Value, json};
use std::sync::Arc;

use designcraft_doc::{Asset, AssetId, Content};
use designcraft_images::{
    RetouchOptions, RotaryPreset, cmyk_raster_to_display_rgba, decode_cmyk_tiff, encode_cmyk_tiff, is_cmyk_tiff, measure_tac, optimize_for_rotary_press_with_options,
    separate_to_cmyk_with_profile,
};

use super::{CommandSpec, bad, cmd, has_selection, targets};
use crate::{Result, Session};

fn parse_preset(val: Option<&str>) -> Result<RotaryPreset> {
    match val.unwrap_or("coldset").to_ascii_lowercase().as_str() {
        "coldset" | "newspaper" | "coldsetnewspaper" | "iso12647-3" => Ok(RotaryPreset::ColdsetNewspaper),
        "improved" | "sc" | "improvednewspaper" | "improvednewspapersc" => Ok(RotaryPreset::ImprovedNewspaperSC),
        "heatset" | "magazine" | "heatsetmagazine" => Ok(RotaryPreset::HeatsetMagazine),
        "sheetfed" | "commercial" | "commercialsheetfed" | "offset" => Ok(RotaryPreset::CommercialSheetfed),
        other => Err(bad("image.retouchRotary", format!("unknown rotary preset `{other}`"))),
    }
}

fn retouch_rotary(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "image.retouchRotary";
    let preset = parse_preset(p.get("preset").and_then(Value::as_str))?;
    let profile_param = p.get("profile").and_then(Value::as_str);
    let to_cmyk = match p.get("format").and_then(Value::as_str).unwrap_or("cmyk") {
        "cmyk" | "tiff" | "tif" => true,
        "rgba" | "png" | "rgb" => false,
        other => return Err(bad(ID, format!("unknown output format `{other}` (expected rgba or cmyk)"))),
    };

    let clean = p.get("clean").and_then(Value::as_bool).unwrap_or(false);
    let auto_levels = if clean { false } else { p.get("autoLevels").or_else(|| p.get("auto_levels")).and_then(Value::as_bool).unwrap_or(true) };
    let sharpen = if clean { false } else { p.get("sharpen").and_then(Value::as_bool).unwrap_or(true) };
    let custom_sharpen = if sharpen {
        if let Some(amt) = p.get("sharpenAmount").or_else(|| p.get("sharpen_amount")).and_then(Value::as_f64) {
            let mut u = preset.unsharp_params();
            u.amount = (amt as f32).clamp(0.0, 3.0);
            Some(u)
        } else {
            None
        }
    } else {
        None
    };

    let retouch_opts = RetouchOptions { auto_levels, sharpen, custom_sharpen };

    let target_ids = targets(s, p)?;
    if target_ids.is_empty() {
        return Err(bad(ID, "no graphic items selected or specified"));
    }

    s.edit(|d, _sel| {
        let mut retouched_list = Vec::new();

        for item_id in target_ids {
            let orig_aid = match d.item(item_id).map(|it| &it.content) {
                Some(Content::Graphic(g)) => g.asset,
                _ => continue,
            };

            let Some(asset) = d.assets.get(&orig_aid).cloned() else {
                return Err(bad(ID, format!("asset for item {} not found", item_id.0)));
            };

            if asset.data.is_empty() {
                return Err(bad(ID, format!("asset {} has no data", asset.name)));
            }

            if designcraft_images::is_pdf(&asset.data) || designcraft_images::is_svg(&asset.data) {
                return Err(bad(ID, format!("asset {} is a vector format (PDF/SVG); only raster photos can be retouched", asset.name)));
            }

            let rgba = if is_cmyk_tiff(&asset.data) {
                decode_cmyk_tiff(&asset.data).map(|r| cmyk_raster_to_display_rgba(&r))
            } else {
                None
            }
            .or_else(|| designcraft_images::decode_rgba(&asset.data))
            .ok_or_else(|| bad(ID, format!("could not decode raster image from {}", asset.name)))?;

            let mut retouched_rgba = rgba;
            let report = optimize_for_rotary_press_with_options(&mut retouched_rgba, preset, &retouch_opts);
            let (w, h) = retouched_rgba.dimensions();

            let stem = std::path::Path::new(&asset.name).file_stem().and_then(|s| s.to_str()).unwrap_or("photo");

            let (new_aid, tac_info) = if to_cmyk {
                let cmyk_raster = separate_to_cmyk_with_profile(&retouched_rgba, preset, profile_param);
                let tac_metrics = measure_tac(&cmyk_raster, preset.max_tac_percent() as f32);
                let tiff_bytes = encode_cmyk_tiff(&cmyk_raster).map_err(|e| bad(ID, format!("failed to encode CMYK TIFF: {e}")))?;

                let aid = AssetId(d.alloc());
                let new_asset = Asset {
                    id: aid,
                    name: format!("{stem}_rotary_cmyk.tif"),
                    mime: "image/tiff".into(),
                    link: None,
                    data: Arc::new(tiff_bytes),
                    pixels: Some((w, h)),
                    page: 0,
                };
                d.assets.insert(aid, Arc::new(new_asset));
                let target_prof = profile_param.unwrap_or(if preset == RotaryPreset::ColdsetNewspaper { "ISOnewspaper26v4" } else { "Generic CMYK" });
                let resolved_icc = designcraft_color::cms::resolve_cmyk_profile(target_prof);
                let is_icc = resolved_icc.is_some();
                let profile_label = if is_icc { target_prof.to_string() } else { format!("{target_prof} (Generic Synthetic Model)") };
                (
                    aid,
                    Some(json!({
                        "maxTac": tac_metrics.max_tac,
                        "avgTac": tac_metrics.avg_tac,
                        "exceedingPixels": tac_metrics.exceeding_pixels,
                        "totalPixels": tac_metrics.total_pixels,
                        "profileUsed": profile_label,
                        "isIccProfile": is_icc,
                    })),
                )
            } else {
                let png_bytes = designcraft_images::encode_png(&retouched_rgba).map_err(|e| bad(ID, format!("failed to encode PNG: {e}")))?;

                let aid = AssetId(d.alloc());
                let new_asset = Asset {
                    id: aid,
                    name: format!("{stem}_rotary.png"),
                    mime: "image/png".into(),
                    link: None,
                    data: Arc::new(png_bytes),
                    pixels: Some((w, h)),
                    page: 0,
                };
                d.assets.insert(aid, Arc::new(new_asset));
                (aid, None)
            };

            if let Some(item) = d.item_mut(item_id)
                && let Content::Graphic(ref mut g) = item.content
            {
                g.asset = new_aid;
            }

            retouched_list.push(json!({
                "itemId": item_id.0,
                "originalAsset": orig_aid.0,
                "newAsset": new_aid.0,
                "preset": preset.name(),
                "scene": format!("{:?}", report.original_histogram.scene),
                "elapsedMicros": report.elapsed_micros,
                "histogram": {
                    "totalPixels": report.original_histogram.total_pixels,
                    "mean": report.original_histogram.mean,
                    "stdDev": report.original_histogram.std_dev,
                    "p1": report.original_histogram.p1,
                    "p50": report.original_histogram.p50,
                    "p99": report.original_histogram.p99,
                },
                "tac": tac_info,
            }));
        }

        if retouched_list.is_empty() {
            return Err(bad(ID, "selected items do not contain any graphic photos"));
        }

        Ok(json!({ "retouched": retouched_list }))
    })
}

fn analyze_scene(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "image.analyzeScene";
    let target_ids = targets(s, p)?;
    if target_ids.is_empty() {
        return Err(bad(ID, "no graphic items selected or specified"));
    }

    let st = s.doc()?;
    let mut analysis_list = Vec::new();

    for item_id in target_ids {
        let Some(item) = st.doc.item(item_id) else { continue };
        let Content::Graphic(ref g) = item.content else { continue };
        let Some(asset) = st.doc.assets.get(&g.asset) else { continue };

        if asset.data.is_empty() {
            continue;
        }

        let is_cmyk = is_cmyk_tiff(&asset.data);
        let tac_info = if is_cmyk {
            decode_cmyk_tiff(&asset.data).map(|r| {
                let m = measure_tac(&r, 220.0);
                json!({
                    "maxTac": m.max_tac,
                    "avgTac": m.avg_tac,
                    "exceedingPixels": m.exceeding_pixels,
                    "totalPixels": m.total_pixels,
                })
            })
        } else {
            None
        };

        if let Some(rgba) = designcraft_images::decode_rgba(&asset.data) {
            let hist = designcraft_images::PhotoHistogram::from_rgba8(&rgba);
            analysis_list.push(json!({
                "itemId": item_id.0,
                "assetId": g.asset.0,
                "assetName": asset.name,
                "isCmyk": is_cmyk,
                "scene": format!("{:?}", hist.scene),
                "totalPixels": hist.total_pixels,
                "min": hist.min_luminance,
                "max": hist.max_luminance,
                "mean": hist.mean,
                "stdDev": hist.std_dev,
                "p1": hist.p1,
                "p50": hist.p50,
                "p99": hist.p99,
                "tac": tac_info,
            }));
        }
    }

    if analysis_list.is_empty() {
        return Err(bad(ID, "no valid raster graphic items found"));
    }

    Ok(json!({ "analysis": analysis_list }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "image.retouchRotary",
            "Retoque para Rotativa (AIE)",
            ["Object", "Image"],
            None,
            "{ids?: [u64], id?: u64, preset?: coldset|improved|heatset|sheetfed, format?: rgba|cmyk, profile?: string, clean?: bool, sharpen?: bool, sharpenAmount?: f32, autoLevels?: bool} — Automatic Image Enhancement or Clean CMYK Separation (ISO 12647-3 / ISOnewspaper26v4)",
            has_selection,
            retouch_rotary
        ),
        cmd!(
            "image.retouch_rotary",
            "Retoque para Rotativa (AIE)",
            [],
            None,
            "{ids?: [u64], id?: u64, preset?: coldset|improved|heatset|sheetfed, format?: rgba|cmyk, profile?: string, clean?: bool, sharpen?: bool, sharpenAmount?: f32, autoLevels?: bool} — Alias for image.retouchRotary",
            has_selection,
            retouch_rotary
        ),
        cmd!(
            query "image.analyzeScene",
            "Analizar Escena Fotográfica",
            [],
            None,
            "{ids?: [u64], id?: u64} → [{itemId, assetName, scene, p1, p50, p99, mean, tac}]",
            has_selection,
            analyze_scene
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::base64_encode;
    use designcraft_doc::ItemId;

    fn make_test_photo_png() -> Vec<u8> {
        let mut img = designcraft_images::image::RgbaImage::new(40, 40);
        for y in 0..40 {
            for x in 0..40 {
                let v = ((x + y) * 255 / 80) as u8;
                img.put_pixel(x, y, designcraft_images::image::Rgba([v, v, v, 255]));
            }
        }
        designcraft_images::encode_png(&img).unwrap()
    }

    #[test]
    fn retouch_rotary_command_optimizes_image_and_supports_undo() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();

        let png_bytes = make_test_photo_png();
        let placed = s
            .execute(
                "file.place",
                &json!({
                    "base64": base64_encode(&png_bytes),
                    "name": "editorial_sample.png",
                    "x": 72,
                    "y": 72,
                    "width": 200,
                }),
            )
            .unwrap();

        let item_id = placed["id"].as_u64().unwrap();
        let orig_asset_id = placed["asset"].as_u64().unwrap();

        // 1. Analyze scene query
        let analysis = s.execute("image.analyzeScene", &json!({"ids": [item_id]})).unwrap();
        let arr = analysis["analysis"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["itemId"].as_u64().unwrap(), item_id);
        assert_eq!(arr[0]["totalPixels"].as_u64().unwrap(), 40 * 40);

        // 2. Execute retouch for rotary coldset
        let res = s
            .execute(
                "image.retouchRotary",
                &json!({
                    "ids": [item_id],
                    "preset": "coldset",
                    "format": "rgba",
                }),
            )
            .unwrap();

        let retouched = res["retouched"].as_array().unwrap();
        assert_eq!(retouched.len(), 1);
        let new_asset_id = retouched[0]["newAsset"].as_u64().unwrap();
        assert_ne!(new_asset_id, orig_asset_id);

        // Verify the document item points to the new retouched asset
        let doc_st = s.doc().unwrap();
        let it = doc_st.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = it.content {
            assert_eq!(g.asset.0, new_asset_id);
        } else {
            panic!("expected Content::Graphic");
        }

        // 3. Test Undo
        s.execute("edit.undo", &json!({})).unwrap();
        let doc_after_undo = s.doc().unwrap();
        let it_undone = doc_after_undo.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = it_undone.content {
            assert_eq!(g.asset.0, orig_asset_id);
        } else {
            panic!("expected Content::Graphic");
        }

        // 4. Test Redo
        s.execute("edit.redo", &json!({})).unwrap();
        let doc_after_redo = s.doc().unwrap();
        let it_redone = doc_after_redo.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = it_redone.content {
            assert_eq!(g.asset.0, new_asset_id);
        } else {
            panic!("expected Content::Graphic");
        }
    }

    #[test]
    fn retouch_rotary_command_generates_cmyk_tiff_with_safe_tac() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();

        let png_bytes = make_test_photo_png();
        let placed = s
            .execute(
                "file.place",
                &json!({
                    "base64": base64_encode(&png_bytes),
                    "name": "dark_photo.png",
                    "x": 72,
                    "y": 72,
                    "width": 200,
                }),
            )
            .unwrap();

        let item_id = placed["id"].as_u64().unwrap();

        let res = s
            .execute(
                "image.retouch_rotary",
                &json!({
                    "ids": [item_id],
                    "preset": "coldset",
                    "format": "cmyk",
                }),
            )
            .unwrap();

        let retouched = res["retouched"].as_array().unwrap();
        let tac_info = &retouched[0]["tac"];
        assert!(tac_info.is_object());
        let max_tac = tac_info["maxTac"].as_f64().unwrap();
        assert!(max_tac <= 220.5, "Max TAC exceeded 220%: {max_tac}");
        assert_eq!(tac_info["exceedingPixels"].as_u64().unwrap(), 0);

        // Verify the asset in the document is a CMYK TIFF
        let doc_st = s.doc().unwrap();
        let it = doc_st.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = it.content {
            let asset = doc_st.doc.assets.get(&g.asset).unwrap();
            assert_eq!(asset.mime, "image/tiff");
            assert!(is_cmyk_tiff(&asset.data));
        } else {
            panic!("expected Content::Graphic");
        }
    }

    #[test]
    fn retouch_rotary_command_defaults_to_cmyk_tiff() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();

        let png_bytes = make_test_photo_png();
        let placed = s
            .execute(
                "file.place",
                &json!({
                    "base64": base64_encode(&png_bytes),
                    "name": "default_photo.png",
                    "x": 72,
                    "y": 72,
                    "width": 200,
                }),
            )
            .unwrap();

        let item_id = placed["id"].as_u64().unwrap();

        // Call without specifying "format" parameter
        let res = s
            .execute(
                "image.retouchRotary",
                &json!({
                    "ids": [item_id],
                    "preset": "coldset",
                }),
            )
            .unwrap();

        let retouched = res["retouched"].as_array().unwrap();
        assert_eq!(retouched.len(), 1);
        let doc_st = s.doc().unwrap();
        let it = doc_st.doc.item(ItemId(item_id)).unwrap();
        if let Content::Graphic(ref g) = it.content {
            let asset = doc_st.doc.assets.get(&g.asset).unwrap();
            assert_eq!(asset.mime, "image/tiff");
            assert!(is_cmyk_tiff(&asset.data));
        } else {
            panic!("expected Content::Graphic");
        }
    }

    #[test]
    fn retouch_rotary_command_supports_explicit_isonewspaper_profile() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();

        let png_bytes = make_test_photo_png();
        let placed = s
            .execute(
                "file.place",
                &json!({
                    "base64": base64_encode(&png_bytes),
                    "name": "news_photo.png",
                    "x": 72,
                    "y": 72,
                    "width": 200,
                }),
            )
            .unwrap();

        let item_id = placed["id"].as_u64().unwrap();

        let res = s
            .execute(
                "image.retouchRotary",
                &json!({
                    "ids": [item_id],
                    "preset": "coldset",
                    "profile": "ISOnewspaper26v4",
                    "format": "cmyk",
                }),
            )
            .unwrap();

        let retouched = res["retouched"].as_array().unwrap();
        let tac_info = &retouched[0]["tac"];
        assert_eq!(tac_info["profileUsed"].as_str().unwrap(), "ISOnewspaper26v4");
        assert!(tac_info["maxTac"].as_f64().unwrap() <= 220.5);
    }
}
