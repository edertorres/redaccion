//! CMYK separation, TAC/TIL strict enforcement, and newsprint soft-proofing.
//!
//! Provides:
//! 1. Conversion of sRGB images to press-ready CMYK rasters conforming to ISO 12647-3 / WAN-IFRA.
//! 2. Pixel-level TAC (Total Area Coverage) clamping ensuring ink sum <= 220%.
//! 3. Measurement and auditing of TAC metrics for preflight verification.
//! 4. Screen soft-proofing simulating newsprint base paper color and press dot gain.

use super::RotaryPreset;
use crate::CmykRaster;
use designcraft_color::cms::{GenericCmyk, Intent, generic, resolve_cmyk_profile};
use image::RgbaImage;

/// TAC (Total Area Coverage / Total Ink Limit) auditing report for a CMYK raster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TacMetrics {
    /// Maximum ink coverage observed on any pixel in percent (e.g. 218.5%).
    pub max_tac: f32,
    /// Average ink coverage across all pixels in percent.
    pub avg_tac: f32,
    /// Number of pixels exceeding the safety limit (e.g. 220%).
    pub exceeding_pixels: u64,
    /// Total pixels inspected.
    pub total_pixels: u64,
}

impl TacMetrics {
    /// True if any pixel exceeds the allowed TAC limit (with 0.5% tolerance for rounding).
    pub fn exceeds_limit(&self, limit_percent: f32) -> bool {
        self.max_tac > (limit_percent + 0.5)
    }
}

