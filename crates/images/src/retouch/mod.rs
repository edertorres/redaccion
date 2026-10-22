//! Automatic Image Enhancement (AIE) and rotary press optimization.
//!
//! Provides the core pipeline for ISO 12647-3 / WAN-IFRA newsprint reproduction:
//! 1. Statistical luminance histogram and scene classification.
//! 2. Rotary press auto-levels with safe highlight/shadow target clipping.
//! 3. Dot-gain (TVI) midtone expansion curve.
//! 4. Spatial unsharp masking tuned for coarse halftone screens (100–133 LPI).

pub mod cmyk;
pub mod curves;
pub mod histogram;
pub mod sharpen;

pub use cmyk::{
    TacMetrics, cmyk_raster_to_display_rgba, measure_tac, separate_to_cmyk_with_profile, separate_to_rotary_cmyk, soft_proof_cmyk_to_srgb,
};
pub use curves::{ToneCurve, apply_tone_curve};
pub use histogram::{PhotoHistogram, SceneType};
pub use sharpen::{UnsharpParams, apply_unsharp_mask};

use image::RgbaImage;
use std::time::Instant;

/// Printing technology presets conforming to international press standards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotaryPreset {
    /// Coldset Newsprint 42–45g (ISO 12647-3 / WAN-IFRA). TAC <= 220%, TVI 28-32%.
    #[default]
    ColdsetNewspaper,
    /// Improved Newsprint SC 55g (WAN-IFRA 26v5). TAC <= 240%, TVI 22-26%.
    ImprovedNewspaperSC,
    /// Heatset Weekly Magazine / Supplement. TAC <= 280%, TVI 18-20%.
    HeatsetMagazine,
    /// Commercial Sheetfed Offset (FOGRA51 / Coated 135g). TAC <= 300%, TVI 14-16%.
    CommercialSheetfed,
}

impl RotaryPreset {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ColdsetNewspaper => "Rotativa Periódico Estándar (Coldset - ISO 12647-3)",
            Self::ImprovedNewspaperSC => "Rotativa Periódico Mejorado (SC Paper 55g)",
            Self::HeatsetMagazine => "Revista / Suplemento Semanal (Heatset)",
            Self::CommercialSheetfed => "Offset Plano Comercial (Couché 135g)",
        }
    }

    /// Target printable highlight ink limit. Full dynamic range for ICC separation.
    pub fn target_highlight_ink(&self) -> f32 {
        0.0
    }

    /// Target printable shadow ink limit. Full black range; the ICC profile and TAC clamp prevent ink clogging.
    pub fn target_shadow_ink(&self) -> f32 {
        1.0
    }

    /// Midtone gamma adjustment without double TVI compensation.
    pub fn midtone_gamma(&self) -> f32 {
        match self {
            Self::ColdsetNewspaper => 1.03, // Crisp natural midtone clarity
            Self::ImprovedNewspaperSC => 1.02,
            Self::HeatsetMagazine => 1.01,
            Self::CommercialSheetfed => 1.0,
        }
    }

    /// Maximum total area coverage (TAC / TIL).
    pub fn max_tac_percent(&self) -> u16 {
        match self {
            Self::ColdsetNewspaper => 220,
            Self::ImprovedNewspaperSC => 240,
            Self::HeatsetMagazine => 280,
            Self::CommercialSheetfed => 300,
        }
    }

    /// Calibrated unsharp mask parameters: subtle print crispness without halo ringing or harsh cutouts.
    pub fn unsharp_params(&self) -> UnsharpParams {
        match self {
            Self::ColdsetNewspaper => UnsharpParams { radius: 0.8, amount: 0.40, threshold: 6 },
            Self::ImprovedNewspaperSC => UnsharpParams { radius: 0.7, amount: 0.35, threshold: 5 },
            Self::HeatsetMagazine => UnsharpParams { radius: 0.6, amount: 0.30, threshold: 4 },
            Self::CommercialSheetfed => UnsharpParams { radius: 0.5, amount: 0.25, threshold: 3 },
        }
    }
}

