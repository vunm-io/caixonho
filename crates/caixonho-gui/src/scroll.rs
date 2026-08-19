//! Scroll acceleration for the bucket table.
//!
//! Two devices, two curves: a mouse wheel arrives as discrete notches and
//! gets a streak accelerator, while a trackpad arrives as a stream of small
//! precise deltas already carrying OS momentum and gets a stateless
//! velocity curve. Stacking the streak accelerator on top of OS momentum was
//! tried and reverted — it compounds into fly-away scrolling.

use std::time::{Duration, Instant};

use gpui::{Bounds, Entity, IntoElement, Pixels, ScrollWheelEvent, Styled, Window, canvas, px};
use gpui_component::table::{TableDelegate, TableState};

/// Wheel-acceleration tuning.
///
/// The handler takes the wheel event over entirely (capture phase +
/// `stop_propagation`), so these are absolute multipliers on the native
/// per-notch distance rather than additions to it. A lone notch moves
/// `WHEEL_BASE_MULTIPLIER`; notches arriving within `WHEEL_ACCEL_WINDOW` of
/// each other build a streak worth `WHEEL_ACCEL_STEP` apiece, capped at
/// `WHEEL_MAX_MULTIPLIER`. Pausing resets the streak, so precision scrolling
/// stays precise.
const WHEEL_ACCEL_WINDOW: Duration = Duration::from_millis(140);
/// Speed of a single unhurried notch, relative to native.
const WHEEL_BASE_MULTIPLIER: f32 = 1.6;
/// Added to the multiplier per consecutive fast notch.
const WHEEL_ACCEL_STEP: f32 = 0.5;
/// Ceiling for a sustained flick.
const WHEEL_MAX_MULTIPLIER: f32 = 8.0;

/// Trackpad-flick tuning.
///
/// Trackpads (`ScrollDelta::precise()`) need different treatment: the OS
/// already applies momentum, and the time-streak accelerator stacked on top
/// of it compounds into fly-away scrolling (tried and reverted). Instead each
/// precise event gets a stateless multiplier from its own velocity. Gentle
/// scrolling stays under `TRACKPAD_BOOST_THRESHOLD` px/event and is left
/// entirely on the native path, so precision is untouched; past it a flick
/// ramps linearly to `TRACKPAD_MAX_MULTIPLIER` over `TRACKPAD_BOOST_SPAN`
/// px/event. The OS momentum tail runs through the same curve: it starts fast
/// (boosted, so the coast actually covers ground in a 100k-row table) and
/// decays back under the threshold (native again, so it settles precisely).
const TRACKPAD_BOOST_THRESHOLD: f32 = 12.0;
/// px/event over which the boost ramps from 1x up to the ceiling.
const TRACKPAD_BOOST_SPAN: f32 = 90.0;
/// Ceiling for a hard flick and the start of its momentum tail.
const TRACKPAD_MAX_MULTIPLIER: f32 = 4.0;

/// Stateless velocity→multiplier curve for precise (trackpad) scrolling.
/// `None` means "leave this event entirely on the native path".
fn trackpad_multiplier(speed: f32) -> Option<f32> {
    let ramp = (speed - TRACKPAD_BOOST_THRESHOLD) / TRACKPAD_BOOST_SPAN;
    if ramp <= 0.0 {
        return None;
    }
    Some(1.0 + ramp.min(1.0) * (TRACKPAD_MAX_MULTIPLIER - 1.0))
}

/// Wheel-streak state, kept in its own entity so the accelerator does not
/// need to reach into the app.
#[derive(Default)]
pub struct ScrollAccel {
    last_wheel: Option<Instant>,
    wheel_streak: f32,
}

impl ScrollAccel {
    /// The multiplier for a wheel notch arriving now.
    fn wheel_multiplier(&mut self) -> f32 {
        let now = Instant::now();
        let rapid = self
            .last_wheel
            .is_some_and(|previous| now - previous < WHEEL_ACCEL_WINDOW);
        self.last_wheel = Some(now);
        self.wheel_streak = if rapid { self.wheel_streak + 1.0 } else { 0.0 };
        (WHEEL_BASE_MULTIPLIER + self.wheel_streak * WHEEL_ACCEL_STEP).min(WHEEL_MAX_MULTIPLIER)
    }
}

/// An invisible overlay that takes mouse-wheel events over from the table
/// and re-applies them with acceleration.
///
/// It has to run in the **capture** phase: the table scrolls through
/// gpui's `uniform_list`, whose own wheel listener runs in the bubble
/// phase, and anything we add afterwards is overwritten when the list
/// paints. Capturing first — and stopping propagation — makes this the
/// only code that moves the table, so the multiplier is exact instead of
/// being layered onto a scroll that already happened.
///
/// A `canvas` is used purely as a hook into the paint phase, where
/// `Window::on_mouse_event` can be registered; it draws nothing and
/// inserts no hitbox, so it never blocks clicks on rows.
pub fn accelerator<D: TableDelegate + 'static>(
    table: Entity<TableState<D>>,
    accel: Entity<ScrollAccel>,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let line_height = window.line_height();
            window.on_mouse_event(
                move |event: &ScrollWheelEvent, phase, window: &mut Window, cx| {
                    if !phase.capture() || !bounds.contains(&event.position) {
                        return;
                    }
                    // Horizontal scrolling keeps native behaviour.
                    let delta = event.delta.pixel_delta(line_height);
                    if delta.y.abs() <= delta.x.abs() {
                        return;
                    }

                    let multiplier = if event.delta.precise() {
                        // Trackpad: stateless velocity curve. Under the
                        // threshold the event stays on the native path.
                        match trackpad_multiplier(f32::from(delta.y.abs())) {
                            Some(multiplier) => multiplier,
                            None => return,
                        }
                    } else {
                        accel.update(cx, |accel, _| accel.wheel_multiplier())
                    };

                    let handle = table
                        .read(cx)
                        .vertical_scroll_handle
                        .0
                        .borrow()
                        .base_handle
                        .clone();
                    let max = handle.max_offset().y;
                    let mut offset = handle.offset();
                    // Offsets run from 0 (top) to -max (bottom).
                    offset.y = (offset.y + delta.y * multiplier).clamp(-max, px(0.));
                    handle.set_offset(offset);

                    cx.stop_propagation();
                    window.refresh();
                },
            );
        },
    )
    .absolute()
    .size_full()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gentle_trackpad_scrolling_stays_native() {
        assert_eq!(trackpad_multiplier(6.0), None);
        assert_eq!(trackpad_multiplier(TRACKPAD_BOOST_THRESHOLD), None);
    }

    #[test]
    fn flick_ramps_linearly_then_caps() {
        let mid = trackpad_multiplier(TRACKPAD_BOOST_THRESHOLD + TRACKPAD_BOOST_SPAN / 2.0)
            .expect("above threshold must boost");
        assert!((mid - (1.0 + (TRACKPAD_MAX_MULTIPLIER - 1.0) / 2.0)).abs() < 1e-4);

        let capped = trackpad_multiplier(TRACKPAD_BOOST_THRESHOLD + TRACKPAD_BOOST_SPAN * 3.0)
            .expect("above threshold must boost");
        assert_eq!(capped, TRACKPAD_MAX_MULTIPLIER);
    }
}
