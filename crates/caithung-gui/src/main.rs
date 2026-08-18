// A GUI app must not spawn a console window on Windows. Debug builds keep one
// so `println!` and panic output stay visible while developing.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! # M0 spike — prove the UI stack before any product code.
//!
//! What this binary must demonstrate (see `docs/adr/0001-ui-framework.md`):
//!
//! 1. A GPUI window opens on Windows 11 and macOS from this one source tree.
//! 2. A **virtualized** table scrolls smoothly through 100,000 rows.
//! 3. Rows arrive **asynchronously from a tokio runtime** — the same
//!    tokio-on-background-threads → channel → GPUI-executor bridge the real
//!    app will use for S3 calls. Nothing network-shaped runs on the render
//!    thread.
//! 4. An FPS overlay and a first-paint timer put numbers on "smooth".
//!
//! Everything in this file is spike code: it may be rewritten or deleted
//! without ceremony once M0 is decided. The bridge pattern is the one part
//! expected to survive, in `caithung-core`, in a hardened form.

use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Pixels, Render,
    ScrollWheelEvent, SharedString, Styled, Window, WindowBounds, WindowOptions, canvas, div, px,
    size,
};
use gpui_component::{
    ActiveTheme, Root, TitleBar, h_flex,
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};
use gpui_fps::fps_monitor;

/// Total rows the feed produces: the M0 bar from the project brief.
const TOTAL_ROWS: usize = 100_000;
/// Rows per simulated "page", roughly a `ListObjectsV2` page (max 1000) ×5,
/// so the table visibly fills in batches instead of popping in at once.
const BATCH_SIZE: usize = 5_000;
/// Simulated network latency between pages.
const BATCH_DELAY: Duration = Duration::from_millis(60);

/// One synthetic S3 object. Shapes match the real listing columns so the
/// spike's rendering cost is honest about the product's.
struct ObjectRow {
    key: SharedString,
    size: u64,
    last_modified: SharedString,
    storage_class: &'static str,
    etag: SharedString,
}

/// Deterministic pseudo-data — no RNG dependency, reproducible timings.
fn synth_row(ix: usize) -> ObjectRow {
    // A cheap integer mix so sizes/dates don't look striped.
    let h = (ix as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(17);
    let dirs = ["logs", "backups", "images", "exports", "raw", "builds"];
    let exts = ["parquet", "json", "csv", "png", "tar.gz", "log"];
    let classes = [
        "STANDARD",
        "STANDARD_IA",
        "GLACIER_IR",
        "INTELLIGENT_TIERING",
    ];
    let dir = dirs[(h % 6) as usize];
    let ext = exts[((h >> 8) % 6) as usize];
    ObjectRow {
        key: format!("{dir}/2026/{:02}/object-{ix:06}.{ext}", (h >> 16) % 12 + 1).into(),
        size: h % 4_000_000_000,
        last_modified: format!(
            "2026-{:02}-{:02} {:02}:{:02}",
            (h >> 16) % 12 + 1,
            (h >> 24) % 28 + 1,
            (h >> 32) % 24,
            (h >> 40) % 60
        )
        .into(),
        storage_class: classes[((h >> 48) % 4) as usize],
        etag: format!("\"{:016x}\"", h).into(),
    }
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KiB", b as f64 / (1u64 << 10) as f64),
        b => format!("{b} B"),
    }
}

struct SpikeDelegate {
    columns: Vec<Column>,
    rows: Vec<ObjectRow>,
}

impl SpikeDelegate {
    fn new() -> Self {
        Self {
            columns: vec![
                Column::new("key", "Key").width(px(420.)),
                Column::new("size", "Size").width(px(110.)).text_right(),
                Column::new("modified", "Last modified").width(px(160.)),
                Column::new("class", "Storage class").width(px(170.)),
                Column::new("etag", "ETag").width(px(190.)),
            ],
            rows: Vec::with_capacity(TOTAL_ROWS),
        }
    }
}

impl TableDelegate for SpikeDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        let text: SharedString = match col_ix {
            0 => row.key.clone(),
            1 => human_size(row.size).into(),
            2 => row.last_modified.clone(),
            3 => row.storage_class.into(),
            4 => row.etag.clone(),
            _ => "".into(),
        };
        div().child(text)
    }
}

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

