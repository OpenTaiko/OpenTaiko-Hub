// Long operations: cancelling them, and not losing them to a closed window.
//
// The page names an operation (an install, a build, a bulk download) with an id of its
// choosing and passes it to every command that operation runs. `cancel_job` flags the id;
// downloads, extraction and builds check the flag as they go and stop with CANCELLED,
// which the page recognises and reports as a cancellation rather than a failure.
//
// Closing the window while an operation runs asks first. The question is asked here and
// not by the page: a close handler in the page would leave the window impossible to close
// whenever the page itself has stopped responding.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Error text of a cancelled operation. The page matches it exactly.
pub const CANCELLED: &str = "Cancelled";

/// How long "Close anyway" waits for work that must not be cut off (see `InFlight`).
const CLOSE_WAIT: Duration = Duration::from_secs(60);

/// Sent to the page when the window waits for that work before closing.
const CLOSING_EVENT: &str = "hub-closing";

/// The close question, in the page's language. Sent with every `begin_job`.
#[derive(Deserialize, Clone)]
pub struct ClosePrompt {
    title: String,
    message: String,
    close: String,
    keep: String,
}

struct Entry {
    token: CancelToken,
    /// False once the page ended the operation. The entry is kept, so a command or a
    /// cancel arriving late cannot bring the operation back and make every later close
    /// ask about it.
    active: bool,
}

impl Entry {
    fn active() -> Self {
        Self { token: CancelToken::default(), active: true }
    }
}

#[derive(Default)]
pub struct Jobs {
    entries: Mutex<HashMap<String, Entry>>,
    prompt: Mutex<Option<ClosePrompt>>,
    asking: AtomicBool,
    /// Set once the window is to close as soon as `in_flight` work is done.
    closing: AtomicBool,
    /// Work under way that is waited for rather than cut off (see `InFlight`).
    in_flight: AtomicUsize,
}

impl Jobs {
    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        // Nothing panics while holding it, and a poisoned lock must not take down the
        // close handler, which runs on the main thread
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Cancels every running operation and marks it over.
    fn cancel_all(&self) {
        for entry in self.entries().values_mut().filter(|e| e.active) {
            entry.token.cancel();
            entry.active = false;
        }
    }

    /// The token of operation `id`, registering it as running if this is the first word
    /// of it. An operation that has ended keeps its entry, and its token.
    fn token(&self, id: &str) -> CancelToken {
        self.entries().entry(id.to_string()).or_insert_with(Entry::active).token.clone()
    }

    fn begin(&self, id: &str) {
        self.entries().entry(id.to_string()).or_insert_with(Entry::active);
    }

    /// Cancels operation `id`, unless it has already ended: a late cancel must not touch
    /// a token an ended operation's commands may still hold.
    fn cancel(&self, id: &str) {
        let mut entries = self.entries();
        let entry = entries.entry(id.to_string()).or_insert_with(Entry::active);
        if entry.active {
            entry.token.cancel();
        }
    }

    fn end(&self, id: &str) {
        if let Some(entry) = self.entries().get_mut(id) {
            entry.active = false;
        }
    }

    /// What a close request does now.
    fn on_close(&self) -> CloseDecision {
        if self.closing.load(Ordering::SeqCst) {
            return CloseDecision::Hold;
        }
        if self.entries().values().any(|e| e.active) {
            return CloseDecision::Ask;
        }
        if self.in_flight.load(Ordering::SeqCst) == 0 {
            CloseDecision::Close
        } else {
            CloseDecision::CloseWhenSettled
        }
    }
}

/// How a close request is answered.
#[derive(Debug, PartialEq)]
enum CloseDecision {
    /// Nothing is running.
    Close,
    /// Already closing: the window goes once the work under way is done. Letting this
    /// close through, as a second click on the button did, cut an install off midway.
    Hold,
    /// Nothing to ask about, but files are still being moved.
    CloseWhenSettled,
    /// An operation is running: the user is asked first.
    Ask,
}

#[derive(Default)]
struct Flag {
    cancelled: AtomicBool,
    changed: tokio::sync::Notify,
}

/// A flag a command polls, or waits on. Commands called without a job id get one nobody
/// can set.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Flag>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
        self.0.changed.notify_waiters();
    }

    /// Completes once cancelled, so a wait (a server that stopped answering, a delay
    /// before a retry) can end the moment the user cancels.
    pub async fn cancelled(&self) {
        loop {
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            // Registered before the flag is read, so a cancel in between is not missed
            changed.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            changed.await;
        }
    }

    /// `Err(CANCELLED)` once cancelled, for use with `?`.
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(CANCELLED.to_string())
        } else {
            Ok(())
        }
    }
}

/// The token of `job`. The page registers an operation without waiting for the answer,
/// so its first command can arrive before `begin_job` and registers it here instead.
pub fn token<R: Runtime>(app: &AppHandle<R>, job: &Option<String>) -> CancelToken {
    let Some(id) = job else { return CancelToken::default() };
    app.state::<Jobs>().token(id)
}

/// Work that closing the window waits for instead of cutting off: a build process still
/// to be stopped, or files being moved, which a closing Hub would leave half old and half
/// new. Counted while the guard lives.
pub struct InFlight<R: Runtime>(AppHandle<R>);

pub fn in_flight<R: Runtime>(app: &AppHandle<R>) -> InFlight<R> {
    app.state::<Jobs>().in_flight.fetch_add(1, Ordering::SeqCst);
    InFlight(app.clone())
}

