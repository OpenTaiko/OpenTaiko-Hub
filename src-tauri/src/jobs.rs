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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Error text of a cancelled operation. The page matches it exactly.
pub const CANCELLED: &str = "Cancelled";

/// The close question, in the page's language. Sent with every `begin_job`.
#[derive(Deserialize, Clone)]
pub struct ClosePrompt {
    title: String,
    message: String,
    close: String,
    keep: String,
}

#[derive(Default)]
pub struct Jobs {
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    prompt: Mutex<Option<ClosePrompt>>,
    asking: AtomicBool,
}

/// A flag a command polls. Commands called without a job id get one nobody can set.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
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

/// The token of `job`, registering it on first use.
pub fn token<R: Runtime>(app: &AppHandle<R>, job: &Option<String>) -> CancelToken {
    let Some(id) = job else { return CancelToken::default() };
    let jobs = app.state::<Jobs>();
    let mut map = jobs.flags.lock().unwrap();
    CancelToken(map.entry(id.clone()).or_default().clone())
}

/// Registers an operation as running, so closing the window asks first.
#[tauri::command]
pub fn begin_job<R: Runtime>(app: AppHandle<R>, job: String, prompt: ClosePrompt) {
    let jobs = app.state::<Jobs>();
    jobs.flags.lock().unwrap().entry(job).or_default();
    *jobs.prompt.lock().unwrap() = Some(prompt);
}

#[tauri::command]
pub fn cancel_job<R: Runtime>(app: AppHandle<R>, job: String) {
    let jobs = app.state::<Jobs>();
    let mut map = jobs.flags.lock().unwrap();
    map.entry(job).or_default().store(true, Ordering::Relaxed);
}

/// Called by the page once an operation is over, so its id does not linger.
#[tauri::command]
pub fn end_job<R: Runtime>(app: AppHandle<R>, job: String) {
    app.state::<Jobs>().flags.lock().unwrap().remove(&job);
}

/// Handles a close request: returns true when the window may close now. Otherwise the
/// close is held, and the user is asked whether to stop what is running and close.
pub fn allow_close<R: Runtime>(window: &Window<R>) -> bool {
    let jobs = window.state::<Jobs>();
    if jobs.flags.lock().unwrap().is_empty() {
        return true;
    }
    // A second click on the close button while the question is open changes nothing
    if jobs.asking.swap(true, Ordering::SeqCst) {
        return false;
    }
    let prompt = jobs.prompt.lock().unwrap().clone();
    let Some(prompt) = prompt else {
        jobs.asking.store(false, Ordering::SeqCst);
        return true;
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
                let _ = window.destroy();
            }
        });
    false
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
        token.0.store(true, Ordering::Relaxed);
        assert_eq!(token.check(), Err(CANCELLED.to_string()));
    }
}