/// Convert an sRGB image to a press-ready CMYK raster conforming to the given rotary preset,
/// with an optional target CMYK ICC profile (such as ISOnewspaper26v4).
pub fn separate_to_cmyk_with_profile(img: &RgbaImage, preset: RotaryPreset, profile_name: Option<&str>) -> CmykRaster {
    let width = img.width();
    let height = img.height();
    let total_pixels = (width as usize).saturating_mul(height as usize);

    let max_tac_f32 = (preset.max_tac_percent() as f32) / 100.0;

    // Use coldset press model for newsprint or generic swop for commercial
    let press_params = match preset {
        RotaryPreset::ColdsetNewspaper => generic::newspaper_coldset(),
        RotaryPreset::ImprovedNewspaperSC => {
            let mut p = generic::newspaper_coldset();
            p.tac = 2.40;
            p
        }
        RotaryPreset::HeatsetMagazine => {
            let mut p = generic::swop_like();
            p.tac = 2.80;
            p
        }
        RotaryPreset::CommercialSheetfed => generic::swop_like(),
    };

    let press = GenericCmyk::new(press_params);

    let target_profile = match profile_name {
        Some(p) if !p.trim().is_empty() => p,
        _ if preset == RotaryPreset::ColdsetNewspaper => "ISOnewspaper26v4",
        _ => "",
    };

    let icc_profile = if !target_profile.is_empty() { resolve_cmyk_profile(target_profile) } else { None };

    use rayon::prelude::*;

    let raw_rgba = img.as_flat_samples().samples;
    let mut cmyk_bytes = vec![0u8; total_pixels.saturating_mul(4)];
    let mut alpha_bytes = vec![255u8; total_pixels];
    let mut has_transparency = false;

    // Fast check for transparency
    for chunk in raw_rgba.chunks_exact(4) {
        if chunk[3] < 255 {
            has_transparency = true;
            break;
        }
    }

    if has_transparency {
        for (i, chunk) in raw_rgba.chunks_exact(4).enumerate() {
            alpha_bytes[i] = chunk[3];
        }
    }

    // Parallelize CMYK separation across chunks using Rayon
    const CHUNK_PIXELS: usize = 2048;
    cmyk_bytes.par_chunks_mut(CHUNK_PIXELS * 4).enumerate().for_each(|(chunk_idx, cmyk_chunk)| {
        const CACHE_SIZE: usize = 4096;
        let mut cache_keys = [u32::MAX; CACHE_SIZE];
        let mut cache_cmyk = [[0u8; 4]; CACHE_SIZE];

        let pixel_offset = chunk_idx * CHUNK_PIXELS;
        let chunk_pixel_count = cmyk_chunk.len() / 4;

        for i in 0..chunk_pixel_count {
            let src_idx = (pixel_offset + i) * 4;
            let pr = raw_rgba[src_idx];
            let pg = raw_rgba[src_idx + 1];
            let pb = raw_rgba[src_idx + 2];

            let rgb_key = ((pr as u32) << 16) | ((pg as u32) << 8) | (pb as u32);
            let h = ((rgb_key ^ (rgb_key >> 11) ^ (rgb_key >> 5)) as usize) & (CACHE_SIZE - 1);

            let dst_idx = i * 4;
            if cache_keys[h] == rgb_key {
                let cached = cache_cmyk[h];
                cmyk_chunk[dst_idx] = cached[0];
                cmyk_chunk[dst_idx + 1] = cached[1];
                cmyk_chunk[dst_idx + 2] = cached[2];
                cmyk_chunk[dst_idx + 3] = cached[3];
                continue;
            }

            let r = (pr as f32) / 255.0;
            let g = (pg as f32) / 255.0;
            let b = (pb as f32) / 255.0;

            let [mut c, mut m, mut y, mut k] = if let Some(ref icc) = icc_profile {
                if let Some(cmyk) = icc.from_srgb([r, g, b], Intent::RelativeColorimetric) {
                    if cmyk.len() >= 4 {
                        [cmyk[0], cmyk[1], cmyk[2], cmyk[3]]
                    } else {
                        press.from_srgb([r, g, b], Intent::RelativeColorimetric, true)
                    }
                } else {
                    press.from_srgb([r, g, b], Intent::RelativeColorimetric, true)
                }
            } else {
                press.from_srgb([r, g, b], Intent::RelativeColorimetric, true)
            };

            let total_ink = c + m + y + k;
            if total_ink > max_tac_f32 && total_ink > 0.0 {
                let excess = total_ink - max_tac_f32;
                let cmy_sum = c + m + y;
                if cmy_sum > 0.0 {
                    let scale = ((cmy_sum - excess) / cmy_sum).clamp(0.0, 1.0);
                    c *= scale;
                    m *= scale;
                    y *= scale;
                } else {
                    k = max_tac_f32.min(1.0);
                }
            }

            let mut c_byte = (c * 255.0).round().clamp(0.0, 255.0) as u8;
            let mut m_byte = (m * 255.0).round().clamp(0.0, 255.0) as u8;
            let mut y_byte = (y * 255.0).round().clamp(0.0, 255.0) as u8;
            let mut k_byte = (k * 255.0).round().clamp(0.0, 255.0) as u8;

            let max_byte_sum = (max_tac_f32 * 255.0).round() as u16;
            let byte_sum = c_byte as u16 + m_byte as u16 + y_byte as u16 + k_byte as u16;
            if byte_sum > max_byte_sum {
                let excess = byte_sum - max_byte_sum;
                let cmy_sum = (c_byte as u16) + (m_byte as u16) + (y_byte as u16);
                if cmy_sum > 0 {
                    // Proportional reduction of C, M, Y to strictly preserve neutral grey balance
                    let scale = (cmy_sum.saturating_sub(excess) as f32) / (cmy_sum as f32);
                    c_byte = ((c_byte as f32) * scale).round() as u8;
                    m_byte = ((m_byte as f32) * scale).round() as u8;
                    y_byte = ((y_byte as f32) * scale).round() as u8;

                    // Final rounding adjustment: deduct single unit from the largest component
                    let mut rem = (c_byte as u16 + m_byte as u16 + y_byte as u16 + k_byte as u16).saturating_sub(max_byte_sum);
                    while rem > 0 {
                        if c_byte >= m_byte && c_byte >= y_byte && c_byte > 0 {
                            c_byte -= 1;
                        } else if m_byte >= y_byte && m_byte > 0 {
                            m_byte -= 1;
                        } else if y_byte > 0 {
                            y_byte -= 1;
                        } else if k_byte > 0 {
                            k_byte -= 1;
                        } else {
                            break;
                        }
                        rem -= 1;
                    }
                } else {
                    k_byte = k_byte.saturating_sub(excess as u8);
                }
            }

            let res = [c_byte, m_byte, y_byte, k_byte];
            cache_keys[h] = rgb_key;
            cache_cmyk[h] = res;

            cmyk_chunk[dst_idx] = c_byte;
            cmyk_chunk[dst_idx + 1] = m_byte;
            cmyk_chunk[dst_idx + 2] = y_byte;
            cmyk_chunk[dst_idx + 3] = k_byte;
        }
    });

    CmykRaster { width, height, cmyk: cmyk_bytes, alpha: if has_transparency { Some(alpha_bytes) } else { None } }
}

