//! Gradient masks: each gradient's local adjustments, scaled by how
//! strongly its mask covers the pixel.

use image::{imageops, RgbImage};
use rustyedit_core::{DevelopSettings, Gradient, LocalAdjust};

use super::{effects::Dehaze, grading::Grading, luma};

pub(crate) struct LocalAdjustments {
    /// Gradients as drawn on the rendered (rotated) image.
    gradients: Vec<Gradient>,
    width: f32,
    height: f32,
    /// The photo blurred wide (for local clarity) and narrow (for local
    /// sharpening); only made when some layer uses them.
    wide: Option<RgbImage>,
    narrow: Option<RgbImage>,
    /// Each layer's colour grading, in the same order as `gradients`.
    gradings: Vec<Option<Grading>>,
}

impl LocalAdjustments {
    /// `None` when no gradient changes anything.
    pub(crate) fn new(settings: &DevelopSettings, image: &RgbImage) -> Option<Self> {
        if !settings.gradients.is_active() {
            return None;
        }
        let (width, height) = image.dimensions();
        let gradients: Vec<Gradient> = settings
            .gradients
            .as_slice()
            .iter()
            .filter(|g| g.is_visible())
            .map(|g| g.on_display(settings))
            .collect();
        // Blur radii follow the photo's size, so a layer looks the same on
        // a preview as on the full-size export.
        let scale = width.max(height) as f32 / 2048.0;
        // A layer that blurs needs a wider blur than clarity's (they share it).
        let lens = gradients.iter().any(|g| g.adjust.blur != 0.0);
        let wide_sigma = ((if lens { 16.0 } else { 10.0 }) * scale).max(4.0);
        let wide = gradients
            .iter()
            .any(|g| g.adjust.clarity != 0.0 || g.adjust.blur != 0.0)
            .then(|| imageops::blur(image, wide_sigma));
        let narrow = gradients
            .iter()
            .any(|g| g.adjust.sharpening != 0.0)
            .then(|| imageops::blur(image, (1.2 * scale).max(0.8)));
        let gradings = gradients
            .iter()
            .map(|g| Grading::new(&g.adjust.grading))
            .collect();
        Some(Self {
            gradients,
            width: width as f32,
            height: height as f32,
            wide,
            narrow,
            gradings,
        })
    }

    pub(crate) fn apply(&self, mut rgb: [f32; 3], x: u32, y: u32) -> [f32; 3] {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        for (gradient, grading) in self.gradients.iter().zip(&self.gradings) {
            let mut mask = gradient.mask_at(px, py, self.width, self.height);
            // Brightness and colour ranges narrow the layer, judged on the
            // pixel as the layers before it left it.
            if mask > 0.001 && gradient.has_range() {
                mask *= gradient.range_at(rgb);
            }
            if mask > 0.001 {
                let blur = |image: &Option<RgbImage>| {
                    image.as_ref().map(|image| {
                        let p =
                            image.get_pixel(x.min(image.width() - 1), y.min(image.height() - 1));
                        [
                            p[0] as f32 / 255.0,
                            p[1] as f32 / 255.0,
                            p[2] as f32 / 255.0,
                        ]
                    })
                };
                rgb = adjust(
                    rgb,
                    &gradient.adjust,
                    mask,
                    blur(&self.wide),
                    blur(&self.narrow),
                    grading.as_ref(),
                );
            }
        }
        rgb
    }
}

