//! Lens correction data from the Lensfun database: measured distortion,
//! lateral colour-fringe and vignetting models for thousands of lenses.
//!
//! The database is a folder of XML files (installed with Lensfun, or
//! downloaded from <https://lensfun.github.io>). A lens is found by the
//! name in the photo's EXIF, its models are evaluated at the photo's focal
//! length (interpolating between the calibrated ones), and the result is
//! expressed in the Lens Corrections sliders' units.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use rustyedit_core::{LensCorrection, LensProfile};

/// One lens' calibration data.
#[derive(Debug, Clone, Default)]
struct DbLens {
    names: Vec<String>,
    distortion: Vec<Distortion>,
    tca: Vec<Tca>,
    vignetting: Vec<Vignetting>,
}

#[derive(Debug, Clone, Copy)]
struct Distortion {
    focal: f32,
    model: DistortionModel,
}

#[derive(Debug, Clone, Copy)]
enum DistortionModel {
    /// `Rs = Rd (a Rd^3 + b Rd^2 + c Rd + 1 - a - b - c)`
    PtLens { a: f32, b: f32, c: f32 },
    /// `Rs = Rd (1 - k1 + k1 Rd^2)`
    Poly3 { k1: f32 },
    /// `Rs = Rd (1 + k1 Rd^2 + k2 Rd^4)`
    Poly5 { k1: f32, k2: f32 },
}

impl DistortionModel {
    /// Source radius per unit corrected radius at `r`.
    fn scale_at(&self, r: f32) -> f32 {
        match *self {
            DistortionModel::PtLens { a, b, c } => a * r.powi(3) + b * r * r + c * r + (1.0 - a - b - c),
            DistortionModel::Poly3 { k1 } => 1.0 - k1 + k1 * r * r,
            DistortionModel::Poly5 { k1, k2 } => 1.0 + k1 * r * r + k2 * r.powi(4),
        }
    }

