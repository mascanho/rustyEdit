//! Frames: draws the border of a [`FrameSettings`] around a finished
//! photo, growing the picture (see `rustyedit_core::frame`).
//!
//! Every style is a small function of the pixel's place: it says what the
//! border looks like there, how much of the photo shows through (a rounded
//! window, an oval, a ragged edge) and anything drawn over the photo (a
//! keyline, a bevel). Colours are display values, 0..=1, whatever the
//! output encoding: greys and white frame the same way in any space.

use image::{ImageBuffer, Rgb, RgbImage, RgbaImage};
use rustyedit_core::{FrameSettings, FrameStyle};

type Colour = [f32; 3];

/// What a frame style makes of one pixel.
struct Shade {
    /// The border's colour here.
    frame: Colour,
    /// How much of the photo shows (0..=1).
    cover: f32,
    /// Drawn over everything: colour and opacity.
    over: [f32; 4],
}

/// A 32×32 average of the photo, for the soft backdrop of "Blur fill".
const LOWRES: usize = 32;

struct Framer {
    style: FrameStyle,
    /// Output size, and the photo's place in it.
    ow: f32,
    oh: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    pw: f32,
    ph: f32,
    /// The border unit, in pixels.
    m: f32,
    margins: [f32; 4],
    base: Colour,
    corner: f32,
    line: f32,
    shadow: f32,
    lowres: Vec<Colour>,
}

