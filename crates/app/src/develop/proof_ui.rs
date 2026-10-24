//! Soft-proofing in Develop: look at the photo as another colour space
//! (sRGB, a print profile…) would reproduce it, and mark the colours that
//! space can't hold. The controls live in the View menu; the preview
//! applies them to each rendered frame.

use std::path::PathBuf;

use gpui::{div, prelude::*, AnyElement, App, ClickEvent, Context, SharedString, Window};
use rustyedit_processing::{OutputSpace, ProofIntent, ProofSettings};
use rustyedit_ui::{sp, Theme};

use crate::workspace::Workspace;

/// What the photo is proofed against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ProofTarget {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
    ProPhoto,
    /// A profile loaded from a file.
    Custom,
}

impl ProofTarget {
    const BUILT_IN: [ProofTarget; 4] = [
        ProofTarget::Srgb,
        ProofTarget::DisplayP3,
        ProofTarget::AdobeRgb,
        ProofTarget::ProPhoto,
    ];

    fn label(&self) -> &'static str {
        match self {
            ProofTarget::Srgb => "sRGB",
            ProofTarget::DisplayP3 => "Display P3",
            ProofTarget::AdobeRgb => "Adobe RGB",
            ProofTarget::ProPhoto => "ProPhoto RGB",
            ProofTarget::Custom => "Custom profile",
        }
    }
}

/// Soft-proof state of the Develop module.
pub(crate) struct ProofUi {
    pub(crate) target: ProofTarget,
    pub(crate) intent: ProofIntent,
    pub(crate) simulate: bool,
    pub(crate) warn: bool,
    /// The loaded profile's file name and bytes.
    pub(crate) custom: Option<(String, Vec<u8>)>,
}

impl ProofUi {
    /// Starts with the profile loaded last time (if it's still there).
    pub(crate) fn new(remembered: Option<&PathBuf>) -> Self {
        let custom = remembered.and_then(|path| load_profile(path));
        Self {
            target: if custom.is_some() {
                ProofTarget::Custom
            } else {
                ProofTarget::Srgb
            },
            intent: ProofIntent::Relative,
            simulate: false,
            warn: false,
            custom,
        }
    }

    pub(crate) fn settings(&self) -> Option<ProofSettings> {
        if !self.simulate && !self.warn {
            return None;
        }
        let target = match self.target {
            ProofTarget::Srgb => OutputSpace::Srgb,
            ProofTarget::DisplayP3 => OutputSpace::DisplayP3,
            ProofTarget::AdobeRgb => OutputSpace::AdobeRgb,
            ProofTarget::ProPhoto => OutputSpace::ProPhoto,
            ProofTarget::Custom => OutputSpace::Profile(self.custom.as_ref()?.1.clone()),
        };
        Some(ProofSettings {
            target,
            intent: self.intent,
            simulate: self.simulate,
            warn: self.warn,
        })
    }

    pub(crate) fn active(&self) -> bool {
        self.simulate || self.warn
    }
}

/// Reads an ICC profile, only accepting ones the colour engine understands.
fn load_profile(path: &PathBuf) -> Option<(String, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    if !rustyedit_processing::is_proofing_profile(&bytes) {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some((name, bytes))
}

impl Workspace {
    fn edit_proof(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut ProofUi)) {
        change(&mut self.develop_ui.proof);
        self.request_preview_update(cx);
        cx.notify();
    }

    /// Asks for an ICC profile (e.g. a printer's) to proof against.
    fn load_proof_profile(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let file = cx
                .background_executor()
                .spawn(async {
                    rfd::FileDialog::new()
                        .set_title("Choose an ICC profile to proof against")
                        .add_filter("ICC profiles", &["icc", "icm"])
                        .pick_file()
                })
                .await;
            let Some(file) = file else { return };
            this.update(cx, |this, cx| match load_profile(&file) {
                Some(custom) => {
                    this.status_message = Some(format!("Proofing against {}.", custom.0).into());
                    this.update_settings(|s| s.develop.proof_profile = Some(file.clone()), cx);
                    this.edit_proof(cx, |p| {
                        p.custom = Some(custom);
                        p.target = ProofTarget::Custom;
                        p.simulate = true;
                    });
                }
                None => {
                    this.status_message =
                        Some("That file isn't an ICC profile this app can use.".into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The Soft proof part of the View menu.
    pub(super) fn proof_menu_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let proof = &self.develop_ui.proof;
        let entity = cx.entity();
        let mut rows: Vec<AnyElement> = vec![menu_heading(&theme, "SOFT PROOF")];

        let toggle = |id: &'static str, label: &'static str, on: bool, flip: fn(&mut ProofUi)| {
            let entity = entity.clone();
            menu_row(&theme, id, label, on).on_click(
                move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.edit_proof(cx, flip));
                },
            )
        };
        rows.push(
            toggle("proof-simulate", "Simulate the profile", proof.simulate, |p| {
                p.simulate = !p.simulate
            })
            .into_any_element(),
        );
        rows.push(
            toggle("proof-warn", "Mark out-of-gamut colours", proof.warn, |p| {
                p.warn = !p.warn
            })
            .into_any_element(),
        );

        for target in ProofTarget::BUILT_IN {
            let entity = entity.clone();
            rows.push(
                menu_row(&theme, target.label(), target.label(), proof.target == target)
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| this.edit_proof(cx, |p| p.target = target));
                    })
                    .into_any_element(),
            );
        }
        // The loaded profile (if any) is a choice of its own; loading
        // another replaces it.
        if let Some((name, _)) = &proof.custom {
            let entity = entity.clone();
            rows.push(
                menu_row(&theme, "proof-custom", name, proof.target == ProofTarget::Custom)
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.edit_proof(cx, |p| p.target = ProofTarget::Custom)
                        });
                    })
                    .into_any_element(),
            );
        }
        {
            let entity = entity.clone();
            rows.push(
                menu_row(&theme, "proof-load", "Load a profile\u{2026}", false)
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| this.load_proof_profile(cx));
                    })
                    .into_any_element(),
            );
        }
        for intent in ProofIntent::ALL {
            let entity = entity.clone();
            rows.push(
                menu_row(
                    &theme,
                    intent.label(),
                    match intent {
                        ProofIntent::Relative => "Intent: relative",
                        ProofIntent::Perceptual => "Intent: perceptual",
                    },
                    proof.intent == intent,
                )
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.edit_proof(cx, |p| p.intent = intent));
                })
                .into_any_element(),
            );
        }
        rows
    }
}

fn menu_heading(theme: &Theme, text: &'static str) -> AnyElement {
    div()
        .px_2()
        .pt_1()
        .pb_1()
        .text_size(sp(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(theme.text_faint)
        .child(text)
        .into_any_element()
}

fn menu_row(
    theme: &Theme,
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    div()
        .id(id.into())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(sp(26.0))
        .px_2()
        .cursor_pointer()
        .text_size(sp(11.5))
        .text_color(if on { theme.text } else { theme.text_muted })
        .hover(|el| el.bg(theme.surface).text_color(theme.text))
        .child(
            div()
                .w(sp(12.0))
                .text_color(theme.accent)
                .child(if on { "\u{2713}" } else { "" }),
        )
        .child(
            div()
                .flex_1()
                .min_w(sp(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label.into()),
        )
}