/// Execution telemetry and diagnostic report of an AIE optimization run.
#[derive(Clone, Debug, PartialEq)]
pub struct RetouchReport {
    /// Preset used.
    pub preset: RotaryPreset,
    /// Original histogram analysis.
    pub original_histogram: PhotoHistogram,
    /// Tone reproduction curve applied.
    pub tone_curve: ToneCurve,
    /// Unsharp mask parameters applied.
    pub unsharp_params: UnsharpParams,
    /// Execution time in microseconds.
    pub elapsed_micros: u128,
}

/// Options for tailoring the automatic retouching pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetouchOptions {
    /// Whether to apply auto-levels and dot-gain tone curves (default: true).
    pub auto_levels: bool,
    /// Whether to apply spatial unsharp masking (default: true).
    pub sharpen: bool,
    /// Custom sharpening parameters, or default for the preset if None.
    pub custom_sharpen: Option<UnsharpParams>,
}

impl Default for RetouchOptions {
    fn default() -> Self {
        Self { auto_levels: true, sharpen: true, custom_sharpen: None }
    }
}

impl RetouchOptions {
    /// Pure color management separation without modifying the photo's pixels (no curves, no sharpening).
    pub fn clean() -> Self {
        Self { auto_levels: false, sharpen: false, custom_sharpen: None }
    }
}

/// Optimize an RGBA image in place with full control over auto-levels and sharpening.
pub fn optimize_for_rotary_press_with_options(img: &mut RgbaImage, preset: RotaryPreset, opts: &RetouchOptions) -> RetouchReport {
    let start = Instant::now();

    let hist = PhotoHistogram::from_rgba8(img);

    let curve = if opts.auto_levels {
        let c = ToneCurve::auto_levels_for_rotary(&hist, preset.target_highlight_ink(), preset.target_shadow_ink(), preset.midtone_gamma());
        apply_tone_curve(img, &c);
        c
    } else {
        ToneCurve::identity()
    };

    let usm = if opts.sharpen {
        let p = opts.custom_sharpen.unwrap_or_else(|| preset.unsharp_params());
        apply_unsharp_mask(img, &p);
        p
    } else {
        UnsharpParams { radius: 0.0, amount: 0.0, threshold: 0 }
    };

    let elapsed = start.elapsed().as_micros();

    RetouchReport { preset, original_histogram: hist, tone_curve: curve, unsharp_params: usm, elapsed_micros: elapsed }
}

/// Optimize an RGBA image in place for the specified rotary press preset using standard defaults.
pub fn optimize_for_rotary_press_in_place(img: &mut RgbaImage, preset: RotaryPreset) -> RetouchReport {
    optimize_for_rotary_press_with_options(img, preset, &RetouchOptions::default())
}

