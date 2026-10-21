//! Gradient masks: each gradient's local adjustments, scaled by how
//! strongly its mask covers the pixel.

use rustyedit_core::{DevelopSettings, Gradient, LocalAdjust};

use super::{effects::Dehaze, luma};

pub(crate) struct LocalAdjustments {
    /// Gradients as drawn on the rendered (rotated) image.
    gradients: Vec<Gradient>,
    width: f32,
    height: f32,
}

impl LocalAdjustments {
    /// `None` when no gradient changes anything.
    pub(crate) fn new(settings: &DevelopSettings, width: u32, height: u32) -> Option<Self> {
        if !settings.gradients.is_active() {
            return None;
        }
        Some(Self {
            gradients: settings
                .gradients
                .as_slice()
                .iter()
                .filter(|g| !g.adjust.is_neutral())
                .map(|g| g.on_display(settings))
                .collect(),
            width: width as f32,
            height: height as f32,
        })
    }

    pub(crate) fn apply(&self, mut rgb: [f32; 3], x: u32, y: u32) -> [f32; 3] {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        for gradient in &self.gradients {
            let mask = gradient.mask_at(px, py, self.width, self.height);
            if mask > 0.001 {
                rgb = adjust(rgb, &gradient.adjust, mask);
            }
        }
        rgb
    }
}

/// `adjust` applied at `strength` (0..=1), using the same responses as the
/// global sliders of the same names.
fn adjust(rgb: [f32; 3], adjust: &LocalAdjust, strength: f32) -> [f32; 3] {
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
        let local = LocalAdjustments::new(&settings, 100, 100).unwrap();
        let grey = [0.5; 3];
        assert!(local.apply(grey, 50, 0)[0] < 0.2, "top is darkened");
        assert_eq!(local.apply(grey, 50, 90), grey, "bottom untouched");
    }
}
