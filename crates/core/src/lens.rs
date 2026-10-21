//! Lens correction profiles. A photo's lens name and focal length (read
//! from its EXIF) pick a profile: how much to straighten barrel or
//! pincushion distortion, brighten the corners and remove colour fringes.
//!
//! The values are approximations tuned by eye for well-known lenses, with a
//! generic estimate from the focal length when the lens isn't listed. They
//! land in the Lens Corrections sliders, which can then be refined by hand.

/// Correction amounts in the same -100..=100 units as the sliders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensCorrection {
    pub distortion: f32,
    pub vignetting: f32,
    pub chromatic_aberration: f32,
}

/// A profile found for a lens.
#[derive(Debug, Clone, PartialEq)]
pub struct LensProfile {
    /// What matched: the listed lens, or "Generic estimate for 24 mm".
    pub name: String,
    pub correction: LensCorrection,
    /// Whether the lens itself was recognised.
    pub exact: bool,
}

struct Entry {
    /// Lower-case fragments that must all appear in the lens name.
    keys: &'static [&'static str],
    label: &'static str,
    distortion: f32,
    vignetting: f32,
    ca: f32,
}

const fn e(
    keys: &'static [&'static str],
    label: &'static str,
    distortion: f32,
    vignetting: f32,
    ca: f32,
) -> Entry {
    Entry {
        keys,
        label,
        distortion,
        vignetting,
        ca,
    }
}

/// More specific entries first.
static PROFILES: &[Entry] = &[
    // Fujifilm X
    e(&["xf14mm"], "Fujifilm XF 14mm f/2.8", 34.0, 30.0, 8.0),
    e(&["xf16mm"], "Fujifilm XF 16mm f/1.4", 18.0, 32.0, 10.0),
    e(&["xf18mm"], "Fujifilm XF 18mm f/2", 10.0, 24.0, 8.0),
    e(&["xf23mmf1.4"], "Fujifilm XF 23mm f/1.4", 8.0, 30.0, 8.0),
    e(&["xf23mmf2"], "Fujifilm XF 23mm f/2", 7.0, 22.0, 6.0),
    e(&["xf27mm"], "Fujifilm XF 27mm f/2.8", 4.0, 16.0, 4.0),
    e(&["xf33mm"], "Fujifilm XF 33mm f/1.4", 3.0, 24.0, 6.0),
    e(&["xf35mmf1.4"], "Fujifilm XF 35mm f/1.4", 3.0, 28.0, 6.0),
    e(&["xf35mmf2"], "Fujifilm XF 35mm f/2", 3.0, 20.0, 5.0),
    e(&["xf50mm"], "Fujifilm XF 50mm", 1.0, 20.0, 4.0),
    e(&["xf56mm"], "Fujifilm XF 56mm f/1.2", 1.0, 26.0, 6.0),
    e(&["xf10-24"], "Fujifilm XF 10-24mm", 36.0, 32.0, 12.0),
    e(&["xf16-55"], "Fujifilm XF 16-55mm f/2.8", 18.0, 26.0, 8.0),
    e(&["xf18-55"], "Fujifilm XF 18-55mm", 22.0, 30.0, 9.0),
    e(&["xf50-140"], "Fujifilm XF 50-140mm", 3.0, 18.0, 5.0),
    e(&["xc15-45"], "Fujifilm XC 15-45mm", 24.0, 30.0, 10.0),
    // Sony E / FE
    e(&["24-70mm", "gm"], "Sony FE 24-70mm GM", 14.0, 30.0, 6.0),
    e(&["24-105mm"], "24-105mm", 14.0, 28.0, 6.0),
    e(&["16-35mm"], "16-35mm", 28.0, 34.0, 8.0),
    e(&["70-200mm"], "70-200mm", 3.0, 22.0, 4.0),
    e(&["85mm", "1.4"], "85mm f/1.4", 2.0, 30.0, 5.0),
    e(&["50mm", "1.4"], "50mm f/1.4", 4.0, 34.0, 6.0),
    e(&["50mm", "1.8"], "50mm f/1.8", 5.0, 30.0, 6.0),
    e(&["35mm", "1.4"], "35mm f/1.4", 8.0, 34.0, 6.0),
    e(&["35mm", "1.8"], "35mm f/1.8", 8.0, 28.0, 6.0),
    e(&["24mm", "1.4"], "24mm f/1.4", 14.0, 36.0, 8.0),
    // Canon / Nikon / Sigma / Tamron zooms
    e(&["24-70"], "24-70mm", 15.0, 30.0, 6.0),
    e(&["28-75"], "28-75mm", 10.0, 28.0, 6.0),
    e(&["70-300"], "70-300mm", 3.0, 22.0, 6.0),
    e(&["100-400"], "100-400mm", 2.0, 20.0, 5.0),
];

