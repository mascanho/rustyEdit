use crate::sp;
use gpui::{div, prelude::*, AnyView, App, Context, SharedString, Window};

use crate::theme::Theme;

/// A small dark label that appears after hovering an element for a moment.
pub struct Tip {
    text: SharedString,
    theme: Theme,
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .text_size(sp(11.0))
            .text_color(theme.text)
            .child(self.text.clone())
    }
}

/// Builder for GPUI's `.tooltip(..)`:
/// `div().id("x").tooltip(tip("Save", &theme))`.
pub fn tip(
    text: impl Into<SharedString>,
    theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    let theme = *theme;
    move |_window, cx| {
        let text = text.clone();
        cx.new(|_| Tip { text, theme }).into()
    }
}
