//! Fujifilm's maker note: the film simulation and the other in-camera
//! settings a Fujifilm records in each photo (JPEG, or the JPEG inside a
//! RAF), read into a [`FujiRecipe`].
//!
//! The maker note is a small TIFF-style table after the text "FUJIFILM",
//! in little-endian, with offsets counted from its start. Tag numbers and
//! values follow what ExifTool documents for Fujifilm cameras.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

use rustyedit_core::{
    DynamicRange, FilmSimulation, FujiRecipe, GrainEffect, MonoFilter, Strength, WhiteBalance,
};

/// Where a RAF keeps the offset and length of its embedded JPEG.
const RAF_JPEG_FIELDS: u64 = 84;

/// The settings the camera recorded, or `None` for a photo that isn't from
/// a Fujifilm (or whose maker note can't be read).
pub fn read_fuji_recipe(path: &Path) -> Option<FujiRecipe> {
    let note = maker_note(path)?;
    recipe_from_tags(&parse_tags(&note)?)
}

/// The maker note's bytes: from the photo's own EXIF, or, for a RAF, from
/// the JPEG it carries.
fn maker_note(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 16];
    file.read_exact(&mut magic).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    let bytes = if magic.starts_with(b"FUJIFILMCCD-RAW") {
        file.seek(SeekFrom::Start(RAF_JPEG_FIELDS)).ok()?;
        let mut fields = [0u8; 8];
        file.read_exact(&mut fields).ok()?;
        let offset = u32::from_be_bytes(fields[0..4].try_into().ok()?) as u64;
        let length = u32::from_be_bytes(fields[4..8].try_into().ok()?) as usize;
        if length == 0 || length > 64 * 1024 * 1024 {
            return None;
        }
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut jpeg = vec![0u8; length];
        file.read_exact(&mut jpeg).ok()?;
        jpeg
    } else {
        let mut all = Vec::new();
        // The EXIF sits near the start of a JPEG: no need for all of it.
        BufReader::new(file).take(2 * 1024 * 1024).read_to_end(&mut all).ok()?;
        all
    };
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    match &exif.get_field(exif::Tag::MakerNote, exif::In::PRIMARY)?.value {
        exif::Value::Undefined(note, _) => Some(note.clone()),
        _ => None,
    }
}

/// The maker note's numeric tags, each as its list of values.
fn parse_tags(note: &[u8]) -> Option<HashMap<u16, Vec<i64>>> {
    if !note.starts_with(b"FUJIFILM") {
        return None;
    }
    let u16_at = |at: usize| Some(u16::from_le_bytes(note.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(note.get(at..at + 4)?.try_into().ok()?));
    let ifd = u32_at(8)? as usize;
    let count = u16_at(ifd)? as usize;
    let mut tags = HashMap::new();
    for i in 0..count.min(400) {
        let entry = ifd + 2 + i * 12;
        let (tag, kind, n) = (u16_at(entry)?, u16_at(entry + 2)?, u32_at(entry + 4)? as usize);
        let size = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 => 4,
            _ => continue,
        };
        if n == 0 || n > 64 {
            continue;
        }
        let total = size * n;
        let at = if total <= 4 { entry + 8 } else { u32_at(entry + 8)? as usize };
        let Some(data) = note.get(at..at + total) else {
            continue;
        };
        let values: Vec<i64> = data
            .chunks_exact(size)
            .map(|c| match kind {
                3 => u16::from_le_bytes([c[0], c[1]]) as i64,
                8 => i16::from_le_bytes([c[0], c[1]]) as i64,
                4 => u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64,
                9 => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64,
                6 => c[0] as i8 as i64,
                _ => c[0] as i64,
            })
            .collect();
        tags.insert(tag, values);
    }
    Some(tags)
}

