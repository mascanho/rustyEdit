//! Reusable GPUI widgets shared between the Library (culling) and Develop
//! (editing) views: theme, buttons, sliders, badges and the title/status
//! bars. Nothing here knows about `Catalog` or `Workspace` — everything
//! takes plain values and callbacks, so it stays testable and reusable.

mod button;
mod controls;
mod cull_bar;
mod film_look;
mod flag_badge;
mod histogram;
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

pub use button::{chrome_button, chrome_icon_button, glyph_button};
pub use controls::{stepper, toggle_switch};
pub use cull_bar::{
    cull_bar, cull_button, cull_group, cull_nav_button, label_swatch, CULL_BAR_HEIGHT,
};
pub use film_look::{EdgePrint, FilmLook, Sprockets};
pub use flag_badge::flag_badge;
pub use histogram::{histogram_view, HISTOGRAM_HEIGHT};
pub use icon_rail::{icon_rail, RailButton, RAIL_WIDTH};
pub use icons::{icon, Icon};
pub use panel::{
    collapsible_panel, disclosure_section, section, side_panel, PanelSide, COLLAPSED_PANEL_WIDTH,
    PANEL_HEADER_HEIGHT,
};
pub use rating_stars::rating_stars;
pub use scrollbar::vertical_scrollbar;
pub use slider::{labeled_slider, SliderSpec, SliderTrack};
pub use status_bar::{status_bar, status_divider, STATUS_BAR_HEIGHT};
pub use theme::Theme;
pub use title_bar::{title_bar, TITLE_BAR_HEIGHT};
pub use tooltip::tip;
