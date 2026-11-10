//! Where the camera focused: the autofocus point it recorded in the file,
//! as fractions of the upright photo (0, 0 top left; 1, 1 bottom right).
//!
//! Three places are read: Fujifilm's `FocusPixel` and Sony's `FocusLocation`
//! (both in the maker note), and the standard EXIF `SubjectArea` (the focus
//! point or area on many other cameras). Photos developed elsewhere (a DNG
//! exported from Lightroom) lose the maker note, so the RAW beside them
//! with the same name is tried too.

use std::{fs::File, io::BufReader, path::Path};

use exif::{Exif, In, Tag, Value};

/// Sony's maker-note tag holding the focus position: image width, image
/// height, x, y.
const SONY_FOCUS_LOCATION: u16 = 0x2027;

/// The focus point of `path` on the upright photo, or `None` when the file
/// doesn't say. Reads the file; call off the UI thread.
pub fn read_focus_point(path: &Path) -> Option<(f32, f32)> {
    own_focus_point(path).or_else(|| {
        // A converted copy: look for the original RAW next to it.
        crate::decode::raw_extensions()
            .iter()
            .filter(|ext| !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
            .flat_map(|ext| [ext.to_string(), ext.to_uppercase()])
            .map(|ext| path.with_extension(ext))
            .find(|sibling| sibling != path && sibling.is_file())
            .and_then(|sibling| own_focus_point(&sibling))
    })
}

fn own_focus_point(path: &Path) -> Option<(f32, f32)> {
    // Fujifilm keeps it in its own maker-note tag (and RAFs aren't readable
    // as plain EXIF at all).
    if let Some(((x, y), orientation)) = crate::fuji::read_fuji_focus(path) {
        return Some(upright((x, y), orientation));
    }
    let file = File::open(path).ok()?;
    let exif = exif::Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .ok()?;
    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .unwrap_or(1) as u8;
    let (x, y) = sony_focus_location(&exif).or_else(|| subject_area(&exif))?;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    Some(upright((x, y), orientation))
}

/// EXIF `SubjectArea`: a point (2 values) or an area (3, 4) in pixels of
/// the image, centre first.
fn subject_area(exif: &Exif) -> Option<(f32, f32)> {
    let area = exif.get_field(Tag::SubjectArea, In::PRIMARY)?;
    let Value::Short(v) = &area.value else {
        return None;
    };
    if v.len() < 2 {
        return None;
    }
    let dim = |tags: &[Tag]| {
        tags.iter()
            .find_map(|t| exif.get_field(*t, In::PRIMARY)?.value.get_uint(0))
            .filter(|d| *d > 0)
    };
    let width = dim(&[Tag::PixelXDimension, Tag::ImageWidth])? as f32;
    let height = dim(&[Tag::PixelYDimension, Tag::ImageLength])? as f32;
    Some((v[0] as f32 / width, v[1] as f32 / height))
}

/// Sony's `FocusLocation` from the maker note.
fn sony_focus_location(exif: &Exif) -> Option<(f32, f32)> {
    let field = exif.get_field(Tag::MakerNote, In::PRIMARY)?;
    let Value::Undefined(note, at) = &field.value else {
        return None;
    };
    let is_sony = exif
        .get_field(Tag::Make, In::PRIMARY)
        .is_some_and(|m| m.display_value().to_string().to_uppercase().contains("SONY"));
    if !is_sony {
        return None;
    }
    // Newer bodies start the directory right away; older ones put a
    // 12-byte "SONY DSC" header first.
    let start = if note.starts_with(b"SONY DSC") { 12 } else { 0 };
    focus_location(note, *at as usize, start, exif.little_endian())
}

/// The `FocusLocation` entry of the maker-note directory at `start` in
/// `note`, which sits at `note_at` in the file (the entries' offsets are
/// from the start of the file).
fn focus_location(note: &[u8], note_at: usize, start: usize, little: bool) -> Option<(f32, f32)> {
    let u16_at = |o: usize| {
        let b: [u8; 2] = note.get(o..o + 2)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |o: usize| {
        let b: [u8; 4] = note.get(o..o + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let entries = u16_at(start)? as usize;
    for i in 0..entries.min(400) {
        let entry = start + 2 + i * 12;
        if u16_at(entry)? != SONY_FOCUS_LOCATION {
            continue;
        }
        // Four 16-bit numbers (8 bytes) live outside the entry.
        if u16_at(entry + 2)? != 3 || u32_at(entry + 4)? != 4 {
            return None;
        }
        let offset = u32_at(entry + 8)? as usize;
        let values = |at: usize| -> Option<[u16; 4]> {
            Some([u16_at(at)?, u16_at(at + 2)?, u16_at(at + 4)?, u16_at(at + 6)?])
        };
        let [w, h, x, y] = values(offset.checked_sub(note_at)?)?;
        if w == 0 || h == 0 || (x == 0 && y == 0) {
            return None;
        }
        return Some((x as f32 / w as f32, y as f32 / h as f32));
    }
    None
}

/// A point on the sensor's frame → the same point on the photo turned
/// upright by its EXIF `orientation`.
fn upright((x, y): (f32, f32), orientation: u8) -> (f32, f32) {
    match orientation {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (1.0 - y, x),
        7 => (1.0 - y, 1.0 - x),
        8 => (y, 1.0 - x),
        _ => (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_photo_moves_the_point_with_it() {
        // Top-left corner area of a landscape frame, turned for portrait.
        let p = (0.2, 0.1);
        // Rotated 90 degrees clockwise (orientation 6): it ends up top right.
        let (x, y) = upright(p, 6);
        assert!((x - 0.9).abs() < 1e-6 && (y - 0.2).abs() < 1e-6);
        // Upside down (3).
        assert_eq!(upright(p, 3), (0.8, 0.9));
        // 270 (8): bottom left.
        let (x, y) = upright(p, 8);
        assert!((x - 0.1).abs() < 1e-6 && (y - 0.8).abs() < 1e-6);
        assert_eq!(upright(p, 1), p);
    }

    #[test]
    fn sony_focus_location_is_found_in_a_maker_note() {
        // A directory at the note's start (placed at file offset 100) with
        // two entries; the second is FocusLocation, its numbers stored
        // after the directory.
        let mut note = Vec::new();
        note.extend(2u16.to_le_bytes());
        // Entry 1: something else.
        note.extend(0x2010u16.to_le_bytes());
        note.extend(1u16.to_le_bytes());
        note.extend(1u32.to_le_bytes());
        note.extend(7u32.to_le_bytes());
        // Entry 2: FocusLocation, 4 shorts at file offset 100 + 26.
        note.extend(SONY_FOCUS_LOCATION.to_le_bytes());
        note.extend(3u16.to_le_bytes());
        note.extend(4u32.to_le_bytes());
        note.extend(126u32.to_le_bytes());
        for v in [6000u16, 4000, 4500, 1000] {
            note.extend(v.to_le_bytes());
        }
        assert_eq!(focus_location(&note, 100, 0, true), Some((0.75, 0.25)));
    }

    #[test]
    fn an_unset_location_is_ignored() {
        let mut note = Vec::new();
        note.extend(1u16.to_le_bytes());
        note.extend(SONY_FOCUS_LOCATION.to_le_bytes());
        note.extend(3u16.to_le_bytes());
        note.extend(4u32.to_le_bytes());
        note.extend(16u32.to_le_bytes());
        for v in [6000u16, 4000, 0, 0] {
            note.extend(v.to_le_bytes());
        }
        assert_eq!(focus_location(&note, 0, 0, true), None);
    }
}
