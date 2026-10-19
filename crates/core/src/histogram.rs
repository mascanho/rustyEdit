/// Per-channel tonal distribution of an image: how many pixels fall into
/// each of 256 brightness levels for red, green, blue and luminance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub red: [u32; 256],
    pub green: [u32; 256],
    pub blue: [u32; 256],
    pub luma: [u32; 256],
}

impl Histogram {
    /// Builds a histogram from tightly packed 8-bit RGB pixels
    /// (`[r, g, b, r, g, b, …]`), e.g. `image::RgbImage::as_raw()`.
    pub fn from_rgb8(pixels: &[u8]) -> Self {
        let mut histogram = Self {
            red: [0; 256],
            green: [0; 256],
            blue: [0; 256],
            luma: [0; 256],
        };
        for px in pixels.chunks_exact(3) {
            let (r, g, b) = (px[0], px[1], px[2]);
            histogram.red[r as usize] += 1;
            histogram.green[g as usize] += 1;
            histogram.blue[b as usize] += 1;
            // Rec. 709 luma in fixed point (weights sum to 256).
            let luma = (54 * r as u32 + 183 * g as u32 + 19 * b as u32) >> 8;
            histogram.luma[luma as usize] += 1;
        }
        histogram
    }

    /// The tallest bin across the colour channels, ignoring the pure black
    /// and pure white bins. Clipped pixels pile up there and would
    /// otherwise flatten the rest of the graph, so they're excluded from
    /// scaling (and simply drawn clipped at full height).
    pub fn peak(&self) -> u32 {
        [&self.red, &self.green, &self.blue]
            .iter()
            .flat_map(|channel| channel[1..255].iter().copied())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_channel_and_luma() {
        let h = Histogram::from_rgb8(&[255, 0, 0, 255, 255, 255, 0, 0, 0]);
        assert_eq!(h.red[255], 2);
        assert_eq!(h.green[0], 2);
        assert_eq!(h.luma[255], 1);
        assert_eq!(h.luma[0], 1);
        assert_eq!(h.luma.iter().sum::<u32>(), 3);
    }

    #[test]
    fn peak_ignores_clipped_extremes() {
        let mut pixels = vec![0u8; 3 * 100];
        pixels.extend_from_slice(&[128, 128, 128, 128, 128, 128]);
        assert_eq!(Histogram::from_rgb8(&pixels).peak(), 2);
    }
}