struct SpikeApp {
    table: Entity<TableState<SpikeDelegate>>,
    started: Instant,
    first_paint: Option<Duration>,
    last_wheel: Option<Instant>,
    wheel_streak: f32,
}

impl SpikeApp {
    fn new(started: Instant, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(SpikeDelegate::new(), window, cx));

        // Redraw the header (row counter) whenever the table's data changes.
        cx.observe(&table, |_, _, cx| cx.notify()).detach();

        // ---- The tokio ↔ GPUI bridge under test ----------------------------
        //
        // Producer: a real tokio multi-thread runtime on its own OS threads,
        // emitting row batches with simulated network latency — the seat the
        // AWS SDK will occupy in the real app.
        //
        // Consumer: a task on GPUI's executor, awaiting the channel and
        // applying each batch to the table entity. The render thread never
        // waits on the producer.
        let (tx, rx) = flume::unbounded::<Vec<ObjectRow>>();

        std::thread::Builder::new()
            .name("tokio-feed".into())
            .spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
                rt.block_on(async move {
                    let mut ix = 0;
                    while ix < TOTAL_ROWS {
                        tokio::time::sleep(BATCH_DELAY).await;
                        let batch: Vec<ObjectRow> = (ix..(ix + BATCH_SIZE).min(TOTAL_ROWS))
                            .map(synth_row)
                            .collect();
                        ix += batch.len();
                        if tx.send_async(batch).await.is_err() {
                            break; // UI is gone; stop producing.
                        }
                    }
                });
            })
            .expect("failed to spawn tokio feed thread");

        let table_for_feed = table.clone();
        cx.spawn(async move |_this, cx| {
            // Ends when the producer finishes and the channel drains; on app
            // shutdown the executor drops this task with the app.
            while let Ok(batch) = rx.recv_async().await {
                cx.update(|cx| {
                    table_for_feed.update(cx, |state, cx| {
                        state.delegate_mut().rows.extend(batch);
                        cx.notify();
                    });
                });
            }
        })
        .detach();
        // --------------------------------------------------------------------

        Self {
            table,
            started,
            first_paint: None,
            last_wheel: None,
            wheel_streak: 0.0,
        }
    }
}

impl SpikeApp {
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
    fn wheel_accelerator(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let table = self.table.clone();
        let this = cx.entity();

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
                            this.update(cx, |this, _| {
                                let now = Instant::now();
                                let rapid = this
                                    .last_wheel
                                    .is_some_and(|prev| now - prev < WHEEL_ACCEL_WINDOW);
                                this.last_wheel = Some(now);
                                this.wheel_streak =
                                    if rapid { this.wheel_streak + 1.0 } else { 0.0 };
                                (WHEEL_BASE_MULTIPLIER + this.wheel_streak * WHEEL_ACCEL_STEP)
                                    .min(WHEEL_MAX_MULTIPLIER)
                            })
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
}

impl Render for SpikeApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.first_paint.is_none() {
            self.first_paint = Some(self.started.elapsed());
        }
        let loaded = self.table.read(cx).delegate().rows.len();
        let status: SharedString = format!(
            "{loaded} / {TOTAL_ROWS} rows · first paint {} ms",
            self.first_paint.unwrap_or_default().as_millis()
        )
        .into();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                // Draws the draggable title bar with native-feeling window
                // controls; pairs with `TitleBar::window_options()` in main().
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_2()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .child("caithung — M0 spike"),
                        )
                        .child(div().text_color(cx.theme().muted_foreground).child(status)),
                ),
            )
            .child(
                // `relative()` so the FPS HUD pins to this container's top
                // right, below the title bar, instead of covering it.
                div()
                    .relative()
                    .min_h_0()
                    .flex_1()
                    .px_3()
                    .pb_3()
                    .child(DataTable::new(&self.table))
                    .child(self.wheel_accelerator(cx))
                    .child(fps_monitor(window, cx)),
            )
    }
}

fn main() {
    let started = Instant::now();
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);

            cx.spawn(async move |cx| {
                let options = cx.update(|cx| WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1280.), px(800.)),
                        cx,
                    ))),
                    ..TitleBar::window_options()
                });

                cx.open_window(options, |window, cx| {
                    window.set_window_title("caithung — M0 spike");
                    let view = cx.new(|cx| SpikeApp::new(started, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("failed to open window");
            })
            .detach();
        });
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