/// The recipe the tags describe.
fn recipe_from_tags(tags: &HashMap<u16, Vec<i64>>) -> Option<FujiRecipe> {
    let first = |tag: u16| tags.get(&tag).and_then(|v| v.first().copied());
    // Cameras shooting a black and white simulation write no Film Mode.
    let film_mode = first(0x1401);
    let saturation = first(0x1003).unwrap_or(0);

    // Black and white is recorded in the Saturation tag; any other
    // simulation in Film Mode.
    let (simulation, mono_filter) = match saturation {
        0x300 => (FilmSimulation::Monochrome, MonoFilter::Off),
        0x301 => (FilmSimulation::Monochrome, MonoFilter::Red),
        0x302 => (FilmSimulation::Monochrome, MonoFilter::Yellow),
        0x303 => (FilmSimulation::Monochrome, MonoFilter::Green),
        0x310 => (FilmSimulation::Sepia, MonoFilter::Off),
        0x500 => (FilmSimulation::Acros, MonoFilter::Off),
        0x501 => (FilmSimulation::Acros, MonoFilter::Red),
        0x502 => (FilmSimulation::Acros, MonoFilter::Yellow),
        0x503 => (FilmSimulation::Acros, MonoFilter::Green),
        _ => (film_simulation(film_mode?)?, MonoFilter::Off),
    };

    let strength = |tag: u16| match first(tag) {
        Some(64) => Strength::Strong,
        Some(32) => Strength::Weak,
        _ => Strength::Off,
    };
    let grain = match (first(0x1047), first(0x104c)) {
        (Some(32), Some(32)) => GrainEffect::WeakLarge,
        (Some(32), _) => GrainEffect::WeakSmall,
        (Some(64), Some(32)) => GrainEffect::StrongLarge,
        (Some(64), _) => GrainEffect::StrongSmall,
        _ => GrainEffect::Off,
    };
    let dynamic_range = match first(0x1403).or_else(|| first(0x1402)) {
        Some(100) | Some(0x100) => DynamicRange::Dr100,
        Some(200) | Some(0x200) => DynamicRange::Dr200,
        Some(400) | Some(0x400) => DynamicRange::Dr400,
        _ => DynamicRange::Auto,
    };
    // 0x1040 / 0x1041 hold tone in 1/16 steps, hard (+) as negative.
    let tone = |tag: u16| first(tag).map_or(0.0, |v| (-(v as f32) / 16.0).clamp(-2.0, 4.0));
    let (white_balance, kelvin) = match first(0x1002) {
        Some(0x0) => (WhiteBalance::Auto, 5500),
        Some(0x1) => (WhiteBalance::AutoWhitePriority, 5500),
        Some(0x2) => (WhiteBalance::AutoAmbience, 5500),
        Some(0x100) => (WhiteBalance::Daylight, 5500),
        Some(0x200) => (WhiteBalance::Shade, 7500),
        Some(0x300..=0x304) => (WhiteBalance::Fluorescent, 4200),
        Some(0x400) => (WhiteBalance::Incandescent, 2900),
        Some(0xff0) => (
            WhiteBalance::Kelvin,
            first(0x1005).map_or(5500, |k| k.clamp(2000, 10000) as u32),
        ),
        _ => (WhiteBalance::Auto, 5500),
    };
    // Red and blue fine tuning, stored in twentieths of a step.
    let (wb_red, wb_blue) = tags
        .get(&0x100a)
        .filter(|v| v.len() >= 2)
        .map_or((0, 0), |v| {
            ((v[0] / 20).clamp(-9, 9) as i32, (v[1] / 20).clamp(-9, 9) as i32)
        });

    Some(FujiRecipe {
        name: "From the camera".into(),
        film_simulation: simulation,
        mono_filter,
        dynamic_range,
        grain,
        color_chrome: strength(0x1048),
        color_chrome_blue: strength(0x104e),
        white_balance,
        kelvin,
        wb_red,
        wb_blue,
        highlight: tone(0x1041),
        shadow: tone(0x1040),
        iso: String::new(),
        exposure_compensation: String::new(),
        ..FujiRecipe::default()
    })
}