/// The profile for `lens` (as named in the EXIF) at `focal_mm`, or a
/// generic estimate for that focal length when the lens isn't listed.
/// `None` when neither the lens nor the focal length is known.
pub fn lens_profile(lens: &str, focal_mm: f32) -> Option<LensProfile> {
    let normalised: String = lens
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if let Some(entry) = PROFILES
        .iter()
        .find(|p| p.keys.iter().all(|k| normalised.contains(*k)))
    {
        // Zooms are worst at the wide end; ease off towards the long end.
        let wide = if is_zoom(&normalised) && focal_mm > 0.0 {
            zoom_factor(&normalised, focal_mm)
        } else {
            1.0
        };
        return Some(LensProfile {
            name: entry.label.to_string(),
            correction: LensCorrection {
                distortion: entry.distortion * wide,
                vignetting: entry.vignetting,
                chromatic_aberration: entry.ca,
            },
            exact: true,
        });
    }
    if focal_mm <= 0.0 {
        return None;
    }
    // Generic: wide lenses bulge and darken their corners more.
    let wide = ((50.0 - focal_mm) / 35.0).clamp(-0.3, 1.0);
    Some(LensProfile {
        name: format!("Generic estimate for {focal_mm:.0} mm"),
        correction: LensCorrection {
            distortion: (wide * 28.0).max(0.0),
            vignetting: 12.0 + wide.max(0.0) * 14.0,
            chromatic_aberration: 4.0 + wide.max(0.0) * 4.0,
        },
        exact: false,
    })
}

fn is_zoom(normalised: &str) -> bool {
    // "24-70", "16-55": two numbers joined by a dash.
    normalised
        .as_bytes()
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && w[1] == b'-' && w[2].is_ascii_digit())
}

/// 1.0 at the wide end of a zoom, falling towards 0.1 at its long end.
fn zoom_factor(normalised: &str, focal_mm: f32) -> f32 {
    // The first "a-b" range in the name.
    let range = normalised.find('-').and_then(|dash| {
        let before: String = normalised[..dash]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let after: String = normalised[dash + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        Some((before.parse::<f32>().ok()?, after.parse::<f32>().ok()?))
    });
    match range {
        Some((lo, hi)) if hi > lo => {
            let t = ((focal_mm - lo) / (hi - lo)).clamp(0.0, 1.0);
            1.0 - 0.9 * t
        }
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_lenses_are_matched_whatever_the_spacing() {
        let p = lens_profile("XF23mmF2 R WR", 23.0).unwrap();
        assert!(p.exact);
        assert_eq!(p.name, "Fujifilm XF 23mm f/2");
        let q = lens_profile("XF 23mm F1.4 R", 23.0).unwrap();
        assert_eq!(q.name, "Fujifilm XF 23mm f/1.4");
    }

    #[test]
    fn zooms_are_corrected_most_at_the_wide_end() {
        let wide = lens_profile("XF18-55mmF2.8-4 R LM OIS", 18.0).unwrap();
        let long = lens_profile("XF18-55mmF2.8-4 R LM OIS", 55.0).unwrap();
        assert!(wide.correction.distortion > long.correction.distortion * 3.0);
    }

    #[test]
    fn unknown_lenses_get_a_generic_estimate_from_the_focal_length() {
        let wide = lens_profile("Some Lens", 16.0).unwrap();
        let tele = lens_profile("Some Lens", 135.0).unwrap();
        assert!(!wide.exact);
        assert!(wide.correction.distortion > tele.correction.distortion);
        assert!(wide.name.contains("16 mm"));
        assert!(lens_profile("", 0.0).is_none());
    }
}
