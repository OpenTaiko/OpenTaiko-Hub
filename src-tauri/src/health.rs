// Liveness watch for the webview.
//
// The page runs in its own process (WebKitWebProcess on Linux, the WebView2 renderer on
// Windows). When that process dies — descriptor exhaustion, an out-of-memory kill, a
// GStreamer abort — the window and this process both survive and only the page is gone.
// The user sees a permanently blank window, calls it a crash, and there is nothing in the
// report we can act on: no panic, no exit code, no stack.
//
// WebKitGTK does emit `web-process-terminated`, but wry only exposes that handler on
// macOS and iOS; the Linux path has no hook, so catching it would mean binding to
// webkit2gtk directly. That cannot be compiled or tested from a Windows host, and getting
// it wrong breaks the Linux release build outright. Instead the frontend reports in on a
// timer and this module notices when it stops — which is portable, testable, and catches
// a dead renderer whatever killed it.
//
// It only records the fact. Reloading automatically would risk throwing away a user's
// in-progress download or migration whenever a slow operation merely delayed a beat.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How long the UI may stay silent before it is worth reporting.
///
/// A dead renderer never comes back, so reporting it late costs only a late line in the
/// log. A live one that is merely starved — compiling OpenTaiko pins every core for half
/// an hour — produces a false report, which is worse: it sends whoever reads the log
/// hunting a crash that never happened. 40 s was not enough and did exactly that, so this
/// is set well past any stall a build has been seen to cause.
const SILENCE_LIMIT: Duration = Duration::from_secs(90);

/// How often the watcher looks.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct UiHealth {
    started: Instant,
    /// Milliseconds since `started` at the last beat.
    last_beat_ms: AtomicU64,
    /// Whether the UI has ever reported, which separates "never loaded" from "died".
    ever_seen: AtomicBool,
    /// Latches so one silence is logged once rather than on every poll.
    reported: AtomicBool,
}

impl UiHealth {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            last_beat_ms: AtomicU64::new(0),
            ever_seen: AtomicBool::new(false),
            reported: AtomicBool::new(false),
        }
    }

    /// Milliseconds since the watch started. Taken as a parameter by the logic below so
    /// the silence rules can be tested without waiting on a real clock.
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Called by the frontend on a timer.
    pub fn beat(&self) {
        let now = self.now_ms();
        self.last_beat_ms.store(now, Ordering::Relaxed);
        if !self.ever_seen.swap(true, Ordering::Relaxed) {
            log::info!("UI is alive (first beat {} ms after start)", now);
        }
        // Recovered on its own: allow the next silence to be reported too.
        if self.reported.swap(false, Ordering::Relaxed) {
            log::warn!("UI resumed reporting after a silence");
        }
    }

    fn silence_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.last_beat_ms.load(Ordering::Relaxed))
    }

    /// One watcher pass. Returns true when it logged a new silence.
    fn check(&self) -> bool {
        self.check_at(self.now_ms())
    }

    fn check_at(&self, now_ms: u64) -> bool {
        let silence = self.silence_ms(now_ms);
        if silence < SILENCE_LIMIT.as_millis() as u64 {
            return false;
        }
        if self.reported.swap(true, Ordering::Relaxed) {
            return false; // already reported this silence
        }
        let seconds = silence / 1000;
        if self.ever_seen.load(Ordering::Relaxed) {
            log::warn!(
                "The UI has not reported for {} s while the app is still running. Either \
                 its process died (look for 'Too many open files', an out-of-memory kill, \
                 or a GStreamer abort above), in which case the window stays blank until \
                 it is reloaded, or the page is blocked on a long task. A later 'UI \
                 resumed reporting' line means it was blocked, not dead.",
                seconds
            );
        } else {
            log::error!(
                "The webview never reported in, {} s after start. The page failed to \
                 load or its process died during startup; the window will be blank.",
                seconds
            );
        }
        true
    }
}

impl Default for UiHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts the background watcher. The thread lives for the life of the process.
pub fn spawn_watcher(health: std::sync::Arc<UiHealth>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_INTERVAL);
        health.check();
    });
}

#[tauri::command]
pub fn ui_heartbeat(health: tauri::State<'_, std::sync::Arc<UiHealth>>) {
    health.beat();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_watch_is_quiet_and_knows_the_ui_has_not_reported() {
        let h = UiHealth::new();
        assert!(!h.check(), "must not report before the silence limit elapses");
        assert!(!h.ever_seen.load(Ordering::Relaxed));
    }

    #[test]
    fn a_beat_marks_the_ui_seen_and_resets_the_silence() {
        let h = UiHealth::new();
        h.beat();
        assert!(h.ever_seen.load(Ordering::Relaxed));
        assert!(h.silence_ms(h.now_ms()) < 1_000);
    }

    #[test]
    fn a_slow_operation_below_the_limit_is_not_reported() {
        let h = UiHealth::new();
        h.beat();
        let just_under = SILENCE_LIMIT.as_millis() as u64 - 1;
        assert!(!h.check_at(just_under), "a long but finite pause is not a dead page");
    }

    #[test]
    fn a_silence_is_reported_once_then_again_only_after_recovery() {
        let h = UiHealth::new();
        h.beat();
        let long_after = SILENCE_LIMIT.as_millis() as u64 + 5_000;

        assert!(h.check_at(long_after), "first silence must be reported");
        assert!(!h.check_at(long_after), "the same silence must not be reported twice");

        h.beat();
        assert!(!h.reported.load(Ordering::Relaxed), "a beat clears the latch");
        assert!(
            h.check_at(h.now_ms() + long_after),
            "a later silence must be reported again"
        );
    }

    #[test]
    fn a_page_that_never_loads_is_reported_as_such() {
        let h = UiHealth::new();
        assert!(!h.ever_seen.load(Ordering::Relaxed));
        assert!(
            h.check_at(SILENCE_LIMIT.as_millis() as u64 + 1),
            "a page that never reported must still be flagged"
        );
    }
}