    fn lerp(&self, other: &Self, t: f32) -> Self {
        use DistortionModel::*;
        let mix = |a: f32, b: f32| a + (b - a) * t;
        match (*self, *other) {
            (PtLens { a, b, c }, PtLens { a: a2, b: b2, c: c2 }) => PtLens {
                a: mix(a, a2),
                b: mix(b, b2),
                c: mix(c, c2),
            },
            (Poly3 { k1 }, Poly3 { k1: k2 }) => Poly3 { k1: mix(k1, k2) },
            (Poly5 { k1, k2 }, Poly5 { k1: j1, k2: j2 }) => Poly5 {
                k1: mix(k1, j1),
                k2: mix(k2, j2),
            },
            // Different models at neighbouring focals: use the nearer.
            (a, b) => {
                if t < 0.5 {
                    a
                } else {
                    b
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Tca {
    focal: f32,
    /// Red and blue scale relative to green.
    vr: f32,
    vb: f32,
}

#[derive(Debug, Clone, Copy)]
struct Vignetting {
    focal: f32,
    aperture: f32,
    k: [f32; 3],
}

/// The two entries around `focal` and how far between them it is (0 = the
/// first), or the single nearest one.
fn bracket<T: Copy>(items: &[T], focal_of: impl Fn(&T) -> f32, focal: f32) -> Option<(T, T, f32)> {
    let mut sorted: Vec<T> = items.to_vec();
    sorted.sort_by(|a, b| focal_of(a).total_cmp(&focal_of(b)));
    let first = *sorted.first()?;
    let last = *sorted.last()?;
    if focal <= focal_of(&first) {
        return Some((first, first, 0.0));
    }
    if focal >= focal_of(&last) {
        return Some((last, last, 0.0));
    }
    let upper = sorted.iter().position(|i| focal_of(i) >= focal)?;
    let (lo, hi) = (sorted[upper - 1], sorted[upper]);
    let span = focal_of(&hi) - focal_of(&lo);
    Some((lo, hi, if span > 0.0 { (focal - focal_of(&lo)) / span } else { 0.0 }))
}

impl DbLens {
    /// The correction at `focal` mm for an image of `aspect` (long / short).
    fn correction(&self, focal: f32, aspect: f32) -> LensCorrection {
        // Lensfun measures radius in units of half the shorter side, so the
        // corner of the frame is at this radius.
        let corner = (1.0 + aspect * aspect).sqrt();

        let distortion = bracket(&self.distortion, |d| d.focal, focal)
            .map(|(lo, hi, t)| {
                let model = lo.model.lerp(&hi.model, t);
                // Our model pulls a corner's source inwards by `k` of its
                // radius: scale = 1 - k at the corner.
                let k = 1.0 - model.scale_at(corner);
                (k / 0.25 * 100.0).clamp(-100.0, 100.0)
            })
            .unwrap_or(0.0);

        let chromatic = bracket(&self.tca, |t| t.focal, focal)
            .map(|(lo, hi, t)| {
                let vr = lo.vr + (hi.vr - lo.vr) * t;
                let vb = lo.vb + (hi.vb - lo.vb) * t;
                (((vr - vb) / 2.0) / 0.01 * 100.0).clamp(-100.0, 100.0)
            })
            .unwrap_or(0.0);

        // The vignetting entries nearest the focal length; of those, the
        // middle aperture.
        let vignetting = {
            let nearest = self
                .vignetting
                .iter()
                .map(|v| (v.focal - focal).abs())
                .fold(f32::INFINITY, f32::min);
            let mut group: Vec<&Vignetting> = self
                .vignetting
                .iter()
                .filter(|v| (v.focal - focal).abs() <= nearest + 0.01)
                .collect();
            group.sort_by(|a, b| a.aperture.total_cmp(&b.aperture));
            group
                .get(group.len() / 2)
                .map(|v| {
                    // Lensfun: falloff C(R) = 1 + k1 R^2 + k2 R^4 + k3 R^6 with R = 1 at
                    // the corner; brightening by 1/C undoes it.
                    let c = 1.0 + v.k[0] + v.k[1] + v.k[2];
                    let gain = if c > 0.05 { 1.0 / c } else { 1.0 };
                    ((gain - 1.0) / 0.9 * 100.0).clamp(-100.0, 100.0)
                })
                .unwrap_or(0.0)
        };

        LensCorrection {
            distortion,
            vignetting,
            chromatic_aberration: chromatic,
        }
    }
}

/// A parsed database.
#[derive(Debug, Default)]
pub struct LensDb {
    lenses: Vec<DbLens>,
}

fn normalise(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

impl LensDb {
    /// Reads every `.xml` file under `dirs` (two levels deep).
    pub fn load(dirs: &[PathBuf]) -> Self {
        let mut db = Self::default();
        for dir in dirs {
            let mut files = Vec::new();
            collect_xml(dir, 2, &mut files);
            for file in files {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    db.add_xml(&text);
                }
            }
        }
        db
    }

    /// Parses one Lensfun XML document into the database.
    pub fn add_xml(&mut self, xml: &str) {
        let Ok(doc) = roxmltree::Document::parse_with_options(
            xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        ) else {
            return;
        };
        for lens in doc.descendants().filter(|n| n.has_tag_name("lens")) {
            let mut entry = DbLens::default();
            for child in lens.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "model" => {
                        if let Some(text) = child.text() {
                            entry.names.push(text.trim().to_string());
                        }
                    }
                    "calibration" => {
                        for c in child.children().filter(|n| n.is_element()) {
                            entry.add_calibration(&c);
                        }
                    }
                    _ => {}
                }
            }
            if !entry.names.is_empty() {
                self.lenses.push(entry);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.lenses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lenses.is_empty()
    }

    /// The profile for `lens` (the name in the photo's EXIF) at `focal` mm.
    pub fn find(&self, lens: &str, focal: f32, aspect: f32) -> Option<LensProfile> {
        let wanted = normalise(lens);
        if wanted.len() < 4 {
            return None;
        }
        let exact = self
            .lenses
            .iter()
            .find(|l| l.names.iter().any(|n| normalise(n) == wanted));
        // Failing that, a name that contains the other (EXIF names often
        // drop the maker or add a suffix).
        let found = exact.or_else(|| {
            self.lenses.iter().find(|l| {
                l.names.iter().any(|n| {
                    let n = normalise(n);
                    n.len() >= 6 && (n.contains(&wanted) || wanted.contains(&n))
                })
            })
        })?;
        if found.distortion.is_empty() && found.vignetting.is_empty() && found.tca.is_empty() {
            return None;
        }
        Some(LensProfile {
            name: format!("{} (Lensfun)", found.names[0]),
            correction: found.correction(focal, aspect.max(1.0)),
            exact: true,
        })
    }
}

impl DbLens {
    fn add_calibration(&mut self, node: &roxmltree::Node) {
        let num = |name: &str| node.attribute(name).and_then(|v| v.parse::<f32>().ok());
        let focal = num("focal").unwrap_or(0.0);
        match node.tag_name().name() {
            "distortion" => {
                let model = match node.attribute("model") {
                    Some("ptlens") => DistortionModel::PtLens {
                        a: num("a").unwrap_or(0.0),
                        b: num("b").unwrap_or(0.0),
                        c: num("c").unwrap_or(0.0),
                    },
                    Some("poly3") => DistortionModel::Poly3 {
                        k1: num("k1").unwrap_or(0.0),
                    },
                    Some("poly5") => DistortionModel::Poly5 {
                        k1: num("k1").unwrap_or(0.0),
                        k2: num("k2").unwrap_or(0.0),
                    },
                    _ => return,
                };
                self.distortion.push(Distortion { focal, model });
            }
            "tca" => {
                let (vr, vb) = match node.attribute("model") {
                    Some("linear") => (num("kr"), num("kb")),
                    Some("poly3") => (num("vr"), num("vb")),
                    _ => return,
                };
                self.tca.push(Tca {
                    focal,
                    vr: vr.unwrap_or(1.0),
                    vb: vb.unwrap_or(1.0),
                });
            }
            "vignetting" if node.attribute("model") == Some("pa") => {
                self.vignetting.push(Vignetting {
                    focal,
                    aperture: num("aperture").unwrap_or(0.0),
                    k: [
                        num("k1").unwrap_or(0.0),
                        num("k2").unwrap_or(0.0),
                        num("k3").unwrap_or(0.0),
                    ],
                });
            }
            _ => {}
        }
    }
}

fn collect_xml(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_xml(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) {
            out.push(path);
        }
    }
}

/// Where Lensfun databases are usually installed, plus `extra` (a folder
/// the user chose) first.
pub fn lensfun_dirs(extra: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = extra.into_iter().map(Path::to_path_buf).collect();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/lensfun"));
        dirs.push(home.join("Library/Application Support/lensfun"));
    }
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("lensfun"));
    }
    for fixed in [
        "/usr/share/lensfun",
        "/usr/local/share/lensfun",
        "/opt/homebrew/share/lensfun",
        "/opt/local/share/lensfun",
    ] {
        dirs.push(PathBuf::from(fixed));
    }
    dirs.retain(|d| d.is_dir());
    dirs.dedup();
    dirs
}

/// The database for `dirs`, loaded once and reused.
fn database(dirs: Vec<PathBuf>) -> Arc<LensDb> {
    static CACHE: OnceLock<Mutex<Option<(Vec<PathBuf>, Arc<LensDb>)>>> = OnceLock::new();
    let cell = CACHE.get_or_init(Default::default);
    let mut cached = cell.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_dirs, db)) = cached.as_ref() {
        if *cached_dirs == dirs {
            return db.clone();
        }
    }
    let db = Arc::new(LensDb::load(&dirs));
    *cached = Some((dirs, db.clone()));
    db
}

