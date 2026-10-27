//! Reusable GPUI widgets shared between the Library (culling) and Develop
//! (editing) views: theme, buttons, sliders, badges and the title/status
//! bars. Nothing here knows about `Catalog` or `Workspace` — everything
//! takes plain values and callbacks, so it stays testable and reusable.

mod button;
mod controls;
mod cull_bar;
mod cursor;
mod film_look;
mod flag_badge;
mod histogram;
mod scopes;
mod icon_rail;
mod icons;
mod panel;
mod rating_stars;
mod scrollbar;
mod slider;
mod status_bar;
mod theme;
mod title_bar;
mod tooltip;
mod scale;

pub use button::{chrome_button, chrome_bottom_tab, chrome_icon_button, chrome_tab, glyph_button};
pub use controls::{mini_toggle_switch, stepper, toggle_switch};
pub use cull_bar::{
    cull_bar, cull_button, cull_group, cull_nav_button, label_swatch, CULL_BAR_HEIGHT,
};
pub use cursor::{GRABBING_CURSOR, GRAB_CURSOR};
pub use film_look::{EdgePrint, FilmLook, Sprockets};
pub use flag_badge::flag_badge;
pub use scopes::{scope_view, SCOPE_HEIGHT};
pub use histogram::{
    histogram_view, histogram_view_with_height, paint_histogram, HistogramPaint, HistogramProps,
    HISTOGRAM_HEIGHT,
};
pub use icon_rail::{icon_rail, icon_rail_with, RailButton, RAIL_BUTTON_SIZE, RAIL_WIDTH};
pub use icons::{icon, Icon};
pub use panel::{
    collapsible_panel, collapsible_panel_pinned, disclosure_section, disclosure_section_with,
    footer_button, panel_footer, section, side_panel, PanelSide, SectionExtras,
    COLLAPSED_PANEL_WIDTH, PANEL_HEADER_HEIGHT,
};
pub use rating_stars::rating_stars;
pub use scrollbar::vertical_scrollbar;
pub use slider::{
    compact_slider, labeled_slider, reset_button, slider_keystroke, sliders_pointer_down,
    SliderSpec, SliderTrack,
};
pub use status_bar::{status_bar, status_divider, STATUS_BAR_HEIGHT};
pub use theme::Theme;
pub use title_bar::{title_bar, CloseWindow, TITLE_BAR_HEIGHT};
pub use tooltip::tip;
pub use scale::{set_ui_scale, sp, ui_scale};