/// Convert an sRGB image to a press-ready CMYK raster conforming to the given rotary preset.
///
/// Ensures that:
/// * Heavy GCR is applied so black carries the dark tones.
/// * Total ink coverage ($C + M + Y + K$) is strictly clamped to the preset limit (e.g. <= 220%).
pub fn separate_to_rotary_cmyk(img: &RgbaImage, preset: RotaryPreset) -> CmykRaster {
    separate_to_cmyk_with_profile(img, preset, None)
}

/// Audit total ink coverage across all pixels in a CMYK raster.
pub fn measure_tac(raster: &CmykRaster, threshold_percent: f32) -> TacMetrics {
    let total_pixels = usize::try_from(raster.width).unwrap_or(0).saturating_mul(usize::try_from(raster.height).unwrap_or(0));
    if total_pixels == 0 || raster.cmyk.len() < total_pixels * 4 {
        return TacMetrics { max_tac: 0.0, avg_tac: 0.0, exceeding_pixels: 0, total_pixels: 0 };
    }

    let mut max_tac = 0.0f32;
    let mut sum_tac = 0.0f64;
    let mut exceeding_pixels = 0u64;

    for chunk in raster.cmyk.chunks_exact(4) {
        let c = chunk[0] as f32;
        let m = chunk[1] as f32;
        let y = chunk[2] as f32;
        let k = chunk[3] as f32;

        let tac = (c + m + y + k) / 2.55; // 0..255 to 0..100%
        if tac > max_tac {
            max_tac = tac;
        }
        sum_tac += tac as f64;
        if tac > threshold_percent {
            exceeding_pixels += 1;
        }
    }

    let avg_tac = (sum_tac / (total_pixels as f64)) as f32;

    TacMetrics { max_tac, avg_tac, exceeding_pixels, total_pixels: total_pixels as u64 }
}

/// Simulate on-screen newsprint soft-proofing from a CMYK raster.
///
/// Converts CMYK ink values back to sRGB taking into account the greyish/yellowish
/// substrate of 45g newsprint and dot gain, allowing editors to see the printed reality.
pub fn soft_proof_cmyk_to_srgb(raster: &CmykRaster, preset: RotaryPreset) -> RgbaImage {
    let width = raster.width;
    let height = raster.height;
    let mut out = RgbaImage::new(width, height);

    let press_params = match preset {
        RotaryPreset::ColdsetNewspaper | RotaryPreset::ImprovedNewspaperSC => generic::newspaper_coldset(),
        RotaryPreset::HeatsetMagazine | RotaryPreset::CommercialSheetfed => generic::swop_like(),
    };
    let press = GenericCmyk::new(press_params);

    let mut alpha_iter = raster.alpha.as_ref().map(|a| a.iter());

    for (i, chunk) in raster.cmyk.chunks_exact(4).enumerate() {
        let x = (i as u32) % width;
        let y = (i as u32) / width;

        let c = (chunk[0] as f32) / 255.0;
        let m = (chunk[1] as f32) / 255.0;
        let y_ink = (chunk[2] as f32) / 255.0;
        let k = (chunk[3] as f32) / 255.0;

        // Simulate paper tint and dot-gain in soft proof (absolute colorimetric)
        let [r, g, b] = press.to_srgb([c, m, y_ink, k], true);
        let a = alpha_iter.as_mut().and_then(|it| it.next()).copied().unwrap_or(255);

        let r_byte = (r * 255.0).round().clamp(0.0, 255.0) as u8;
        let g_byte = (g * 255.0).round().clamp(0.0, 255.0) as u8;
        let b_byte = (b * 255.0).round().clamp(0.0, 255.0) as u8;

        out.put_pixel(x, y, image::Rgba([r_byte, g_byte, b_byte, a]));
    }

    out
}

