//! The display's colour profile. GPUI hands its pixels to the display
//! unconverted, so the live preview converts the edited photo into the
//! display's own colour space itself; this tells it which one that is.

/// Re-reads the main display's profile. Call on the main thread, e.g.
/// whenever a photo is opened in Develop (the window may have moved to
/// another display since).
pub(crate) fn refresh() {
    rustyedit_processing::set_display_profile(main_display_icc());
}

#[cfg(target_os = "macos")]
fn main_display_icc() -> Option<Vec<u8>> {
    let mtm = objc2::MainThreadMarker::new()?;
    let screen = objc2_app_kit::NSScreen::mainScreen(mtm)?;
    let data = screen.colorSpace()?.ICCProfileData()?;
    Some(data.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn main_display_icc() -> Option<Vec<u8>> {
    None
}