/// The film simulation a Film Mode value means.
fn film_simulation(mode: i64) -> Option<FilmSimulation> {
    Some(match mode {
        0x000 | 0x100 | 0x110 | 0x130 | 0x300 => FilmSimulation::Provia,
        0x120 => FilmSimulation::Astia,
        0x200 | 0x400 => FilmSimulation::Velvia,
        0x500 => FilmSimulation::ProNegStd,
        0x501 => FilmSimulation::ProNegHi,
        0x600 => FilmSimulation::ClassicChrome,
        0x700 => FilmSimulation::EternaCinema,
        0x800 => FilmSimulation::ClassicNeg,
        0x900 => FilmSimulation::EternaBleachBypass,
        0xa00 => FilmSimulation::NostalgicNeg,
        0xb00 => FilmSimulation::RealaAce,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A maker note holding `entries` as SHORT/SLONG values.
    fn note(entries: &[(u16, u16, &[i64])]) -> Vec<u8> {
        let mut out = b"FUJIFILM".to_vec();
        out.extend_from_slice(&12u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        let mut extra: Vec<u8> = Vec::new();
        let table_end = 12 + 2 + entries.len() * 12 + 4;
        for (tag, kind, values) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&(values.len() as u32).to_le_bytes());
            let size = if *kind == 3 { 2 } else { 4 };
            let mut data = Vec::new();
            for v in *values {
                match kind {
                    3 => data.extend_from_slice(&(*v as u16).to_le_bytes()),
                    9 => data.extend_from_slice(&(*v as i32).to_le_bytes()),
                    _ => data.extend_from_slice(&(*v as u32).to_le_bytes()),
                }
            }
            if size * values.len() <= 4 {
                data.resize(4, 0);
                out.extend_from_slice(&data);
            } else {
                out.extend_from_slice(&((table_end + extra.len()) as u32).to_le_bytes());
                extra.extend_from_slice(&data);
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&extra);
        out
    }

    fn recipe(entries: &[(u16, u16, &[i64])]) -> Option<FujiRecipe> {
        recipe_from_tags(&parse_tags(&note(entries))?)
    }

    #[test]
    fn reads_a_film_simulation_and_the_settings_around_it() {
        let r = recipe(&[
            (0x1401, 3, &[0x600]),
            (0x1003, 3, &[0x8000]),
            (0x1002, 3, &[0x0]),
            (0x1041, 9, &[-16]),
            (0x1040, 9, &[16]),
            (0x1047, 3, &[32]),
            (0x104c, 3, &[16]),
            (0x1048, 3, &[64]),
            (0x1403, 3, &[200]),
            (0x100a, 9, &[40, -20]),
        ])
        .unwrap();
        assert_eq!(r.film_simulation, FilmSimulation::ClassicChrome);
        assert_eq!(r.highlight, 1.0);
        assert_eq!(r.shadow, -1.0);
        assert_eq!(r.grain, GrainEffect::WeakSmall);
        assert_eq!(r.color_chrome, Strength::Strong);
        assert_eq!(r.dynamic_range, DynamicRange::Dr200);
        assert_eq!((r.wb_red, r.wb_blue), (2, -1));
    }

    #[test]
    fn black_and_white_comes_from_the_saturation_tag() {
        let acros_red = recipe(&[(0x1401, 3, &[0x0]), (0x1003, 3, &[0x501])]).unwrap();
        assert_eq!(acros_red.film_simulation, FilmSimulation::Acros);
        assert_eq!(acros_red.mono_filter, MonoFilter::Red);
        let mono = recipe(&[(0x1401, 3, &[0x0]), (0x1003, 3, &[0x300])]).unwrap();
        assert_eq!(mono.film_simulation, FilmSimulation::Monochrome);
        let sepia = recipe(&[(0x1401, 3, &[0x0]), (0x1003, 3, &[0x310])]).unwrap();
        assert_eq!(sepia.film_simulation, FilmSimulation::Sepia);
    }

    #[test]
    fn notes_that_arent_fujifilms_or_are_cut_short_give_nothing() {
        assert!(parse_tags(b"OLYMPUS\0stuff").is_none());
        assert!(parse_tags(b"FUJIFILM").is_none());
        // No film mode and not black and white: nothing to say.
        assert!(recipe(&[(0x1003, 3, &[0x0])]).is_none());
        // ...but black and white needs none (the camera doesn't write one).
        let bw = recipe(&[(0x1003, 3, &[0x501]), (0x1048, 3, &[64])]).unwrap();
        assert_eq!(bw.film_simulation, FilmSimulation::Acros);
        // An unknown film mode: nothing to say either.
        assert!(recipe(&[(0x1401, 3, &[0x7777]), (0x1003, 3, &[0x0])]).is_none());
    }
}
