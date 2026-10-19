use gpui::{div, prelude::*, px, App, SharedString, Window};
use rustyedit_core::Rating;

use crate::theme::Theme;

/// A row of five stars reflecting `rating`, filled up to its value. Pass an
/// `on_set` callback to make the stars clickable (e.g. in the status bar);
/// pass `None` to render a read-only badge (e.g. inside a grid cell).
pub fn rating_stars(
    id: impl Into<SharedString>,
    rating: Rating,
    theme: &Theme,
    size: f32,
    on_set: Option<impl Fn(u8, &mut Window, &mut App) + 'static>,
) -> impl IntoElement {
    let theme = *theme;
    let value = rating.value();
    let id: SharedString = id.into();

    let on_set = on_set.map(std::rc::Rc::new);

    div()
        .flex()
        .flex_row()
        .gap_1()
        .children((1..=5u8).map(|star| {
            let filled = star <= value;
            let on_set = on_set.clone();
            let star_id = format!("{}-star-{}", id, star);
            let mut el = div()
                .id(SharedString::from(star_id))
                .text_size(px(size))
                .text_color(if filled { theme.star } else { theme.text_faint })
                .child(if filled { "\u{2605}" } else { "\u{2606}" });
            if let Some(on_set) = on_set {
                el = el
                    .cursor_pointer()
                    .tooltip(crate::tooltip::tip(
                        if star == 1 {
                            "1 star".to_string()
                        } else {
                            format!("{star} stars")
                        },
                        &theme,
                    ))
                    .on_click(move |_, window, cx| on_set(star, window, cx));
            }
            el
        }))
}
