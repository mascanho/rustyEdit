//! The editing panel's slider sections, and every change to a photo's
//! Develop settings: slider moves, and whole-state changes (presets,
//! snapshots, history steps, pasted or reset edits).

use gpui::{hsla, prelude::*, AnyElement, Context, Hsla};
use rustyedit_core::{Adjustment, ColorBand, DevelopSettings, GradeProp, MixerProp};
use rustyedit_ui::{SliderSpec, SliderTrack};

use crate::workspace::{Mode, Workspace};

/// How a whole-state change shows up in each photo's history.
#[derive(Clone, Copy)]
pub(crate) enum HistoryNote<'a> {
    /// Logged as `title` with `detail` as its new value.
    Record { title: &'a str, detail: &'a str },
    /// Not logged; used when stepping back through the history itself.
    Skip,
}

impl Workspace {
    /// A slider per adjustment (Basic, Color, Detail, Effects).
    pub(super) fn adjustment_sliders(
        &self,
        adjustments: &[Adjustment],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        adjustments
            .iter()
            .map(|adj| self.adjustment_slider(*adj, cx))
            .collect()
    }

    /// One slider bound to `adjustment` on the selected photo.
    pub(super) fn adjustment_slider(
        &self,
        adjustment: Adjustment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let develop = self
            .catalog
            .selected_asset()
            .map(|asset| asset.develop)
            .unwrap_or_default();
        let (min, max) = adjustment.range();
        let spec = SliderSpec {
            min,
            max,
            default: adjustment.default_value(),
            signed: adjustment.is_signed(),
            track: slider_track(adjustment, &develop),
        };
        let entity = cx.entity();

        rustyedit_ui::labeled_slider(
            gpui::SharedString::from(format!("adjustment-{:?}", adjustment)),
            adjustment.label(),
            adjustment.get(&develop),
            spec,
            &self.theme,
            move |new_value, _window, cx| {
                entity.update(cx, |this, cx| {
                    this.apply_adjustment(adjustment, new_value, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(crate) fn apply_adjustment(
        &mut self,
        adjustment: Adjustment,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self
            .catalog
            .selected_asset()
            .map(|a| adjustment.get(&a.develop))
        else {
            return;
        };
        let format = |v: f32| {
            if adjustment.is_signed() {
                format!("{v:+.0}")
            } else {
                format!("{v:.0}")
            }
        };
        self.edit_develop(
            &adjustment.history_label(),
            |develop| adjustment.set(develop, value),
            |develop| (format(before), format(adjustment.get(develop))),
            cx,
        );
    }

    /// Changes the selected photo's Develop settings in place: one undo
    /// step per gesture (a drag's changes under the same `label` merge),
    /// logged in its history as `label` with the `(from, to)` that
    /// `describe` gives for the new settings.
    pub(crate) fn edit_develop(
        &mut self,
        label: &str,
        edit: impl FnOnce(&mut DevelopSettings),
        describe: impl FnOnce(&DevelopSettings) -> (String, String),
        cx: &mut Context<Self>,
    ) {
        let ix = self.catalog.selected;
        let Some(asset) = self.catalog.assets.get_mut(ix) else {
            return;
        };
        let before = asset.state();
        edit(&mut asset.develop);
        if asset.develop == before.develop {
            return;
        }
        let (from, to) = describe(&asset.develop);
        asset.record_develop_change(label, from, to);
        let after = asset.state();
        self.push_undo(label.to_string(), vec![(ix, before, after)], true);
        self.mark_catalog_dirty(cx);
        self.request_preview_update(cx);
        cx.notify();
    }

    /// Replaces the Develop settings of every photo in `targets` as one undo
    /// step named `undo_label`.
    pub(crate) fn replace_develop(
        &mut self,
        targets: Vec<usize>,
        undo_label: &str,
        note: HistoryNote,
        cx: &mut Context<Self>,
        new: impl Fn(&DevelopSettings) -> DevelopSettings,
    ) {
        let updates = targets
            .into_iter()
            .filter_map(|ix| Some((ix, new(&self.catalog.assets.get(ix)?.develop))))
            .collect();
        self.replace_develop_each(updates, undo_label, note, cx);
    }

    /// Gives each photo in `updates` its own new Develop settings, as one
    /// undo step named `undo_label`.
    pub(crate) fn replace_develop_each(
        &mut self,
        updates: Vec<(usize, DevelopSettings)>,
        undo_label: &str,
        note: HistoryNote,
        cx: &mut Context<Self>,
    ) {
        let mut changes = Vec::new();
        for (ix, develop) in updates {
            let Some(asset) = self.catalog.assets.get_mut(ix) else {
                continue;
            };
            let before = asset.state();
            asset.develop = develop;
            if asset.develop == before.develop {
                continue;
            }
            if let HistoryNote::Record { title, detail } = note {
                asset.record_develop_change(title, "", detail);
            }
            changes.push((ix, before, asset.state()));
        }
        if changes.is_empty() {
            return;
        }

        // Rotation is baked into cached thumbnails, which need rebuilding.
        let rotated: Vec<usize> = changes
            .iter()
            .filter(|(_, before, after)| {
                before.develop.rotation_degrees != after.develop.rotation_degrees
            })
            .map(|(ix, ..)| *ix)
            .collect();
        self.push_undo(undo_label.to_string(), changes, false);
        self.mark_catalog_dirty(cx);
        if !rotated.is_empty() {
            self.refresh_asset_thumbnails(&rotated, cx);
        }
        if self.mode == Mode::Develop {
            self.request_preview_update(cx);
        }
        cx.notify();
    }
}

/// A colour on the hue wheel, `hue` in degrees.
pub(super) fn wheel(hue: f32, saturation: f32, lightness: f32) -> Hsla {
    hsla(hue.rem_euclid(360.0) / 360.0, saturation, lightness, 1.0)
}

/// Colour ramps for sliders whose meaning is a colour, so the track shows
/// what dragging does; plain sliders keep the accent fill.
fn slider_track(adjustment: Adjustment, develop: &DevelopSettings) -> SliderTrack {
    let grey = hsla(0.0, 0.0, 0.5, 1.0);
    match adjustment {
        Adjustment::Temperature => {
            SliderTrack::Gradient(wheel(215.0, 0.6, 0.55), wheel(48.0, 0.8, 0.55))
        }
        Adjustment::Tint => SliderTrack::Gradient(wheel(120.0, 0.5, 0.5), wheel(300.0, 0.5, 0.55)),
        Adjustment::Mixer(band, prop) => mixer_track(band, prop),
        Adjustment::Grade(_, GradeProp::Hue) => SliderTrack::Hue,
        Adjustment::Grade(zone, GradeProp::Saturation) => {
            SliderTrack::Gradient(grey, wheel(develop.grading.zone(zone).hue, 0.8, 0.5))
        }
        _ => SliderTrack::Accent,
    }
}

fn mixer_track(band: ColorBand, prop: MixerProp) -> SliderTrack {
    let hue = band.hue();
    match prop {
        MixerProp::Hue => {
            SliderTrack::Gradient(wheel(hue - 30.0, 0.7, 0.5), wheel(hue + 30.0, 0.7, 0.5))
        }
        MixerProp::Saturation => {
            SliderTrack::Gradient(hsla(0.0, 0.0, 0.5, 1.0), wheel(hue, 0.85, 0.5))
        }
        MixerProp::Luminance => SliderTrack::Gradient(wheel(hue, 0.6, 0.2), wheel(hue, 0.6, 0.8)),
    }
}
