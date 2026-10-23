//! Asks before quitting. Every way out comes through here: the Quit action
//! (⌘Q / the menu), the title bar's close button, and the OS asking the
//! window to close (Alt+F4, the Windows caption button, macOS's red light).

use std::cell::Cell;

use gpui::{App, PromptLevel, Window};

thread_local! {
    /// Set once the user has said yes, so the window's close hook lets go.
    static CONFIRMED: Cell<bool> = const { Cell::new(false) };
    /// A prompt is already up; don't stack a second one.
    static PROMPTING: Cell<bool> = const { Cell::new(false) };
}

/// Hook for the window's close request: allows it only once confirmed,
/// otherwise asks and keeps the window open meanwhile.
pub(crate) fn should_close(window: &mut Window, cx: &mut App) -> bool {
    if CONFIRMED.get() {
        return true;
    }
    confirm_quit(window, cx);
    false
}

/// Asks from the active window (or quits outright if there's none to ask
/// from).
pub(crate) fn request_quit(cx: &mut App) {
    let asked = cx
        .active_window()
        .or_else(|| cx.windows().first().copied())
        .and_then(|handle| handle.update(cx, |_, window, cx| confirm_quit(window, cx)).ok());
    if asked.is_none() {
        cx.quit();
    }
}

fn confirm_quit(window: &mut Window, cx: &mut App) {
    if PROMPTING.replace(true) {
        return;
    }
    let answer = window.prompt(
        PromptLevel::Warning,
        "Quit RustyEdit?",
        Some("Are you sure you want to quit?"),
        &["Quit", "Cancel"],
        cx,
    );
    cx.spawn(async move |cx| {
        let quit = answer.await == Ok(0);
        PROMPTING.set(false);
        if quit {
            CONFIRMED.set(true);
            cx.update(|cx| cx.quit()).ok();
        }
    })
    .detach();
}