impl Framer {
    fn new(width: u32, height: u32, f: &FrameSettings, fetch: &dyn Fn(u32, u32) -> Colour) -> Self {
        let [l, t, r, b] = f.margins(width, height);
        let (pw, ph) = (width as f32, height as f32);
        let mut lowres = Vec::new();
        if f.style == FrameStyle::BlurFill {
            for gy in 0..LOWRES {
                for gx in 0..LOWRES {
                    let mut sum = [0.0f32; 3];
                    for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                        let x = (((gx as f32 + ox) / LOWRES as f32) * pw) as u32;
                        let y = (((gy as f32 + oy) / LOWRES as f32) * ph) as u32;
                        let c = fetch(x.min(width - 1), y.min(height - 1));
                        for i in 0..3 {
                            sum[i] += c[i] / 4.0;
                        }
                    }
                    lowres.push(sum);
                }
            }
        }
        Self {
            style: f.style,
            ow: pw + (l + r) as f32,
            oh: ph + (t + b) as f32,
            x0: l as f32,
            y0: t as f32,
            x1: l as f32 + pw,
            y1: t as f32 + ph,
            pw,
            ph,
            m: f.unit(width, height),
            margins: [l as f32, t as f32, r as f32, b as f32],
            base: hsl(f.hue, f.saturation / 100.0, f.lightness / 100.0),
            corner: (f.corner / 100.0).clamp(0.0, 1.0),
            line: (f.line / 100.0).clamp(0.0, 1.0),
            shadow: (f.shadow / 100.0).clamp(0.0, 1.0),
            lowres,
        }
    }

    fn out_size(&self) -> (u32, u32) {
        (self.ow as u32, self.oh as u32)
    }

    /// The finished pixel at (`x`, `y`) of the framed picture.
    fn pixel(&self, x: u32, y: u32, fetch: &dyn Fn(u32, u32) -> Colour) -> Colour {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let shade = self.shade(fx, fy);
        let mut out = shade.frame;
        if shade.cover > 0.0 {
            let sx = (x as f32 - self.x0).clamp(0.0, self.pw - 1.0) as u32;
            let sy = (y as f32 - self.y0).clamp(0.0, self.ph - 1.0) as u32;
            out = mix(out, fetch(sx, sy), shade.cover);
        }
        if shade.over[3] > 0.0 {
            out = mix(out, [shade.over[0], shade.over[1], shade.over[2]], shade.over[3]);
        }
        out
    }

    fn shade(&self, fx: f32, fy: f32) -> Shade {
        let m = self.m;
        let (cx, cy) = ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0);
        let (hx, hy) = (self.pw / 2.0, self.ph / 2.0);
        // Distance outside the photo's rectangle (negative inside).
        let d = sd_box(fx - cx, fy - cy, hx, hy);
        let base = self.base;
        let dark = scale(base, 0.45);
        let mut s = Shade {
            frame: base,
            cover: smooth(d),
            over: [0.0; 4],
        };
        let (k_corner, k_line, k_shadow) = (self.corner, self.line, self.shadow);
        // Distance to the canvas's edge.
        let edge = fx.min(fy).min(self.ow - fx).min(self.oh - fy);
        let paper = |c: Colour, grain: f32| {
            let n = noise(fx * 0.9, fy * 0.9, 7) - 0.5;
            scale(c, 1.0 + n * grain)
        };

        match self.style {
            FrameStyle::None => {}
            FrameStyle::Border => {
                let t = (k_line * m * 0.25).max(0.0);
                if t > 0.0 {
                    s.over = with_alpha(dark, band(d, 0.0, t.max(1.0)) * 0.9);
                }
            }
            FrameStyle::Matte => {
                s.frame = paper(base, 0.03);
                let t = (k_line * m * 0.12).max(1.0);
                s.over = with_alpha(dark, band(d, 0.0, t) * 0.8);
                let depth = k_shadow * m * 0.4;
                if d < 0.0 && depth > 0.0 {
                    let a = 0.35 * k_shadow * (1.0 + d / depth).clamp(0.0, 1.0);
                    s.over = blend_over(s.over, [0.0, 0.0, 0.0, a]);
                }
            }
            FrameStyle::FineArt => {
                s.frame = paper(base, 0.02);
                let gap = m * 0.5;
                let t = (k_line * m * 0.08).max(1.0);
                s.over = with_alpha([0.05; 3], band(d, gap, gap + t) * 0.9);
            }
            FrameStyle::Gallery => {
                s.frame = paper(base, 0.025);
                let depth = (k_shadow * m * 0.9).max(1.0);
                if d < 0.0 {
                    s.over = [0.0, 0.0, 0.0, 0.5 * k_shadow * (1.0 + d / depth).clamp(0.0, 1.0)];
                } else {
                    let cast = m * 0.5;
                    let a = 0.2 * k_shadow * (1.0 - d / cast).clamp(0.0, 1.0);
                    s.frame = mix(s.frame, [0.0; 3], a);
                }
            }
            FrameStyle::Passepartout => {
                let ring = m * 1.1;
                let bevel = m * (0.18 + 0.4 * k_line);
                let inner = lighten(base, 0.2);
                s.frame = paper(if d < ring { inner } else { base }, 0.03);
                s.frame = mix(s.frame, scale(base, 0.5), band(d, ring, ring + 1.0) * 0.7);
                if d >= 0.0 && d < bevel {
                    // The cut edge of the board, lit from the top left.
                    let lit = (self.x0 - fx).max(self.y0 - fy) > (fx - self.x1).max(fy - self.y1);
                    s.frame = if lit { [0.97, 0.97, 0.95] } else { [0.8, 0.8, 0.78] };
                }
            }
            FrameStyle::DoubleLine => {
                s.frame = paper(base, 0.02);
                let t = (k_line * m * 0.1).max(1.0);
                let line = band(d, m * 0.45, m * 0.45 + t).max(band(d, m * 0.95, m * 0.95 + t));
                s.over = with_alpha(scale(base, 0.3), line * 0.85);
            }
            FrameStyle::Rounded | FrameStyle::Shadow | FrameStyle::Gradient | FrameStyle::BlurFill => {
                let r = k_corner * 0.25 * self.pw.min(self.ph);
                let rd = sd_rbox(fx - cx, fy - cy, hx, hy, r);
                s.cover = smooth(rd);
                match self.style {
                    FrameStyle::Rounded => {
                        let t = k_line * m * 0.15;
                        if t > 0.0 {
                            s.over = with_alpha(dark, band(rd, 0.0, t.max(1.0)) * 0.9);
                        }
                    }
                    FrameStyle::Shadow => {
                        s.frame = drop_shadow(base, fx, fy - m * 0.4 * k_shadow, cx, cy, hx, hy, r, m, k_shadow, 0.55);
                    }
                    FrameStyle::Gradient => {
                        let t = (fx / self.ow + fy / self.oh) / 2.0;
                        let (h, sat, l) = rgb_to_hsl(base);
                        s.frame = mix(hsl(h, sat, l), hsl(h + 70.0, sat, l), t);
                        s.frame = drop_shadow(s.frame, fx, fy - m * 0.3 * k_shadow, cx, cy, hx, hy, r, m, k_shadow, 0.4);
                    }
                    _ => {
                        s.frame = self.blurred_backdrop(fx, fy);
                        s.frame = drop_shadow(s.frame, fx, fy - m * 0.4 * k_shadow, cx, cy, hx, hy, r, m, k_shadow, 0.55);
                    }
                }
            }
            FrameStyle::Polaroid | FrameStyle::Instax => {
                let instax = self.style == FrameStyle::Instax;
                let rc = if instax { m * (0.3 + k_corner * 2.0) } else { m * k_corner * 1.2 };
                let card = sd_rbox(fx - self.ow / 2.0, fy - self.oh / 2.0, self.ow / 2.0, self.oh / 2.0, rc);
                let mut paper_colour = paper(base, 0.03);
                // Slightly lighter at the top, and worn towards the edge.
                paper_colour = scale(paper_colour, 1.02 - 0.05 * fy / self.oh);
                let wear = (1.0 - edge / (m * 0.6)).clamp(0.0, 1.0);
                paper_colour = mix(paper_colour, scale(paper_colour, 0.82), 0.5 * k_shadow * wear * wear);
                s.frame = mix([0.09; 3], paper_colour, smooth(card));
                let r = if instax { m * 0.12 } else { 0.0 };
                let rd = sd_rbox(fx - cx, fy - cy, hx, hy, r);
                s.cover = smooth(rd);
                let depth = m * 0.18;
                let a = if rd < 0.0 { 0.4 * k_shadow * (1.0 + rd / depth).clamp(0.0, 1.0) } else { 0.0 };
                s.over = [0.0, 0.0, 0.0, a.max(band(rd, 0.0, 1.0) * 0.25)];
            }
            FrameStyle::Vintage | FrameStyle::Deckle => {
                let vintage = self.style == FrameStyle::Vintage;
                let freq = if vintage { 1.0 / (m * 0.6) } else { 1.0 / (m * 0.22) };
                let wobble = fbm(fx * freq, fy * freq) * 2.0 - 1.0;
                let rim = if vintage { m * 0.2 + wobble * m * 0.12 } else { m * 0.1 + wobble * m * 0.1 };
                let backdrop = if vintage { [0.16, 0.11, 0.08] } else { [0.08; 3] };
                let stain = fbm(fx / (m * 4.0), fy / (m * 4.0));
                let mut p = paper(base, 0.05);
                p = scale(p, 0.93 + 0.1 * stain);
                let t = (1.0 - edge / (m * 1.3)).clamp(0.0, 1.0);
                let tone = if vintage { [0.45, 0.3, 0.15] } else { [0.7, 0.68, 0.62] };
                p = mix(p, tone, 0.45 * k_shadow * t * t * (0.5 + 0.5 * stain));
                // Fibres at a torn edge let a little of the backdrop show.
                let fringe = ((edge - rim) / (m * 0.1)).clamp(0.0, 1.0);
                p = mix(mix(p, backdrop, 0.35), p, fringe);
                s.frame = mix(backdrop, p, smooth(rim - edge));
                let line = if vintage { [0.35, 0.25, 0.15] } else { [0.5; 3] };
                s.over = with_alpha(line, band(d, 0.0, 1.5) * 0.5);
            }
            FrameStyle::Stamp => {
                let p = m * 0.55;
                let r = p * 0.32;
                // The nearest edge decides which row of perforations.
                let (e, u) = if fx <= fy && fx <= self.ow - fx && fx <= self.oh - fy {
                    (fx, fy)
                } else if self.ow - fx <= fy && self.ow - fx <= self.oh - fy {
                    (self.ow - fx, fy)
                } else if fy <= self.oh - fy {
                    (fy, fx)
                } else {
                    (self.oh - fy, fx)
                };
                let along = u - ((u / p).floor() + 0.5) * p;
                let hole = (along * along + e * e).sqrt() - r;
                s.frame = mix(paper(base, 0.03), [0.1; 3], smooth(hole));
                let t = (k_line * m * 0.05).max(1.0);
                s.over = with_alpha(scale(base, 0.45), band(d, m * 0.45, m * 0.45 + t) * 0.8);
            }
            FrameStyle::Film35 => {
                s.frame = base;
                let r = k_corner * m * 0.9;
                s.cover = smooth(sd_rbox(fx - cx, fy - cy, hx, hy, r));
                let (top, bottom) = (self.margins[1], self.margins[3]);
                let pitch = m * 1.45;
                let slot = (fx / pitch).floor();
                let sx = fx - (slot + 0.5) * pitch;
                for (bar_top, bar_h) in [(0.0, top), (self.y1, bottom)] {
                    let bar_c = bar_top + bar_h / 2.0;
                    let hole = sd_rbox(sx, fy - bar_c, m * 0.45, m * 0.31, m * 0.12);
                    if (fy - bar_c).abs() < bar_h / 2.0 {
                        s.frame = mix(s.frame, [0.93, 0.92, 0.88], smooth(hole));
                    }
                }
                // Edge print: a bar code and the arrow, in amber.
                let amber = [1.0, 0.62, 0.12];
                let code_y = self.y0 - m * 0.4;
                let k = (fx / (m * 0.14)).floor();
                if (fy - code_y).abs() < m * 0.07 && hash2(k as i32, 3) > 0.45 && fx > m && fx < self.ow - m {
                    s.over = with_alpha(amber, 0.85);
                }
                let line_y = self.y1 + m * 0.34;
                if (fy - line_y).abs() < 0.6 && fx > self.x0 && fx < self.x1 {
                    s.over = with_alpha(amber, 0.55);
                }
                let tri = (fx - self.x0 - m * 0.4).abs() + (fy - (self.y1 + m * 0.62)).abs() * 1.4;
                if tri < m * 0.22 {
                    s.over = with_alpha(amber, 0.85);
                }
                if k_line > 0.0 {
                    s.over = blend_over(s.over, with_alpha([0.8; 3], band(d, 0.0, 1.2) * 0.35 * k_line));
                }
            }
            FrameStyle::Film120 => {
                s.frame = base;
                let r = m * 0.12 * (1.0 + k_corner * 4.0);
                s.cover = smooth(sd_rbox(fx - cx, fy - cy, hx, hy, r));
                let amber = [1.0, 0.6, 0.15];
                let tri = |x: f32, y: f32| (fx - x).abs() * 1.4 + (fy - y).abs();
                // Frame numbers' stand-ins: an arrow and a bar at the corners.
                if tri(self.x0 + m * 0.9, self.y0 - m * 0.6) < m * 0.2
                    || tri(self.x1 - m * 0.9, self.y1 + m * 0.6) < m * 0.2
                    || ((fx - (self.x0 + m * 1.6)).abs() < m * 0.5 && (fy - (self.y0 - m * 0.6)).abs() < m * 0.05)
                    || ((fx - (self.x1 - m * 1.6)).abs() < m * 0.5 && (fy - (self.y1 + m * 0.6)).abs() < m * 0.05)
                {
                    s.over = with_alpha(amber, 0.8);
                }
                // Alignment ticks between frames.
                if (fx - self.ow / 2.0).abs() < 1.0 && (fy < self.y0 - m * 0.2 || fy > self.y1 + m * 0.2) {
                    s.over = with_alpha(amber, 0.6);
                }
                if k_line > 0.0 {
                    s.over = blend_over(s.over, with_alpha([0.85; 3], band(d, 0.0, 1.0) * 0.3 * k_line));
                }
            }
            FrameStyle::Slide => {
                let rc = m * (0.5 + k_corner * 2.0);
                let card = sd_rbox(fx - self.ow / 2.0, fy - self.oh / 2.0, self.ow / 2.0, self.oh / 2.0, rc);
                let lit = 1.02 - 0.08 * k_shadow * (fx / self.ow + fy / self.oh) / 2.0 * 2.0;
                let mut c = scale(paper(base, 0.015), lit);
                // A raised rim just inside the edge.
                c = mix(c, lighten(c, 0.05), band(edge, m * 0.15, m * 0.3));
                c = mix(c, scale(c, 0.8), band(edge, m * 0.3, m * 0.36));
                let near = (1.0 - d / (m * 0.35)).clamp(0.0, 1.0);
                c = mix(c, scale(c, 0.75), 0.4 * k_shadow * near * near * (d >= 0.0) as u8 as f32);
                s.frame = mix([0.09; 3], c, smooth(card));
                let rd = sd_rbox(fx - cx, fy - cy, hx, hy, m * 0.2);
                s.cover = smooth(rd);
            }
            FrameStyle::Cinema => {
                s.frame = base;
                s.cover = smooth(d);
            }
            FrameStyle::Grunge => {
                let n = fbm(fx / (m * 0.45), fy / (m * 0.45)) * 2.0 - 1.0;
                let n2 = fbm(fx / (m * 0.09) + 40.0, fy / (m * 0.09)) * 2.0 - 1.0;
                let dd = d + n * m * 0.5 + n2 * m * 0.08 - m * 0.3;
                s.cover = smooth(dd);
                s.frame = base;
                // Dust and scratches on the rebate.
                let speck = hash2((fx / 2.0) as i32, (fy / 2.0) as i32 + 77);
                if speck > 0.9965 {
                    s.over = with_alpha([0.75; 3], 0.7);
                }
                let halo = band(dd, -m * 0.12, m * 0.12);
                s.frame = mix(base, lighten(base, 0.12), halo * 0.5);
            }
            FrameStyle::Wood | FrameStyle::Gold | FrameStyle::Metal => {
                // Which side of the moulding this pixel is on, mitred at the corners.
                let dl = self.x0 - fx;
                let dt = self.y0 - fy;
                let dr = fx - self.x1;
                let db = fy - self.y1;
                if d >= 0.0 {
                    let (side, depth, thickness) = {
                        let mut best = (0, dl, self.margins[0]);
                        for (i, (dist, th)) in [(dt, self.margins[1]), (dr, self.margins[2]), (db, self.margins[3])]
                            .into_iter()
                            .enumerate()
                        {
                            if dist > best.1 {
                                best = (i + 1, dist, th);
                            }
                        }
                        best
                    };
                    let prof = (depth / thickness.max(1.0)).clamp(0.0, 1.0);
                    let horizontal = side == 1 || side == 3;
                    let (along, cross) = if horizontal { (fx, fy) } else { (fy, fx) };
                    let (h, sat, l) = rgb_to_hsl(base);
                    let lightness = match self.style {
                        FrameStyle::Wood => {
                            let warp = fbm(along / (m * 3.0), cross / (m * 0.45)) * 3.0;
                            let rings = 0.5 + 0.5 * (cross / (m * 0.11) + warp * 2.0).sin();
                            let fine = noise(along / (m * 0.8), cross / (m * 0.03), 11);
                            l * (0.78 + 0.34 * rings + 0.12 * fine) * profile(prof, 1.0)
                        }
                        FrameStyle::Gold => {
                            let ridges = 0.62 + 0.38 * (prof * std::f32::consts::PI * 3.0).sin();
                            let shine = 0.5 + 0.5 * (along / (m * 1.6) + prof * 3.0).sin();
                            l * (0.55 + 0.5 * ridges * (0.7 + 0.5 * shine)) * profile(prof, 0.6)
                        }
                        _ => {
                            let brush = noise(along / (m * 6.0), cross / (m * 0.025), 5);
                            l * (0.9 + 0.2 * brush) * profile(prof, 1.1)
                        }
                    };
                    let mut c = hsl(h, sat, lightness.clamp(0.0, 1.0));
                    // The mitre joint.
                    let second = [dl, dt, dr, db]
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != side)
                        .map(|(_, v)| *v)
                        .fold(f32::MIN, f32::max);
                    if (depth - second).abs() < 1.0 && second > 0.0 {
                        c = scale(c, 0.55);
                    }
                    s.frame = c;
                }
                let depth = (k_shadow * m * 0.5).max(1.0);
                if d < 0.0 {
                    s.over = [0.0, 0.0, 0.0, 0.5 * k_shadow * (1.0 + d / depth).clamp(0.0, 1.0)];
                }
            }
            FrameStyle::Neon => {
                let (h, ..) = rgb_to_hsl(base);
                let bg = hsl(h, 0.5, 0.025);
                let neon = hsl(h, 1.0, 0.6);
                let r = k_corner * 0.12 * self.pw.min(self.ph);
                let rd = sd_rbox(fx - cx, fy - cy, hx, hy, r);
                s.cover = smooth(rd);
                let d0 = m * 0.9;
                let w = (k_line * m * 0.06).max(1.0);
                let dist = (rd - d0).abs();
                let glow = (-dist / (m * (0.08 + 0.3 * k_shadow))).exp();
                let mut c = [
                    bg[0] + neon[0] * glow * 0.85,
                    bg[1] + neon[1] * glow * 0.85,
                    bg[2] + neon[2] * glow * 0.85,
                ];
                c = c.map(|v| v.min(1.0));
                s.frame = c;
                let core = 1.0 - smooth(dist - w);
                s.over = with_alpha(mix(neon, [1.0; 3], 0.75), core.clamp(0.0, 1.0));
                if rd < 0.0 {
                    s.over = [0.0; 4];
                }
            }
            FrameStyle::Offset => {
                let t = (k_line * m * 0.25).max(2.0);
                let so = m * (0.3 + 0.5 * k_shadow);
                let shadow = sd_box(fx - cx - so, fy - cy - so, hx + t, hy + t);
                s.frame = mix(base, [0.04; 3], smooth(shadow));
                s.over = with_alpha([0.04; 3], band(d, 0.0, t) * 0.97);
            }
            FrameStyle::Oval => {
                let dx = fx - cx;
                let dy = fy - cy;
                let k = ((dx / hx).powi(2) + (dy / hy).powi(2)).sqrt().max(1e-6);
                let grad = ((dx / (hx * hx)).powi(2) + (dy / (hy * hy)).powi(2)).sqrt() / k;
                let sd = (k - 1.0) / grad.max(1e-6);
                s.cover = smooth(sd);
                s.frame = paper(base, 0.03);
                let t = (k_line * m * 0.1).max(1.0);
                let ring = band(sd, m * 0.22, m * 0.22 + t).max(band(sd, m * 0.5, m * 0.5 + t * 0.6));
                s.over = with_alpha(scale(base, 0.4), ring * 0.8);
                let outer = band(edge, m * 0.35, m * 0.35 + t * 0.7);
                s.over = blend_over(s.over, with_alpha(scale(base, 0.4), outer * 0.7));
                let depth = (k_shadow * m * 0.4).max(1.0);
                if sd < 0.0 {
                    s.over = blend_over(s.over, [0.0, 0.0, 0.0, 0.45 * k_shadow * (1.0 + sd / depth).clamp(0.0, 1.0)]);
                }
            }
        }
        s
    }

    /// Blur fill: the photo, enlarged to cover the whole canvas, blurred
    /// (by being only 32×32 samples), darkened and made richer.
    fn blurred_backdrop(&self, fx: f32, fy: f32) -> Colour {
        let scale_to_cover = (self.ow / self.pw).max(self.oh / self.ph);
        let u = ((fx - self.ow / 2.0) / scale_to_cover + self.pw / 2.0) / self.pw;
        let v = ((fy - self.oh / 2.0) / scale_to_cover + self.ph / 2.0) / self.ph;
        let gx = (u * LOWRES as f32 - 0.5).clamp(0.0, LOWRES as f32 - 1.001);
        let gy = (v * LOWRES as f32 - 0.5).clamp(0.0, LOWRES as f32 - 1.001);
        let (ix, iy) = (gx as usize, gy as usize);
        let (tx, ty) = (gx - ix as f32, gy - iy as f32);
        let at = |x: usize, y: usize| self.lowres[y * LOWRES + x];
        let top = mix(at(ix, iy), at(ix + 1, iy), tx);
        let bottom = mix(at(ix, iy + 1), at(ix + 1, iy + 1), tx);
        let c = mix(top, bottom, ty);
        let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let rich = c.map(|v| (l + (v - l) * 1.35).clamp(0.0, 1.0));
        scale(rich, 0.55)
    }
}

