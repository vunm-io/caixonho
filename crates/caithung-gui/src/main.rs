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
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, WindowOptions, div, px,
};
use gpui_component::{
    ActiveTheme, Root, h_flex,
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

struct SpikeApp {
    table: Entity<TableState<SpikeDelegate>>,
    started: Instant,
    first_paint: Option<Duration>,
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
        }
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
                h_flex()
                    .w_full()
                    .px_3()
                    .py_2()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("caithung — M0 spike"),
                    )
                    .child(div().text_color(cx.theme().muted_foreground).child(status)),
            )
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .px_3()
                    .pb_3()
                    .child(DataTable::new(&self.table)),
            )
            .child(fps_monitor(window, cx))
    }
}

fn main() {
    let started = Instant::now();
    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| SpikeApp::new(started, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}