/// Convert a CMYK raster to an sRGB image for on-screen display.
///
/// Unlike soft-proofing (which uses absolute colorimetric rendering to simulate paper tint),
/// this uses relative colorimetric rendering so paper white maps to clean monitor white [255, 255, 255],
/// while deep rich black and halftone colors accurately reproduce the target ink densities without
/// the naive linear fading of uncalibrated CMYK decoders.
pub fn cmyk_raster_to_display_rgba(raster: &CmykRaster) -> RgbaImage {
    use rayon::prelude::*;

    let width = raster.width;
    let height = raster.height;
    let total_pixels = usize::try_from(width).unwrap_or(0).saturating_mul(usize::try_from(height).unwrap_or(0));
    if total_pixels == 0 || raster.cmyk.len() < total_pixels * 4 {
        return RgbaImage::new(width.max(1), height.max(1));
    }

    let mut out_rgba = vec![255u8; total_pixels * 4];
    let icc_profile = resolve_cmyk_profile("ISOnewspaper26v4");
    let generic_newspaper = GenericCmyk::new(generic::newspaper_coldset());

    const CHUNK_PIXELS: usize = 2048;
    out_rgba.par_chunks_mut(CHUNK_PIXELS * 4).zip(raster.cmyk.par_chunks(CHUNK_PIXELS * 4)).for_each(|(rgba_chunk, cmyk_chunk)| {
        const CACHE_SIZE: usize = 4096;
        let mut cache_keys = [u32::MAX; CACHE_SIZE];
        let mut cache_rgb = [[0u8; 3]; CACHE_SIZE];

        for (rgba_px, cmyk_px) in rgba_chunk.chunks_exact_mut(4).zip(cmyk_chunk.chunks_exact(4)) {
            let c_byte = cmyk_px[0];
            let m_byte = cmyk_px[1];
            let y_byte = cmyk_px[2];
            let k_byte = cmyk_px[3];

            let cmyk_key = ((c_byte as u32) << 24) | ((m_byte as u32) << 16) | ((y_byte as u32) << 8) | (k_byte as u32);
            let h = ((cmyk_key ^ (cmyk_key >> 16) ^ (cmyk_key >> 8)) as usize) & (CACHE_SIZE - 1);

            if cache_keys[h] == cmyk_key {
                let rgb = cache_rgb[h];
                rgba_px[0] = rgb[0];
                rgba_px[1] = rgb[1];
                rgba_px[2] = rgb[2];
                continue;
            }

            let c = (c_byte as f32) / 255.0;
            let m = (m_byte as f32) / 255.0;
            let y = (y_byte as f32) / 255.0;
            let k = (k_byte as f32) / 255.0;

            let [r, g, b] = if let Some(ref icc) = icc_profile {
                if let Some(rgb) = icc.to_srgb(&[c, m, y, k], Intent::RelativeColorimetric) {
                    rgb
                } else {
                    generic_newspaper.to_srgb_bpc([c, m, y, k])
                }
            } else {
                generic_newspaper.to_srgb_bpc([c, m, y, k])
            };

            let r_byte = (r * 255.0).round().clamp(0.0, 255.0) as u8;
            let g_byte = (g * 255.0).round().clamp(0.0, 255.0) as u8;
            let b_byte = (b * 255.0).round().clamp(0.0, 255.0) as u8;

            cache_keys[h] = cmyk_key;
            cache_rgb[h] = [r_byte, g_byte, b_byte];

            rgba_px[0] = r_byte;
            rgba_px[1] = g_byte;
            rgba_px[2] = b_byte;
        }
    });

    if let Some(ref alpha) = raster.alpha {
        for (rgba_px, &a) in out_rgba.chunks_exact_mut(4).zip(alpha.iter()) {
            rgba_px[3] = a;
        }
    }

    image::RgbaImage::from_raw(width, height, out_rgba).unwrap_or_else(|| RgbaImage::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn cmyk_separation_enforces_tac_220() {
        // Deep black pixel in sRGB [0, 0, 0]
        let mut img = RgbaImage::new(4, 4);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 255]);
        }

        let cmyk_raster = separate_to_rotary_cmyk(&img, RotaryPreset::ColdsetNewspaper);
        let metrics = measure_tac(&cmyk_raster, 220.0);

        // Strict guarantee: TAC cannot exceed 220%
        assert!(metrics.max_tac <= 220.5, "Max TAC exceeded: {}%", metrics.max_tac);
        assert_eq!(metrics.exceeding_pixels, 0);
    }

    #[test]
    fn soft_proof_generates_valid_image() {
        let mut img = RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([100, 150, 200, 255]);
        }
        let cmyk = separate_to_rotary_cmyk(&img, RotaryPreset::ColdsetNewspaper);
        let proof = soft_proof_cmyk_to_srgb(&cmyk, RotaryPreset::ColdsetNewspaper);
        assert_eq!(proof.dimensions(), (10, 10));
        assert!(proof.get_pixel(0, 0)[0] > 0);
    }
}
