//! Hand cursors for dragging a photo around, per platform.
//!
//! gpui maps its open / closed hand styles to the native grab cursors on
//! macOS and Linux, but Windows has no such cursors and gpui falls back to
//! the plain arrow there, so a held photo shows no hand at all. Windows
//! gets its own hand (the pointing one) instead.

use gpui::CursorStyle;

/// Over something that can be dragged.
pub const GRAB_CURSOR: CursorStyle = if cfg!(target_os = "windows") {
    CursorStyle::Arrow
} else {
    CursorStyle::OpenHand
};

/// While the mouse is held down on it: dragging.
pub const GRABBING_CURSOR: CursorStyle = if cfg!(target_os = "windows") {
    CursorStyle::PointingHand
} else {
    CursorStyle::ClosedHand
};