impl<R: Runtime> Drop for InFlight<R> {
    fn drop(&mut self) {
        self.0.state::<Jobs>().in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Registers an operation as running, so closing the window asks first.
#[tauri::command]
pub fn begin_job<R: Runtime>(app: AppHandle<R>, job: String, prompt: ClosePrompt) {
    let jobs = app.state::<Jobs>();
    jobs.begin(&job);
    *jobs.prompt.lock().unwrap_or_else(|e| e.into_inner()) = Some(prompt);
}

#[tauri::command]
pub fn cancel_job<R: Runtime>(app: AppHandle<R>, job: String) {
    app.state::<Jobs>().cancel(&job);
}

/// Called by the page once an operation is over.
#[tauri::command]
pub fn end_job<R: Runtime>(app: AppHandle<R>, job: String) {
    app.state::<Jobs>().end(&job);
}

/// Called by a page that has just loaded: operations an earlier page started have nobody
/// left to show them or end them, so they are cancelled rather than kept running unseen,
/// and closing stops asking about them.
#[tauri::command]
pub fn reset_jobs<R: Runtime>(app: AppHandle<R>) {
    app.state::<Jobs>().cancel_all();
}

/// Handles a close request: returns true when the window may close now. Otherwise the
/// close is held, and the user is asked whether to stop what is running and close.
pub fn allow_close<R: Runtime>(window: &Window<R>) -> bool {
    let jobs = window.state::<Jobs>();
    match jobs.on_close() {
        CloseDecision::Close => return true,
        CloseDecision::Hold => return false,
        CloseDecision::CloseWhenSettled => {
            close_when_settled(window.clone());
            return false;
        }
        CloseDecision::Ask => {}
    }
    // A second click on the close button while the question is open changes nothing
    if jobs.asking.swap(true, Ordering::SeqCst) {
        return false;
    }
    let prompt = jobs.prompt.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // A job whose first command arrived before its registration has no question yet.
    // Closing went through at once then, cutting off whatever it was moving.
    let Some(prompt) = prompt else {
        jobs.asking.store(false, Ordering::SeqCst);
        close_when_settled(window.clone());
        return false;
    };
    let window = window.clone();
    window
        .dialog()
        .message(prompt.message)
        .title(prompt.title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(prompt.close, prompt.keep))
        .parent(&window)
        .show(move |close| {
            window.state::<Jobs>().asking.store(false, Ordering::SeqCst);
            if close {
                close_when_settled(window);
            }
        });
    false
}

/// Closing used to destroy the window with everything still running: a build carried on
/// without the Hub, and an install was cut off midway. Everything is cancelled, and the
/// window closes once a build has been stopped and no files are being moved.
fn close_when_settled<R: Runtime>(window: Window<R>) {
    if window.state::<Jobs>().closing.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let jobs = window.state::<Jobs>();
        jobs.cancel_all();
        // The window stays a moment longer: the page says why, rather than seeming to
        // ignore the close
        if jobs.in_flight.load(Ordering::SeqCst) > 0 {
            let _ = window.emit(CLOSING_EVENT, ());
        }
        let until = Instant::now() + CLOSE_WAIT;
        while jobs.in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = window.destroy();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_without_a_job_is_never_cancelled() {
        let token = CancelToken::default();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());
    }

    #[test]
    fn a_set_flag_reads_as_cancelled() {
        let token = CancelToken::default();
        token.cancel();
        assert_eq!(token.check(), Err(CANCELLED.to_string()));
    }

    #[test]
    fn a_wait_ends_when_cancelled_even_from_another_thread() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let token = CancelToken::default();
        let other = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            other.cancel();
        });
        let ended = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), token.cancelled()).await.is_ok()
        });
        assert!(ended);
        // Already cancelled: returns at once
        runtime.block_on(token.cancelled());
    }

    #[test]
    fn an_ended_job_stays_ended() {
        let jobs = Jobs::default();
        jobs.begin("a");
        let token = jobs.token("a");
        jobs.end("a");
        assert_eq!(jobs.on_close(), CloseDecision::Close);
        // A late command finds the ended entry instead of starting the job again, and a
        // late cancel leaves its token alone
        let late = jobs.token("a");
        jobs.cancel("a");
        assert_eq!(jobs.on_close(), CloseDecision::Close);
        assert!(!token.is_cancelled() && !late.is_cancelled());
    }

    #[test]
    fn a_command_arriving_before_its_job_registers_it() {
        let jobs = Jobs::default();
        let token = jobs.token("a");
        assert_eq!(jobs.on_close(), CloseDecision::Ask);
        jobs.begin("a");
        jobs.cancel("a");
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancelling_everything_stops_and_ends_each_job() {
        let jobs = Jobs::default();
        jobs.begin("a");
        jobs.begin("b");
        let token = jobs.token("a");
        jobs.cancel_all();
        assert!(token.is_cancelled());
        assert_eq!(jobs.on_close(), CloseDecision::Close);
    }

    // "Close anyway" cancels every job, so a second click on the close button found
    // nothing running and closed the window while an install was still moving files
    #[test]
    fn a_close_under_way_holds_every_later_one() {
        let jobs = Jobs::default();
        jobs.begin("install");
        assert_eq!(jobs.on_close(), CloseDecision::Ask);
        jobs.in_flight.fetch_add(1, Ordering::SeqCst);
        jobs.closing.store(true, Ordering::SeqCst);
        jobs.cancel_all();
        assert_eq!(jobs.on_close(), CloseDecision::Hold);

        // Files still moving with no job left to ask about: closed once they are in place
        let jobs = Jobs::default();
        jobs.in_flight.fetch_add(1, Ordering::SeqCst);
        assert_eq!(jobs.on_close(), CloseDecision::CloseWhenSettled);
        jobs.in_flight.fetch_sub(1, Ordering::SeqCst);
        assert_eq!(jobs.on_close(), CloseDecision::Close);
    }
}
