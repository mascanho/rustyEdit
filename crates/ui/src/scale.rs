//! The interface scale (Settings ▸ Appearance ▸ Interface scale).
//!
//! Fixed UI sizes are written as `sp(12.0)` ("scaled pixels") instead of
//! `px(12.0)`: the same at 100%, and grown or shrunk with the setting.
//! Positions and sizes worked out from real bounds (the photo's rectangle,
//! the pointer) stay in plain `px`, so they're never scaled twice. Sizes in
//! rems (`text_sm`, `gap_2`, ...) follow the window's rem size, which the
//! app sets from the same value.

use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{px, Pixels};

static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

/// The interface scale, 1.0 = 100%.
pub fn ui_scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

/// Sets the interface scale (clamped to 50-200%).
pub fn set_ui_scale(scale: f32) {
    SCALE.store(scale.clamp(0.5, 2.0).to_bits(), Ordering::Relaxed);
}

/// A fixed UI size, in pixels at 100%.
pub fn sp(value: f32) -> Pixels {
    px(value * ui_scale())
}