/// `adjust` applied at `strength` (0..=1), using the same responses as the
/// global sliders of the same names.
fn adjust(
    rgb: [f32; 3],
    adjust: &LocalAdjust,
    strength: f32,
    wide_blur: Option<[f32; 3]>,
    narrow_blur: Option<[f32; 3]>,
    grading: Option<&Grading>,
) -> [f32; 3] {
    let amount = |value: f32| value / 100.0 * strength;
    let mut c = rgb;

    if adjust.exposure != 0.0 {
        let factor = 2f32.powf(amount(adjust.exposure) * 2.0);
        c = c.map(|v| v * factor);
    }
    if adjust.shadows != 0.0 || adjust.highlights != 0.0 {
        let l = luma(c).clamp(0.0, 1.0);
        let lift = amount(adjust.shadows) * (1.0 - l).powf(1.5) * 0.5
            + amount(adjust.highlights) * l.powf(1.5) * 0.35;
        c = c.map(|v| v + lift);
    }
    if adjust.contrast != 0.0 {
        let factor = 1.0 + amount(adjust.contrast);
        c = c.map(|v| (v - 0.5) * factor + 0.5);
    }
    if adjust.temperature != 0.0 || adjust.tint != 0.0 {
        let (temp, tint) = (amount(adjust.temperature), amount(adjust.tint));
        c[0] += temp * 0.12 - tint * 0.06;
        c[1] += tint * 0.12;
        c[2] += -temp * 0.12 - tint * 0.06;
    }
    if adjust.saturation != 0.0 {
        let l = luma(c);
        let factor = 1.0 + amount(adjust.saturation);
        c = c.map(|v| l + (v - l) * factor);
    }
    if let Some(dehaze) = Dehaze::new(adjust.dehaze * strength) {
        c = dehaze.apply(c.map(|v| v.clamp(0.0, 1.0)));
    }
    // Lens blur: towards the wide blur of the photo.
    if let (true, Some(blur)) = (adjust.blur != 0.0, wide_blur) {
        let k = amount(adjust.blur);
        for i in 0..3 {
            c[i] += (blur[i] - c[i]) * k;
        }
    }
    // Local contrast and sharpness: push the pixel away from (or, when
    // negative, towards) a blurred copy of the photo.
    if let (true, Some(blur)) = (adjust.clarity != 0.0, wide_blur) {
        let k = amount(adjust.clarity) * 0.9;
        for i in 0..3 {
            c[i] += (c[i] - blur[i]) * k;
        }
    }
    if let (true, Some(blur)) = (adjust.sharpening != 0.0, narrow_blur) {
        let k = amount(adjust.sharpening) * 1.6;
        for i in 0..3 {
            c[i] += (c[i] - blur[i]) * k;
        }
    }
    // Colour grading, faded in with the mask.
    if let Some(grading) = grading {
        let graded = grading.apply(c.map(|v| v.clamp(0.0, 1.0)));
        for i in 0..3 {
            c[i] += (graded[i] - c[i]) * strength;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::GradientKind;

    #[test]
    fn linear_exposure_darkens_only_the_start_side() {
        let mut settings = DevelopSettings::default();
        settings.gradients.push(Gradient {
            kind: GradientKind::Linear,
            start: (0.5, 0.0),
            end: (0.5, 0.5),
            adjust: LocalAdjust {
                exposure: -100.0,
                ..Default::default()
            },
            ..Default::default()
        });
        let local = LocalAdjustments::new(&settings, &RgbImage::new(100, 100)).unwrap();
        let grey = [0.5; 3];
        assert!(local.apply(grey, 50, 0)[0] < 0.2, "top is darkened");
        assert_eq!(local.apply(grey, 50, 90), grey, "bottom untouched");
    }

    #[test]
    fn a_range_layer_only_touches_the_chosen_tones() {
        let mut settings = DevelopSettings::default();
        settings.gradients.push(Gradient {
            kind: GradientKind::Range,
            luma_range: (0.0, 0.4),
            luma_feather: 0.05,
            adjust: LocalAdjust {
                exposure: 100.0,
                ..Default::default()
            },
            ..Default::default()
        });
        let local = LocalAdjustments::new(&settings, &RgbImage::new(10, 10)).unwrap();
        assert!(local.apply([0.2; 3], 5, 5)[0] > 0.4, "shadows are lifted");
        assert_eq!(local.apply([0.8; 3], 5, 5), [0.8; 3], "highlights are left");
    }

    #[test]
    fn a_brush_layer_only_touches_where_it_was_painted() {
        let mut brush = Gradient {
            kind: GradientKind::Brush,
            adjust: LocalAdjust {
                exposure: -100.0,
                ..Default::default()
            },
            ..Default::default()
        };
        brush.add_dab(rustyedit_core::Dab { x: 0.25, y: 0.5, radius: 0.1, erase: false });
        let mut settings = DevelopSettings::default();
        settings.gradients.push(brush);
        let local = LocalAdjustments::new(&settings, &RgbImage::new(100, 100)).unwrap();
        assert!(local.apply([0.5; 3], 25, 50)[0] < 0.2);
        assert_eq!(local.apply([0.5; 3], 80, 50), [0.5; 3]);
    }

    #[test]
    fn a_blurring_layer_softens_only_its_area() {
        // A dot in a dark field: inside the layer it is blurred away.
        let mut image = RgbImage::from_pixel(64, 64, image::Rgb([20, 20, 20]));
        for y in 30..34 {
            for x in 30..34 {
                image.put_pixel(x, y, image::Rgb([240, 240, 240]));
            }
        }
        let mut settings = DevelopSettings::default();
        settings.gradients.push(Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            radius: (0.2, 0.2),
            feather: 0.0,
            adjust: LocalAdjust {
                blur: 100.0,
                ..Default::default()
            },
            ..Default::default()
        });
        let local = LocalAdjustments::new(&settings, &image).unwrap();
        let sharp = [240.0 / 255.0; 3];
        let inside = local.apply(sharp, 31, 31);
        assert!(inside[0] < 0.8, "the dot is softened: {inside:?}");
        let outside = local.apply([0.9; 3], 2, 2);
        assert_eq!(outside, [0.9; 3]);
    }

    #[test]
    fn a_layer_can_add_local_clarity_sharpening_and_grading() {
        let image = RgbImage::from_pixel(64, 64, image::Rgb([128, 128, 128]));
        let mut settings = DevelopSettings::default();
        settings.gradients.push(Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            radius: (0.5, 0.5),
            feather: 0.0,
            adjust: LocalAdjust {
                clarity: 100.0,
                sharpening: 100.0,
                grading: {
                    let mut grading = rustyedit_core::ColorGrading::default();
                    grading.midtones.hue = 0.0;
                    grading.midtones.saturation = 100.0;
                    grading
                },
                ..Default::default()
            },
            ..Default::default()
        });
        let local = LocalAdjustments::new(&settings, &image).unwrap();
        assert!(local.wide.is_some() && local.narrow.is_some());
        // A pixel brighter than its surroundings gets pushed brighter...
        let bright = local.apply([0.9, 0.5, 0.5], 32, 32);
        let flat = LocalAdjustments::new(
            &DevelopSettings {
                gradients: settings.gradients,
                ..Default::default()
            },
            &image,
        )
        .unwrap();
        // ...and a red midtone grade raises red relative to blue.
        assert!(bright[0] - bright[2] > flat.apply([0.5; 3], 32, 32)[0] - 0.5);
        let tinted = flat.apply([0.5; 3], 32, 32);
        assert!(tinted[0] > tinted[2]);
    }
}
