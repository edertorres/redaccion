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
    ///   - Highlight target: default ~4% ink (RGB ~245).
    ///   - Shadow target: default ~86% ink (RGB ~36).
    /// * Applies midtone gamma expansion ($1.20 \dots 1.25$) to neutralize dot-gain.
    pub fn auto_levels_for_rotary(
        hist: &PhotoHistogram,
        target_highlight_ink: f32, // e.g. 0.04 (4%)
        target_shadow_ink: f32,    // e.g. 0.86 (86%)
        base_gamma: f32,           // e.g. 1.25
    ) -> Self {
        if hist.total_pixels == 0 {
            return Self::identity();
        }

        // Adapt input points conservatively: do not over-stretch photos that already have natural contrast
        let (adj_in_black, adj_in_white) = match hist.scene {
            super::histogram::SceneType::LowContrast => {
                // Low contrast or flat fog: gently expand
                (hist.p1, hist.p99)
            }
            super::histogram::SceneType::Underexposed => {
                // Dark photo: anchor shadow gently
                (hist.p1.min(15), hist.p99.max(240))
            }
            super::histogram::SceneType::Overexposed => (hist.p1.min(15), hist.p99),
            _ => {
                // Normal & HighContrast: preserve the photographer's natural lighting!
                // Only trim if there is clear dead space
                let b = if hist.p1 > 35 { hist.p1.saturating_sub(15) } else { 0 };
                let w = if hist.p99 < 220 { hist.p99.saturating_add(15) } else { 255 };
                (b, w)
            }
        };

        // Convert ink percentage (0.0 to 1.0) to 8-bit RGB target
        // 0% ink -> 255 RGB (pure white); 100% ink -> 0 RGB (solid black)
        let out_white = (255.0 * (1.0 - target_highlight_ink.clamp(0.0, 0.20))).round().clamp(0.0, 255.0) as u8;
        let out_black = (255.0 * (1.0 - target_shadow_ink.clamp(0.50, 1.00))).round().clamp(0.0, 255.0) as u8;

        // Adapt gamma based on detected scene classification
        let adjusted_gamma = match hist.scene {
            super::histogram::SceneType::Underexposed => (base_gamma + 0.08).min(1.18), // Controlled lift for dark photos
            super::histogram::SceneType::Overexposed => (base_gamma - 0.05).max(0.95),  // Tame bright scenes
            super::histogram::SceneType::LowContrast => (base_gamma + 0.02).min(1.08),
            super::histogram::SceneType::HighContrast => (base_gamma - 0.02).max(0.98),
            super::histogram::SceneType::Normal => base_gamma,
        };

        Self::from_levels(adj_in_black, adj_in_white, adjusted_gamma, out_black, out_white)
    }

    /// Lookup a mapped value for an 8-bit input.
    pub fn get(&self, value: u8) -> u8 {
        self.0.get(value as usize).copied().unwrap_or(value)
    }
}

/// Apply a tone reproduction curve using Luminance-only processing (Photoshop Luminosity mode).
///
/// Instead of mapping each RGB channel independently (which changes the ratios between channels
/// and causes severe oversaturation and skin-tone sunburn), this evaluates the tone curve on the
/// pixel's perceptual luminance ($Y = 0.299R + 0.587G + 0.114B$) and scales the color components
/// proportionally. This preserves 100% of the original color saturation and hue.
/// Alpha channel is strictly preserved.
pub fn apply_tone_curve(img: &mut RgbaImage, curve: &ToneCurve) {
    for pixel in img.pixels_mut() {
        let r = pixel[0] as f32;
        let g = pixel[1] as f32;
        let b = pixel[2] as f32;

        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let y_byte = y.round().clamp(0.0, 255.0) as u8;
        let y_target = curve.get(y_byte) as f32;

        if y > 0.5 {
            let scale = y_target / y;
            let max_c = r.max(g).max(b);
            if max_c * scale > 255.0 && max_c > 0.0 {
                let s = 255.0 / max_c;
                pixel[0] = (r * s).round().clamp(0.0, 255.0) as u8;
                pixel[1] = (g * s).round().clamp(0.0, 255.0) as u8;
                pixel[2] = (b * s).round().clamp(0.0, 255.0) as u8;
            } else {
                pixel[0] = (r * scale).round().clamp(0.0, 255.0) as u8;
                pixel[1] = (g * scale).round().clamp(0.0, 255.0) as u8;
                pixel[2] = (b * scale).round().clamp(0.0, 255.0) as u8;
            }
        } else {
            let diff = y_target - y;
            pixel[0] = (r + diff).round().clamp(0.0, 255.0) as u8;
            pixel[1] = (g + diff).round().clamp(0.0, 255.0) as u8;
            pixel[2] = (b + diff).round().clamp(0.0, 255.0) as u8;
        }
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
