//! Photo histogram analysis and scene classification for newsprint and rotary press.
//!
//! Provides fast (<= 5 ms for multi-megapixel photos) calculation of luminance distribution,
//! percentiles ($P_1$, $P_{50}$, $P_{99}$), dynamic range, and scene characterisation
//! (underexposed, overexposed, low contrast, high contrast) under clean-room and never-crash rules.

use image::RgbaImage;

/// Scene classification detected from the luminance distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneType {
    /// Balanced dynamic range and tonal distribution.
    Normal,
    /// Heavily weighted towards shadows; low median and significant dark clipping.
    Underexposed,
    /// Heavily weighted towards highlights; high median and blown highlights.
    Overexposed,
    /// Compressed dynamic range with low standard deviation (hazy, foggy, or flat lighting).
    LowContrast,
    /// High dynamic range with heavy concentration at both extremes.
    HighContrast,
}

impl SceneType {
    pub fn label(&self) -> &'static str {
        match self {
            SceneType::Normal => "Normal",
            SceneType::Underexposed => "Subexpuesta (Clave baja)",
            SceneType::Overexposed => "Sobreexpuesta (Clave alta)",
            SceneType::LowContrast => "Bajo contraste (Plana / Bruma)",
            SceneType::HighContrast => "Alto contraste",
        }
    }
}

/// Comprehensive histogram statistics for an 8-bit raster image.
#[derive(Clone, Debug, PartialEq)]
pub struct PhotoHistogram {
    /// 256 luminance bins based on Rec.709 ($Y = 0.2126\,R + 0.7152\,G + 0.0722\,B$).
    pub luminance: [u32; 256],
    /// Red channel histogram (256 bins).
    pub red: [u32; 256],
    /// Green channel histogram (256 bins).
    pub green: [u32; 256],
    /// Blue channel histogram (256 bins).
    pub blue: [u32; 256],
    /// Total non-transparent pixels analyzed.
    pub total_pixels: u64,
    /// Minimum luminance observed in the image (0..=255).
    pub min_luminance: u8,
    /// Maximum luminance observed in the image (0..=255).
    pub max_luminance: u8,
    /// Average luminance ($0.0 \dots 255.0$).
    pub mean: f64,
    /// Standard deviation of luminance.
    pub std_dev: f64,
    /// 1st percentile: dark anchor point, ignoring isolated black noise.
    pub p1: u8,
    /// 50th percentile: median tonal key of the scene.
    pub p50: u8,
    /// 99th percentile: highlight anchor point, ignoring specular noise.
    pub p99: u8,
    /// Usable dynamic range ($P_{99} - P_1$).
    pub dynamic_range: u8,
    /// Scene classification deduced from the histogram.
    pub scene: SceneType,
}

impl PhotoHistogram {
    /// Compute the histogram and statistical metrics from an RGBA image buffer.
    /// Transparent pixels (alpha == 0) are ignored to avoid background skew.
    pub fn from_rgba8(img: &RgbaImage) -> Self {
        let mut luminance = [0u32; 256];
        let mut red = [0u32; 256];
        let mut green = [0u32; 256];
        let mut blue = [0u32; 256];
        let mut total_pixels = 0u64;
        let mut min_lum = 255u8;
        let mut max_lum = 0u8;

        for pixel in img.pixels() {
            let a = pixel[3];
            if a == 0 {
                continue;
            }
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];

            // Rec.709 integer approximation: (2126 * R + 7152 * G + 722 * B + 5000) / 10000
            let y = ((2126u32
                .saturating_mul(u32::from(r))
                .saturating_add(7152u32.saturating_mul(u32::from(g)))
                .saturating_add(722u32.saturating_mul(u32::from(b)))
                .saturating_add(5000))
                / 10000)
                .min(255) as u8;

            if let Some(slot) = luminance.get_mut(y as usize) {
                *slot = slot.saturating_add(1);
            }
            if let Some(slot) = red.get_mut(r as usize) {
                *slot = slot.saturating_add(1);
            }
            if let Some(slot) = green.get_mut(g as usize) {
                *slot = slot.saturating_add(1);
            }
            if let Some(slot) = blue.get_mut(b as usize) {
                *slot = slot.saturating_add(1);
            }

            if y < min_lum {
                min_lum = y;
            }
            if y > max_lum {
                max_lum = y;
            }
            total_pixels = total_pixels.saturating_add(1);
        }

        if total_pixels == 0 {
            return Self {
                luminance,
                red,
                green,
                blue,
                total_pixels: 0,
                min_luminance: 0,
                max_luminance: 0,
                mean: 0.0,
                std_dev: 0.0,
                p1: 0,
                p50: 128,
                p99: 255,
                dynamic_range: 255,
                scene: SceneType::Normal,
            };
        }

