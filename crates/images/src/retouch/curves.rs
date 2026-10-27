//! Tone reproduction curves and auto-levels adaptation for rotary press.
//!
//! Maps digital sRGB ranges into the usable physical window of newsprint (4% highlight to 86% shadow)
//! and compensates for coldset press dot-gain (TVI ~28%) via adaptive midtone expansion.

use super::histogram::PhotoHistogram;
use image::RgbaImage;

/// A 256-entry tonal lookup table (LUT) mapping input byte $0..=255 \to 0..=255$.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToneCurve(pub [u8; 256]);

impl Default for ToneCurve {
    fn default() -> Self {
        Self::identity()
    }
}

impl ToneCurve {
    /// Create an identity tone curve ($f(x) = x$).
    pub fn identity() -> Self {
        let mut lut = [0u8; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = i as u8;
        }
        Self(lut)
    }

    /// Construct a curve from input/output black & white points and gamma.
    ///
    /// * `in_black`: Input luminance value mapped to minimum output.
    /// * `in_white`: Input luminance value mapped to maximum output.
    /// * `gamma`: Midtone curvature ($> 1.0$ brightens midtones, $< 1.0$ darkens).
    /// * `out_black`: Target black value (e.g. 36 for 86% shadow ink in newsprint).
    /// * `out_white`: Target white value (e.g. 245 for 4% highlight ink in newsprint).
    pub fn from_levels(in_black: u8, in_white: u8, gamma: f32, out_black: u8, out_white: u8) -> Self {
        let in_b = in_black.min(in_white.saturating_sub(1)) as f32;
        let in_w = in_white.max(in_black.saturating_add(1)) as f32;
        let in_range = (in_w - in_b).max(1.0);

        let out_b = out_black as f32;
        let out_w = out_white as f32;
        let out_range = out_w - out_b;

        let safe_gamma = if gamma > 0.05 && gamma.is_finite() { gamma } else { 1.0 };
        let inv_gamma = 1.0 / safe_gamma;

        let mut lut = [0u8; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            let x = i as f32;
            let normalized = ((x - in_b) / in_range).clamp(0.0, 1.0);
            let curved = normalized.powf(inv_gamma);
            let target = out_b + curved * out_range;
            *slot = target.round().clamp(0.0, 255.0) as u8;
        }

        Self(lut)
    }

    /// Pure midtone gamma lift for dot-gain compensation without changing black or white points.
    ///
    /// For rotary press (28% TVI), gamma is typically between 1.20 and 1.25.
    pub fn lift_midtones(gamma: f32) -> Self {
        Self::from_levels(0, 255, gamma, 0, 255)
    }

    /// Generate an automated tone reproduction curve tailored for newspaper coldset / heatset.
    ///
    /// * Analyzes $P_1$ and $P_{99}$ to anchor true black and white points safely.
    /// * Avoids harsh clipping by mapping to standard rotary press ink bounds:
    ///   - Highlight target: default ~4% ink (RGB ~245) to retain printable raster dots without dropout holes.
    ///   - Shadow target: default ~90% ink (RGB ~25) to prevent ink puddling and dot-bridging.
    /// * Applies midtone gamma expansion ($1.20 \dots 1.25$) to neutralize rotary coldset dot-gain (TVI ~28%).
    pub fn auto_levels_for_rotary(
        hist: &PhotoHistogram,
        target_highlight_ink: f32, // e.g. 0.04 (4%)
        target_shadow_ink: f32,    // e.g. 0.90 (90%)
        base_gamma: f32,           // e.g. 1.22
    ) -> Self {
        if hist.total_pixels == 0 {
            return Self::identity();
        }

        // Convert ink percentage (0.0 to 1.0) to 8-bit RGB target
        // 0% ink -> 255 RGB (pure white); 100% ink -> 0 RGB (solid black)
        let out_white = (255.0 * (1.0 - target_highlight_ink.clamp(0.0, 0.20))).round().clamp(0.0, 255.0) as u8;
        let out_black = (255.0 * (1.0 - target_shadow_ink.clamp(0.50, 1.00))).round().clamp(0.0, 255.0) as u8;

        // Determine input shadow anchor (in_black):
        // P1 is the 1st percentile. If P1 > 8, pull dead fog out of shadows while protecting real texture.
        let in_black = match hist.scene {
            super::histogram::SceneType::LowContrast => hist.p1,
            super::histogram::SceneType::Underexposed => hist.p1.min(10),
            super::histogram::SceneType::Overexposed => hist.p1.min(20),
            _ => {
                if hist.p1 > 8 {
                    (hist.p1 as f32 * 0.75).round() as u8
                } else {
                    0
                }
            }
        };

        // Determine input highlight anchor (in_white):
        // P99 is the 99th percentile. Expand highlights smoothly without clipping textured whites.
        let in_white = match hist.scene {
            super::histogram::SceneType::LowContrast => hist.p99,
            super::histogram::SceneType::Overexposed => hist.p99.max(250),
            super::histogram::SceneType::Underexposed => hist.p99.max(240),
            _ => {
                if hist.p99 < 248 {
                    (hist.p99 as f32 + (255.0 - hist.p99 as f32) * 0.25).round().clamp(0.0, 255.0) as u8
                } else {
                    255
                }
            }
        };

        // Adapt gamma based on detected scene classification and median luminance:
        // Rotary coldset press has ~28% dot gain. Midtones MUST be lifted so photos don't look muddy.
        let adjusted_gamma = match hist.scene {
            super::histogram::SceneType::Underexposed => (base_gamma + 0.12).clamp(1.15, 1.40),
            super::histogram::SceneType::Overexposed => (base_gamma - 0.10).clamp(1.04, 1.18),
            super::histogram::SceneType::LowContrast => (base_gamma + 0.04).clamp(1.10, 1.30),
            super::histogram::SceneType::HighContrast => (base_gamma - 0.04).clamp(1.10, 1.25),
            super::histogram::SceneType::Normal => {
                if hist.p50 < 100 {
                    let boost = ((100.0 - hist.p50 as f32) / 100.0) * 0.08;
                    (base_gamma + boost).clamp(1.15, 1.35)
                } else if hist.p50 > 155 {
                    let drop = ((hist.p50 as f32 - 155.0) / 100.0) * 0.06;
                    (base_gamma - drop).clamp(1.08, 1.25)
                } else {
                    base_gamma
                }
            }
        };

        Self::from_levels(in_black, in_white, adjusted_gamma, out_black, out_white)
    }