/// Looks `lens` up in the installed Lensfun database (and `extra`, a folder
/// the user chose). `None` when there is no database or no such lens.
pub fn find_lens_profile(extra: Option<&Path>, lens: &str, focal: f32, aspect: f32) -> Option<LensProfile> {
    let dirs = lensfun_dirs(extra);
    if dirs.is_empty() {
        return None;
    }
    database(dirs).find(lens, focal, aspect)
}

/// How many lenses the databases in the usual places (and `extra`) hold;
/// 0 when none is installed.
pub fn lens_database_size(extra: Option<&Path>) -> usize {
    let dirs = lensfun_dirs(extra);
    if dirs.is_empty() {
        0
    } else {
        database(dirs).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<lensdatabase version="2">
  <lens>
    <maker>Fujifilm</maker>
    <model>Fujifilm XF 18-55mm f/2.8-4 R LM OIS</model>
    <model lang="en">XF18-55mmF2.8-4 R LM OIS</model>
    <mount>Fujifilm X</mount>
    <cropfactor>1.5</cropfactor>
    <calibration>
      <distortion model="ptlens" focal="18" a="0.0" b="-0.0360" c="0.0"/>
      <distortion model="ptlens" focal="55" a="0.0" b="0.0050" c="0.0"/>
      <tca model="poly3" focal="18" vr="1.0004" vb="0.9996"/>
      <tca model="poly3" focal="55" vr="1.0000" vb="1.0000"/>
      <vignetting model="pa" focal="18" aperture="2.8" distance="1000" k1="-0.55" k2="0.05" k3="0.0"/>
      <vignetting model="pa" focal="55" aperture="4" distance="1000" k1="-0.10" k2="0.0" k3="0.0"/>
    </calibration>
  </lens>
  <lens>
    <maker>Sigma</maker>
    <model>Sigma 35mm f/1.4 DG HSM | A</model>
    <calibration>
      <distortion model="poly3" focal="35" k1="0.0100"/>
    </calibration>
  </lens>
</lensdatabase>"#;

    fn db() -> LensDb {
        let mut db = LensDb::default();
        db.add_xml(XML);
        db
    }

    #[test]
    fn a_lens_is_found_by_its_exif_name_however_spaced() {
        let db = db();
        assert_eq!(db.len(), 2);
        let p = db.find("XF18-55mmF2.8-4 R LM OIS", 18.0, 1.5).unwrap();
        assert!(p.name.contains("XF 18-55mm"));
        assert!(db.find("Sigma 35mm f/1.4 DG HSM | A", 35.0, 1.5).is_some());
        assert!(db.find("Unknown lens", 35.0, 1.5).is_none());
        assert!(db.find("", 35.0, 1.5).is_none());
    }

    #[test]
    fn barrel_distortion_becomes_a_positive_correction_that_fades_with_focal_length() {
        let db = db();
        let wide = db.find("XF18-55mmF2.8-4 R LM OIS", 18.0, 1.5).unwrap().correction;
        let mid = db.find("XF18-55mmF2.8-4 R LM OIS", 36.0, 1.5).unwrap().correction;
        let long = db.find("XF18-55mmF2.8-4 R LM OIS", 55.0, 1.5).unwrap().correction;
        // b = -0.036 at 18 mm: the corner is sampled well inside, so the
        // correction pulls edges outwards (positive).
        assert!(wide.distortion > 15.0, "{wide:?}");
        assert!(mid.distortion < wide.distortion && mid.distortion > long.distortion);
        // 55 mm is mild pincushion.
        assert!(long.distortion < 0.0, "{long:?}");
    }

    #[test]
    fn vignetting_and_fringes_are_converted_too() {
        let db = db();
        let wide = db.find("XF18-55mmF2.8-4 R LM OIS", 18.0, 1.5).unwrap().correction;
        // C(1) = 1 - 0.55 + 0.05 = 0.5: the corner is half as bright.
        assert!(wide.vignetting > 50.0, "{wide:?}");
        // Red is 0.0004 bigger than blue's shrink: a fringe correction.
        assert!(wide.chromatic_aberration > 2.0, "{wide:?}");
        let long = db.find("XF18-55mmF2.8-4 R LM OIS", 55.0, 1.5).unwrap().correction;
        assert!(long.vignetting < wide.vignetting);
        assert!(long.chromatic_aberration.abs() < 0.5);
    }

    #[test]
    fn poly3_models_are_understood() {
        let db = db();
        let p = db.find("Sigma 35mm f/1.4 DG HSM | A", 35.0, 1.5).unwrap().correction;
        // k1 > 0 samples the corner outside: pincushion-ish, negative slider.
        assert!(p.distortion < 0.0, "{p:?}");
    }
}
