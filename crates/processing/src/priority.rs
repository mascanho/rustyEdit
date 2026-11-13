//! Keeps long background jobs (exports) from getting in the way of the
//! photo being edited.
//!
//! Two things are done for a thread that calls [`enter_background`]:
//! - its operating-system priority is lowered, so the UI and the preview
//!   render win any contest for CPU time;
//! - its full-size GPU renders wait their turn: while a live preview or
//!   detail render is running (or finished a moment ago, as a slider drag
//!   keeps asking for more), the export holds off, so the editor's next
//!   frame never sits behind a multi-megapixel export pass in the GPU queue.
//!
//! The wait is bounded, so an export still makes progress while someone
//! drags sliders for minutes on end.

use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        OnceLock,
    },
    time::{Duration, Instant},
};

/// How long the editor has to have been quiet before background GPU work
/// goes ahead.
const QUIET: Duration = Duration::from_millis(250);
/// The longest a single background GPU job waits for the editor.
const MAX_WAIT: Duration = Duration::from_secs(4);

thread_local! {
    static BACKGROUND: Cell<bool> = const { Cell::new(false) };
}

/// What the editor has been doing on the GPU.
struct Gate {
    /// Interactive (preview / detail) renders in flight.
    busy: AtomicUsize,
    /// When the last one finished, in milliseconds since start-up (+1, so
    /// zero means "never").
    last_done_ms: AtomicU64,
}

impl Gate {
    const fn new() -> Self {
        Gate { busy: AtomicUsize::new(0), last_done_ms: AtomicU64::new(0) }
    }

    fn begin(&self) {
        self.busy.fetch_add(1, Ordering::AcqRel);
    }

    fn end(&self) {
        self.last_done_ms.store(now_ms(), Ordering::Release);
        self.busy.fetch_sub(1, Ordering::AcqRel);
    }

    /// Blocks until nothing interactive is running and nothing finished in
    /// the last `quiet`, or `max` is up.
    fn wait_until_quiet(&self, quiet: Duration, max: Duration) {
        let started = Instant::now();
        loop {
            if self.busy.load(Ordering::Acquire) == 0 {
                let last = self.last_done_ms.load(Ordering::Acquire);
                let since = if last == 0 {
                    Duration::MAX
                } else {
                    Duration::from_millis(now_ms().saturating_sub(last))
                };
                if since >= quiet {
                    return;
                }
            }
            if started.elapsed() >= max {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

static GATE: Gate = Gate::new();

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// Marks the calling thread as doing background work and lowers its
/// operating-system priority. Call it first thing on a worker thread; the
/// effect lasts as long as the thread does.
pub fn enter_background() {
    BACKGROUND.with(|b| b.set(true));
    lower_os_priority();
}

pub(crate) fn is_background() -> bool {
    BACKGROUND.with(Cell::get)
}

/// Held while an interactive render runs.
pub(crate) struct Interactive;

impl Interactive {
    pub(crate) fn begin() -> Self {
        GATE.begin();
        Interactive
    }
}

impl Drop for Interactive {
    fn drop(&mut self) {
        GATE.end();
    }
}

/// For background threads: blocks until the editor has been quiet for a
/// moment (or the longest wait is up). Returns at once for other threads.
pub(crate) fn yield_to_interactive() {
    if is_background() {
        GATE.wait_until_quiet(QUIET, MAX_WAIT);
    }
}

#[cfg(target_os = "macos")]
fn lower_os_priority() {
    // "Utility": below everything the user is looking at, but not throttled
    // to a crawl the way the background class is.
    // SAFETY: plain libc call on the current thread.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn lower_os_priority() {
    // On Linux a nice value applies per thread.
    // SAFETY: plain libc calls on the current thread.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 10);
    }
}

#[cfg(not(unix))]
fn lower_os_priority() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_gpu_work_waits_for_the_editor_but_never_forever() {
        static G: Gate = Gate::new();
        let quiet = Duration::from_millis(50);

        // Nothing has rendered: no wait.
        let t = Instant::now();
        G.wait_until_quiet(quiet, Duration::from_secs(2));
        assert!(t.elapsed() < Duration::from_millis(40));

        // A render in flight holds it up until it's done and quiet.
        G.begin();
        let release = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(120));
            G.end();
        });
        let t = Instant::now();
        G.wait_until_quiet(quiet, Duration::from_secs(2));
        assert!(t.elapsed() >= Duration::from_millis(150), "{:?}", t.elapsed());
        release.join().unwrap();

        // An editor that never stops still can't starve it past the limit.
        G.begin();
        let t = Instant::now();
        G.wait_until_quiet(quiet, Duration::from_millis(200));
        assert!(t.elapsed() >= Duration::from_millis(200));
        assert!(t.elapsed() < Duration::from_millis(400));
        G.end();
    }

    #[test]
    fn only_marked_threads_are_background() {
        assert!(!is_background());
        std::thread::spawn(|| {
            enter_background();
            assert!(is_background());
        })
        .join()
        .unwrap();
        assert!(!is_background());
    }
}