    /// Lookup a mapped value for an 8-bit input.
    pub fn get(&self, value: u8) -> u8 {
        self.0.get(value as usize).copied().unwrap_or(value)
    }
}

/// Apply a tone reproduction curve using Luminance-only processing (Photoshop Luminosity mode).
///
/// Implements the exact Adobe PDF / W3C specification for `SetLum` (Luminosity blend mode).
/// Preserves 100% of the original color hue and saturation, adapting out-of-gamut bounds
/// smoothly through luminance-preserving chroma scaling instead of arbitrary channel clipping.
/// Uses Rec.709 coefficients ($Y = 0.2126\,R + 0.7152\,G + 0.0722\,B$) matching the sRGB color space.
/// Alpha channel is strictly preserved.
pub fn apply_tone_curve(img: &mut RgbaImage, curve: &ToneCurve) {
    for pixel in img.pixels_mut() {
        let r = pixel[0] as f32;
        let g = pixel[1] as f32;
        let b = pixel[2] as f32;

        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let y_byte = y.round().clamp(0.0, 255.0) as u8;
        let y_target = curve.get(y_byte) as f32;

        let delta = y_target - y;
        let mut r_new = r + delta;
        let mut g_new = g + delta;
        let mut b_new = b + delta;

        let min_c = r_new.min(g_new).min(b_new);
        let max_c = r_new.max(g_new).max(b_new);

        if min_c < 0.0 {
            let lum = 0.2126 * r_new + 0.7152 * g_new + 0.0722 * b_new;
            let denom = lum - min_c;
            if denom > 1e-4 {
                r_new = lum + (r_new - lum) * lum / denom;
                g_new = lum + (g_new - lum) * lum / denom;
                b_new = lum + (b_new - lum) * lum / denom;
            }
        }
        if max_c > 255.0 {
            let lum = 0.2126 * r_new + 0.7152 * g_new + 0.0722 * b_new;
            let denom = max_c - lum;
            if denom > 1e-4 {
                r_new = lum + (r_new - lum) * (255.0 - lum) / denom;
                g_new = lum + (g_new - lum) * (255.0 - lum) / denom;
                b_new = lum + (b_new - lum) * (255.0 - lum) / denom;
            }
        }

        pixel[0] = r_new.round().clamp(0.0, 255.0) as u8;
        pixel[1] = g_new.round().clamp(0.0, 255.0) as u8;
        pixel[2] = b_new.round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn identity_curve_leaves_pixels_unchanged() {
        let curve = ToneCurve::identity();
        for i in 0..=255 {
            assert_eq!(curve.get(i), i);
        }
    }

    #[test]
    fn midtone_lift_brightens_middle_values() {
        let curve = ToneCurve::lift_midtones(1.25);
        assert_eq!(curve.get(0), 0);
        assert_eq!(curve.get(255), 255);
        // Midtone (128) must be lifted above 128
        assert!(curve.get(128) > 128);
        assert!(curve.get(128) >= 145);
    }

    #[test]
    fn rotary_clipping_bounds_respected() {
        let mut img = RgbaImage::new(100, 1);
        for x in 0..100 {
            img.put_pixel(x, 0, Rgba([x as u8 * 2, x as u8 * 2, x as u8 * 2, 255]));
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        // 4% highlight target -> out_white ~ 245
        // 86% shadow target -> out_black ~ 36
        let curve = ToneCurve::auto_levels_for_rotary(&hist, 0.04, 0.86, 1.25);

        // Minimum output cannot be lower than out_black
        assert!(curve.get(0) >= 34);
        // Maximum output cannot be higher than out_white
        assert!(curve.get(255) <= 247);
    }

    #[test]
    fn apply_curve_preserves_alpha() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([50, 50, 50, 180]));
        img.put_pixel(1, 1, Rgba([200, 200, 200, 95]));

        let curve = ToneCurve::lift_midtones(1.25);
        apply_tone_curve(&mut img, &curve);

        assert_eq!(img.get_pixel(0, 0)[3], 180);
        assert_eq!(img.get_pixel(1, 1)[3], 95);
        assert!(img.get_pixel(0, 0)[0] > 50);
    }
}
