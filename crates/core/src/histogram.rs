use crate::Adjustment;

/// Per-channel tonal distribution of an image: how many pixels fall into
/// each of 256 brightness levels for red, green, blue and luminance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub red: [u32; 256],
    pub green: [u32; 256],
    pub blue: [u32; 256],
    pub luma: [u32; 256],
}

/// Which distribution a histogram view draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistogramChannel {
    /// Red, green and blue overlaid.
    #[default]
    Rgb,
    Luma,
    Red,
    Green,
    Blue,
}

impl HistogramChannel {
    pub const ALL: [HistogramChannel; 5] = [
        HistogramChannel::Rgb,
        HistogramChannel::Luma,
        HistogramChannel::Red,
        HistogramChannel::Green,
        HistogramChannel::Blue,
    ];

    /// Short tab label.
    pub fn label(&self) -> &'static str {
        match self {
            HistogramChannel::Rgb => "RGB",
            HistogramChannel::Luma => "L",
            HistogramChannel::Red => "R",
            HistogramChannel::Green => "G",
            HistogramChannel::Blue => "B",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            HistogramChannel::Rgb => "Red, green and blue",
            HistogramChannel::Luma => "Luminance",
            HistogramChannel::Red => "Red",
            HistogramChannel::Green => "Green",
            HistogramChannel::Blue => "Blue",
        }
    }
}

/// The five tonal ranges a histogram is read in, left to right, each
/// driven by the Basic slider of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TonalZone {
    Blacks,
    Shadows,
    Exposure,
    Highlights,
    Whites,
}

impl TonalZone {
    pub const ALL: [TonalZone; 5] = [
        TonalZone::Blacks,
        TonalZone::Shadows,
        TonalZone::Exposure,
        TonalZone::Highlights,
        TonalZone::Whites,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            TonalZone::Blacks => "Blacks",
            TonalZone::Shadows => "Shadows",
            TonalZone::Exposure => "Exposure",
            TonalZone::Highlights => "Highlights",
            TonalZone::Whites => "Whites",
        }
    }

    /// The levels it covers, as fractions of the range `0..=1`.
    pub fn span(&self) -> (f32, f32) {
        match self {
            TonalZone::Blacks => (0.0, 0.12),
            TonalZone::Shadows => (0.12, 0.38),
            TonalZone::Exposure => (0.38, 0.65),
            TonalZone::Highlights => (0.65, 0.88),
            TonalZone::Whites => (0.88, 1.0),
        }
    }

    /// The zone containing `level` (a fraction of the range).
    pub fn at(level: f32) -> TonalZone {
        let level = level.clamp(0.0, 1.0);
        Self::ALL
            .into_iter()
            .find(|zone| level < zone.span().1)
            .unwrap_or(TonalZone::Whites)
    }

    pub fn adjustment(&self) -> Adjustment {
        match self {
            TonalZone::Blacks => Adjustment::Blacks,
            TonalZone::Shadows => Adjustment::Shadows,
            TonalZone::Exposure => Adjustment::Exposure,
            TonalZone::Highlights => Adjustment::Highlights,
            TonalZone::Whites => Adjustment::Whites,
        }
    }
}

/// Share of pixels pinned at either end of the range, per channel, as
/// fractions of all pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Clipping {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Clipping {
    /// Below this a channel isn't flagged as clipped: a handful of pure
    /// black or white pixels is normal.
    pub const THRESHOLD: f32 = 0.001;

    pub fn worst(&self) -> f32 {
        self.red.max(self.green).max(self.blue)
    }

    /// Which channels are flagged, as `(red, green, blue)`.
    pub fn flagged(&self) -> (bool, bool, bool) {
        (
            self.red > Self::THRESHOLD,
            self.green > Self::THRESHOLD,
            self.blue > Self::THRESHOLD,
        )
    }
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
            histogram.luma[luma8(r, g, b) as usize] += 1;
        }
        histogram
    }

    /// The bins `channel` draws (for RGB, the luminance stands in where a
    /// single series is needed).
    pub fn bins(&self, channel: HistogramChannel) -> &[u32; 256] {
        match channel {
            HistogramChannel::Rgb | HistogramChannel::Luma => &self.luma,
            HistogramChannel::Red => &self.red,
            HistogramChannel::Green => &self.green,
            HistogramChannel::Blue => &self.blue,
        }
    }

    /// Number of pixels counted.
    pub fn total(&self) -> u32 {
        self.luma.iter().sum()
    }

    /// The tallest bin across the colour channels, ignoring the pure black
    /// and pure white bins. Clipped pixels pile up there and would
    /// otherwise flatten the rest of the graph, so they're excluded from
    /// scaling (and simply drawn clipped at full height).
    pub fn peak(&self) -> u32 {
        [&self.red, &self.green, &self.blue, &self.luma]
            .iter()
            .flat_map(|channel| channel[1..255].iter().copied())
            .max()
            .unwrap_or(0)
    }

    /// Pixels at pure black in each channel.
    pub fn shadow_clipping(&self) -> Clipping {
        self.clipping(0)
    }

    /// Pixels at pure white in each channel.
    pub fn highlight_clipping(&self) -> Clipping {
        self.clipping(255)
    }

    fn clipping(&self, level: usize) -> Clipping {
        let total = self.total().max(1) as f32;
        Clipping {
            red: self.red[level] as f32 / total,
            green: self.green[level] as f32 / total,
            blue: self.blue[level] as f32 / total,
        }
    }

    /// Average luminance, `0..=255`.
    pub fn mean_luma(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let sum: u64 = self
            .luma
            .iter()
            .enumerate()
            .map(|(level, &count)| level as u64 * count as u64)
            .sum();
        sum as f32 / total as f32
    }

    /// Share of pixels at `level` in `channel` (luminance for RGB).
    pub fn share_at(&self, channel: HistogramChannel, level: usize) -> f32 {
        let total = self.total().max(1) as f32;
        self.bins(channel)[level.min(255)] as f32 / total
    }
}

/// Rec. 709 luma of an 8-bit pixel, in fixed point (weights sum to 256).
pub fn luma8(r: u8, g: u8, b: u8) -> u8 {
    ((54 * r as u32 + 183 * g as u32 + 19 * b as u32) >> 8) as u8
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
        assert_eq!(h.total(), 3);
    }

    #[test]
    fn peak_ignores_clipped_extremes() {
        let mut pixels = vec![0u8; 3 * 100];
        pixels.extend_from_slice(&[128, 128, 128, 128, 128, 128]);
        assert_eq!(Histogram::from_rgb8(&pixels).peak(), 2);
    }

    #[test]
    fn clipping_is_reported_per_channel() {
        // One of four pixels has blown red; one is pure black.
        let h = Histogram::from_rgb8(&[255, 10, 10, 0, 0, 0, 50, 50, 50, 90, 90, 90]);
        let high = h.highlight_clipping();
        assert_eq!(high.red, 0.25);
        assert_eq!(high.green, 0.0);
        assert_eq!(high.flagged(), (true, false, false));
        assert_eq!(h.shadow_clipping().worst(), 0.25);
    }

    #[test]
    fn zones_cover_the_range_in_order() {
        assert_eq!(TonalZone::at(0.0), TonalZone::Blacks);
        assert_eq!(TonalZone::at(0.5), TonalZone::Exposure);
        assert_eq!(TonalZone::at(1.0), TonalZone::Whites);
        for pair in TonalZone::ALL.windows(2) {
            assert_eq!(pair[0].span().1, pair[1].span().0);
        }
    }
}