// --- Public entry points ---------------------------------------------------

/// `photo` with the frame drawn around it.
pub fn frame_rgb8(photo: &RgbImage, frame: &FrameSettings) -> RgbImage {
    let (w, h, raw) = render(
        photo.width(),
        photo.height(),
        frame,
        &|x, y| {
            let p = photo.get_pixel(x, y).0;
            [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
        },
        3,
        &|c, out: &mut [u8]| {
            for i in 0..3 {
                out[i] = (c[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        },
    );
    RgbImage::from_raw(w, h, raw).expect("sized to fit")
}

/// [`frame_rgb8`] for 16-bit pictures.
pub fn frame_rgb16(
    photo: &ImageBuffer<Rgb<u16>, Vec<u16>>,
    frame: &FrameSettings,
) -> ImageBuffer<Rgb<u16>, Vec<u16>> {
    let (w, h, raw) = render(
        photo.width(),
        photo.height(),
        frame,
        &|x, y| {
            let p = photo.get_pixel(x, y).0;
            [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]
        },
        3,
        &|c, out: &mut [u16]| {
            for i in 0..3 {
                out[i] = (c[i].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            }
        },
    );
    ImageBuffer::from_raw(w, h, raw).expect("sized to fit")
}

/// A preview frame (BGRA in an `RgbaImage`) with the frame drawn around it.
pub fn frame_bgra(photo: &RgbaImage, frame: &FrameSettings) -> RgbaImage {
    let (w, h, raw) = render(
        photo.width(),
        photo.height(),
        frame,
        &|x, y| {
            let p = photo.get_pixel(x, y).0;
            // Stored blue first.
            [p[2] as f32 / 255.0, p[1] as f32 / 255.0, p[0] as f32 / 255.0]
        },
        4,
        &|c, out: &mut [u8]| {
            out[0] = (c[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[1] = (c[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[2] = (c[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[3] = 255;
        },
    );
    RgbaImage::from_raw(w, h, raw).expect("sized to fit")
}

/// The framed picture's size for a `width` × `height` photo.
pub fn framed_size(width: u32, height: u32, frame: &FrameSettings) -> (u32, u32) {
    let [l, t, r, b] = frame.margins(width, height);
    (width + l + r, height + t + b)
}

fn render<T: Copy + Default + Send>(
    width: u32,
    height: u32,
    frame: &FrameSettings,
    fetch: &(dyn Fn(u32, u32) -> Colour + Sync),
    channels: usize,
    write: &(dyn Fn(Colour, &mut [T]) + Sync),
) -> (u32, u32, Vec<T>) {
    let framer = Framer::new(width, height, frame, fetch);
    let (ow, oh) = framer.out_size();
    let mut out = vec![T::default(); ow as usize * oh as usize * channels];
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, oh as usize);
    let rows_per_band = (oh as usize).div_ceil(threads);
    let row_len = ow as usize * channels;
    let framer = &framer;
    std::thread::scope(|scope| {
        for (band, chunk) in out.chunks_mut(rows_per_band * row_len).enumerate() {
            scope.spawn(move || {
                for (i, row) in chunk.chunks_mut(row_len).enumerate() {
                    let y = (band * rows_per_band + i) as u32;
                    for (x, px) in row.chunks_exact_mut(channels).enumerate() {
                        write(framer.pixel(x as u32, y, fetch), px);
                    }
                }
            });
        }
    });
    (ow, oh, out)
}

// --- Drawing helpers ---------------------------------------------------------

fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(c: Colour, k: f32) -> Colour {
    c.map(|v| (v * k).clamp(0.0, 1.0))
}

fn lighten(c: Colour, by: f32) -> Colour {
    c.map(|v| (v + by).clamp(0.0, 1.0))
}

fn with_alpha(c: Colour, a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a.clamp(0.0, 1.0)]
}

/// `top` laid over `under`, both with opacity.
fn blend_over(under: [f32; 4], top: [f32; 4]) -> [f32; 4] {
    let a = top[3] + under[3] * (1.0 - top[3]);
    if a <= 0.0 {
        return [0.0; 4];
    }
    let c = |i: usize| (top[i] * top[3] + under[i] * under[3] * (1.0 - top[3])) / a;
    [c(0), c(1), c(2), a]
}

/// Antialiased coverage for a signed distance (negative inside).
fn smooth(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// Coverage of the stretch `lo..hi` of a distance.
fn band(d: f32, lo: f32, hi: f32) -> f32 {
    ((d - lo + 0.5).clamp(0.0, 1.0)) * ((hi - d + 0.5).clamp(0.0, 1.0))
}

/// The light and dark of a moulding's profile across its width.
fn profile(prof: f32, strength: f32) -> f32 {
    let inner_lip = (1.0 - prof / 0.12).clamp(0.0, 1.0) * 0.28;
    let outer_edge = ((prof - 0.86) / 0.14).clamp(0.0, 1.0) * 0.4;
    let dome = 1.0 - (prof - 0.5).abs() * 0.3;
    (1.0 + (inner_lip - outer_edge + (dome - 0.9)) * strength).max(0.2)
}

fn drop_shadow(
    frame: Colour,
    fx: f32,
    fy: f32,
    cx: f32,
    cy: f32,
    hx: f32,
    hy: f32,
    r: f32,
    m: f32,
    strength: f32,
    darkest: f32,
) -> Colour {
    let ds = sd_rbox(fx - cx, fy - cy, hx, hy, r);
    let sigma = m * (0.3 + 1.2 * strength);
    let t = ((ds + sigma) / (2.0 * sigma)).clamp(0.0, 1.0);
    let falloff = 1.0 - t * t * (3.0 - 2.0 * t);
    mix(frame, [0.0; 3], darkest * strength * falloff)
}

/// Signed distance to a box of half-size (`hx`, `hy`) at the origin.
fn sd_box(x: f32, y: f32, hx: f32, hy: f32) -> f32 {
    let (qx, qy) = (x.abs() - hx, y.abs() - hy);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)
}

/// [`sd_box`] with corners rounded to `r`.
fn sd_rbox(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let r = r.min(hx).min(hy).max(0.0);
    sd_box(x, y, hx - r, hy - r) - r
}

fn hash2(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    h as f32 / u32::MAX as f32
}

/// Smooth value noise, 0..=1.
fn noise(x: f32, y: f32, seed: i32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (tx, ty) = (x - ix, y - iy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let at = |dx: i32, dy: i32| hash2(ix as i32 + dx + seed * 131, iy as i32 + dy);
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Three octaves of [`noise`], 0..=1.
fn fbm(x: f32, y: f32) -> f32 {
    (noise(x, y, 1) * 0.55 + noise(x * 2.1, y * 2.1, 2) * 0.3 + noise(x * 4.3, y * 4.3, 3) * 0.15).clamp(0.0, 1.0)
}

fn hsl(h: f32, s: f32, l: f32) -> Colour {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

fn rgb_to_hsl([r, g, b]: Colour) -> (f32, f32, f32) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb([128, 128, 128]))
    }

    #[test]
    fn every_style_grows_the_picture_and_keeps_the_photo() {
        for style in FrameStyle::ALL.into_iter().filter(|s| *s != FrameStyle::None) {
            let frame = style.defaults();
            let photo = RgbImage::from_fn(240, 160, |x, y| image::Rgb([(x % 251) as u8, (y % 241) as u8, 90]));
            let out = frame_rgb8(&photo, &frame);
            let [l, t, r, b] = frame.margins(240, 160);
            assert_eq!(out.dimensions(), (240 + l + r, 160 + t + b), "{style:?}");
            assert_eq!(framed_size(240, 160, &frame), out.dimensions());
            // The middle of the photo is the photo (oval windows keep the centre too).
            let (cx, cy) = (l + 120, t + 80);
            assert_eq!(out.get_pixel(cx, cy), photo.get_pixel(120, 80), "{style:?}");
        }
    }

    #[test]
    fn no_frame_is_a_no_op_size() {
        let photo = grey(50, 40);
        assert_eq!(framed_size(50, 40, &FrameSettings::default()), (50, 40));
        let out = frame_rgb8(&photo, &FrameSettings::default());
        assert_eq!(out.dimensions(), (50, 40));
    }

    #[test]
    fn a_white_border_is_white() {
        let frame = FrameStyle::Border.defaults();
        let out = frame_rgb8(&grey(200, 120), &frame);
        assert!(out.get_pixel(1, 1).0.iter().all(|&v| v >= 250));
    }

    #[test]
    fn bgra_and_16_bit_frames_match_the_8_bit_one() {
        let frame = FrameStyle::Matte.defaults();
        let rgb = RgbImage::from_pixel(100, 80, image::Rgb([200, 100, 50]));
        let framed = frame_rgb8(&rgb, &frame);
        let bgra = RgbaImage::from_pixel(100, 80, image::Rgba([50, 100, 200, 255]));
        let framed_bgra = frame_bgra(&bgra, &frame);
        assert_eq!(framed_bgra.dimensions(), framed.dimensions());
        let (x, y) = (framed.width() / 2, framed.height() / 2);
        let p = framed.get_pixel(x, y).0;
        let q = framed_bgra.get_pixel(x, y).0;
        assert_eq!([p[2], p[1], p[0]], [q[0], q[1], q[2]]);
        let deep = ImageBuffer::<Rgb<u16>, Vec<u16>>::from_pixel(100, 80, Rgb([200 * 257, 100 * 257, 50 * 257]));
        let framed16 = frame_rgb16(&deep, &frame);
        assert_eq!(framed16.dimensions(), framed.dimensions());
        let d = framed16.get_pixel(x, y).0;
        assert!((d[0] / 257).abs_diff(p[0] as u16) <= 1);
    }

    /// Writes a contact sheet of every style on a photo: `FRAME_SHEET=photo.jpg`.
    #[test]
    #[ignore = "writes /tmp/frames.png for looking at"]
    fn contact_sheet() {
        let Ok(path) = std::env::var("FRAME_SHEET") else { return };
        let photo = image::open(path).unwrap().to_rgb8();
        let photo = image::imageops::resize(&photo, 300, 300 * photo.height() / photo.width(), image::imageops::FilterType::Triangle);
        let styles: Vec<_> = FrameStyle::ALL.into_iter().filter(|s| *s != FrameStyle::None).collect();
        let cell = 420u32;
        let cols = 5u32;
        let rows = (styles.len() as u32).div_ceil(cols);
        let mut sheet = RgbImage::from_pixel(cols * cell, rows * cell, image::Rgb([60, 60, 64]));
        for (i, style) in styles.iter().enumerate() {
            let framed = frame_rgb8(&photo, &style.defaults());
            let framed = image::imageops::resize(&framed, framed.width().min(cell - 20), framed.height() * framed.width().min(cell - 20) / framed.width(), image::imageops::FilterType::Triangle);
            let (cx, cy) = ((i as u32 % cols) * cell + 10, (i as u32 / cols) * cell + 10);
            image::imageops::overlay(&mut sheet, &framed, cx as i64, cy as i64);
        }
        sheet.save("/tmp/frames.png").unwrap();
    }

    #[test]
    fn portraits_and_tiny_photos_do_not_panic() {
        for style in FrameStyle::ALL.into_iter().filter(|s| *s != FrameStyle::None) {
            for (w, h) in [(1, 1), (7, 300), (300, 7), (64, 96)] {
                let out = frame_rgb8(&grey(w, h), &style.defaults());
                assert!(out.width() >= w && out.height() >= h, "{style:?} {w}x{h}");
            }
        }
    }
}