        let p1 = compute_percentile(&luminance, total_pixels, 0.01);
        let p50 = compute_percentile(&luminance, total_pixels, 0.50);
        let p99 = compute_percentile(&luminance, total_pixels, 0.99);
        let dynamic_range = p99.saturating_sub(p1);

        let n = total_pixels as f64;
        let mut sum = 0.0f64;
        for (i, &count) in luminance.iter().enumerate() {
            sum += (i as f64) * (count as f64);
        }
        let mean = sum / n;

        let mut var_sum = 0.0f64;
        for (i, &count) in luminance.iter().enumerate() {
            let diff = (i as f64) - mean;
            var_sum += diff * diff * (count as f64);
        }
        let std_dev = (var_sum / n).sqrt();

        let scene = classify_scene(mean, std_dev, p1, p50, p99, dynamic_range);

        Self {
            luminance,
            red,
            green,
            blue,
            total_pixels,
            min_luminance: min_lum,
            max_luminance: max_lum,
            mean,
            std_dev,
            p1,
            p50,
            p99,
            dynamic_range,
            scene,
        }
    }

    /// Calculate an arbitrary percentile from 0.0% to 100.0%.
    pub fn percentile(&self, pct: f64) -> u8 {
        if self.total_pixels == 0 {
            return 128;
        }
        let clamped = pct.clamp(0.0, 100.0) / 100.0;
        compute_percentile(&self.luminance, self.total_pixels, clamped)
    }
}

fn compute_percentile(bins: &[u32; 256], total_pixels: u64, fraction: f64) -> u8 {
    let target = ((total_pixels as f64) * fraction).round() as u64;
    let target = target.max(1);
    let mut accumulated = 0u64;

    for (bin_idx, &count) in bins.iter().enumerate() {
        accumulated = accumulated.saturating_add(u64::from(count));
        if accumulated >= target {
            return bin_idx as u8;
        }
    }
    255
}

fn classify_scene(mean: f64, std_dev: f64, p1: u8, p50: u8, p99: u8, dynamic_range: u8) -> SceneType {
    // 1. Extreme low-key / underexposed
    if p50 < 70 && p99 < 175 && mean < 80.0 {
        return SceneType::Underexposed;
    }
    // 2. Extreme high-key / overexposed
    if p50 > 185 && p1 > 80 && mean > 175.0 {
        return SceneType::Overexposed;
    }
    // 3. Flat / hazy / low dynamic range
    if dynamic_range < 110 && std_dev < 38.0 {
        return SceneType::LowContrast;
    }
    // 4. Harsh sunlight / bimodal / high dynamic range
    if dynamic_range > 225 && std_dev > 68.0 {
        return SceneType::HighContrast;
    }
    // 5. Normal balanced scene
    SceneType::Normal
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn empty_image_returns_safe_defaults() {
        let img = RgbaImage::new(0, 0);
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.total_pixels, 0);
        assert_eq!(hist.scene, SceneType::Normal);
        assert_eq!(hist.p50, 128);
    }

    #[test]
    fn fully_transparent_image_returns_safe_defaults() {
        let mut img = RgbaImage::new(10, 10);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 0]); // Transparent
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.total_pixels, 0);
    }

    #[test]
    fn uniform_gray_image_metrics() {
        let mut img = RgbaImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([128, 128, 128, 255]);
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.total_pixels, 10_000);
        assert_eq!(hist.min_luminance, 128);
        assert_eq!(hist.max_luminance, 128);
        assert!((hist.mean - 128.0).abs() < 0.1);
        assert!(hist.std_dev < 0.001);
        assert_eq!(hist.p1, 128);
        assert_eq!(hist.p50, 128);
        assert_eq!(hist.p99, 128);
        assert_eq!(hist.dynamic_range, 0);
        assert_eq!(hist.scene, SceneType::LowContrast);
    }

    #[test]
    fn underexposed_scene_detected() {
        let mut img = RgbaImage::new(50, 50);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([30, 30, 30, 255]);
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.scene, SceneType::Underexposed);
    }

    #[test]
    fn overexposed_scene_detected() {
        let mut img = RgbaImage::new(50, 50);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([230, 230, 230, 255]);
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.scene, SceneType::Overexposed);
    }

    #[test]
    fn gradient_image_percentiles() {
        let mut img = RgbaImage::new(256, 1);
        for x in 0..256 {
            img.put_pixel(x, 0, Rgba([x as u8, x as u8, x as u8, 255]));
        }
        let hist = PhotoHistogram::from_rgba8(&img);
        assert_eq!(hist.total_pixels, 256);
        assert_eq!(hist.min_luminance, 0);
        assert_eq!(hist.max_luminance, 255);
        assert!(hist.p1 <= 3);
        assert!((hist.p50 as i32 - 128).abs() <= 2);
        assert!(hist.p99 >= 252);
        assert!(hist.dynamic_range >= 249);
    }
}