/// Optimize an RGBA image for the specified rotary press preset, returning a new image and report.
pub fn optimize_for_rotary_press(img: &RgbaImage, preset: RotaryPreset) -> (RgbaImage, RetouchReport) {
    let mut cloned = img.clone();
    let report = optimize_for_rotary_press_in_place(&mut cloned, preset);
    (cloned, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn full_pipeline_executes_safely() {
        let mut img = RgbaImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let v = ((x + y) * 255 / 200) as u8;
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        let (processed, report) = optimize_for_rotary_press(&img, RotaryPreset::ColdsetNewspaper);
        assert_eq!(processed.dimensions(), (100, 100));
        assert_eq!(report.preset, RotaryPreset::ColdsetNewspaper);
        assert!(report.elapsed_micros < 500_000); // Sub-second (typically < 10ms)
    }

    #[test]
    fn transparent_image_handled_safely() {
        let mut img = RgbaImage::new(20, 20);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        let (processed, report) = optimize_for_rotary_press(&img, RotaryPreset::ColdsetNewspaper);
        assert_eq!(processed.dimensions(), (20, 20));
        assert_eq!(report.original_histogram.total_pixels, 0);
    }

    #[test]
    fn test_foto1_retouch_benchmark() {
        let candidates =
            ["../../documents/Links/FOTO1.jpg", "../../templates/Links/FOTO1.jpg", "../../foto.jpg", "../../documents/Links/belisario.jpeg"];
        for rel in candidates {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
            if let Ok(bytes) = std::fs::read(&path) {
                println!(
                    "
========================================="
                );
                println!("BENCHMARK FOR: {}", rel);
                println!("File size: {} bytes ({:.2} KB)", bytes.len(), bytes.len() as f64 / 1024.0);

                let t_start = std::time::Instant::now();
                let decoded = crate::decode_rgba(&bytes);
                let decode_dur = t_start.elapsed();
                println!("1. decode_rgba: {:.2} ms", decode_dur.as_secs_f64() * 1000.0);

                if let Some(rgba) = decoded {
                    println!("   Dimensions: {}x{} ({} pixels)", rgba.width(), rgba.height(), rgba.width() * rgba.height());

                    let t_hist = std::time::Instant::now();
                    let hist = PhotoHistogram::from_rgba8(&rgba);
                    println!(
                        "2. PhotoHistogram: {:.2} ms (scene: {:?}, P1={}, P50={}, P99={})",
                        t_hist.elapsed().as_secs_f64() * 1000.0,
                        hist.scene,
                        hist.p1,
                        hist.p50,
                        hist.p99
                    );

                    let t_curve = std::time::Instant::now();
                    let curve = ToneCurve::auto_levels_for_rotary(
                        &hist,
                        RotaryPreset::ColdsetNewspaper.target_highlight_ink(),
                        RotaryPreset::ColdsetNewspaper.target_shadow_ink(),
                        RotaryPreset::ColdsetNewspaper.midtone_gamma(),
                    );
                    let mut img_curved = rgba.clone();
                    apply_tone_curve(&mut img_curved, &curve);
                    println!("3. ToneCurve & auto_levels: {:.2} ms", t_curve.elapsed().as_secs_f64() * 1000.0);

                    let t_usm = std::time::Instant::now();
                    let usm = RotaryPreset::ColdsetNewspaper.unsharp_params();
                    apply_unsharp_mask(&mut img_curved, &usm);
                    println!("4. Unsharp mask (radius {}, amount {}): {:.2} ms", usm.radius, usm.amount, t_usm.elapsed().as_secs_f64() * 1000.0);

                    let t_cmyk = std::time::Instant::now();
                    let cmyk = separate_to_rotary_cmyk(&img_curved, RotaryPreset::ColdsetNewspaper);
                    println!("5. CMYK separation: {:.2} ms", t_cmyk.elapsed().as_secs_f64() * 1000.0);

                    let t_tac = std::time::Instant::now();
                    let tac = measure_tac(&cmyk, 220.0);
                    println!(
                        "6. TAC measure: {:.2} ms (max: {:.1}%, avg: {:.1}%, exceeding: {})",
                        t_tac.elapsed().as_secs_f64() * 1000.0,
                        tac.max_tac,
                        tac.avg_tac,
                        tac.exceeding_pixels
                    );

                    let t_tiff = std::time::Instant::now();
                    let tiff_bytes = crate::encode_cmyk_tiff(&cmyk);
                    println!(
                        "7. TIFF CMYK encode: {:.2} ms (size: {} bytes)",
                        t_tiff.elapsed().as_secs_f64() * 1000.0,
                        tiff_bytes.map(|b| b.len()).unwrap_or(0)
                    );

                    let t_png = std::time::Instant::now();
                    let png_bytes = crate::encode_png(&img_curved);
                    println!(
                        "8. PNG RGBA encode: {:.2} ms (size: {} bytes)",
                        t_png.elapsed().as_secs_f64() * 1000.0,
                        png_bytes.map(|b| b.len()).unwrap_or(0)
                    );

                    println!(
                        "TOTAL PIPELINE (RGBA): {:.2} ms",
                        (decode_dur + t_hist.elapsed() + t_curve.elapsed() + t_usm.elapsed() + t_png.elapsed()).as_secs_f64() * 1000.0
                    );
                    println!(
                        "TOTAL PIPELINE (CMYK): {:.2} ms",
                        (decode_dur + t_hist.elapsed() + t_curve.elapsed() + t_usm.elapsed() + t_cmyk.elapsed() + t_tac.elapsed() + t_tiff.elapsed())
                            .as_secs_f64()
                            * 1000.0
                    );
                } else {
                    println!("FAILED TO DECODE IMAGE!");
                }
            } else {
                println!("Could not read: {}", path.display());
            }
        }
    }
}
