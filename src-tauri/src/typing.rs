//! Putting the dictated text into the application that has keyboard focus:
//! live word-by-word typing, a one-shot type, or a clipboard paste.

use crate::errors::{AppError, AppResult};
use crate::settings::Settings;
use crate::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[cfg(windows)]
fn modifiers_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    // SAFETY: GetAsyncKeyState has no preconditions.
    unsafe {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|k| (GetAsyncKeyState(k.0 as i32) as u16 & 0x8000) != 0)
    }
}

#[cfg(not(windows))]
fn modifiers_down() -> bool {
    false
}

/// Waits out the hotkey's modifiers (still held right as recording stops;
/// typing now would risk triggering shortcuts instead of literal characters),
/// then returns a fresh keyboard handle. Only for a one-shot action right
/// after a session ends — never call this per live partial, since in
/// hold-to-talk mode the modifier is legitimately held for the whole session.
fn plain_keyboard() -> AppResult<enigo::Enigo> {
    use enigo::{Enigo, Settings};
    Enigo::new(&Settings::default())
        .map_err(|e| AppError::Other(format!("keyboard input unavailable: {e}")))
}

fn keyboard_settled() -> AppResult<enigo::Enigo> {
    let wait = Instant::now();
    while modifiers_down() && wait.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(30));
    }
    std::thread::sleep(Duration::from_millis(60));
    plain_keyboard()
}

fn backspace(enigo: &mut enigo::Enigo, n: usize) -> AppResult<()> {
    use enigo::{Direction, Key, Keyboard};
    for _ in 0..n {
        enigo
            .key(Key::Backspace, Direction::Click)
            .map_err(|e| AppError::Other(format!("backspace failed: {e}")))?;
    }
    Ok(())
}

fn common_prefix_len(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Types text as real key events (scancodes) on Windows.
///
/// `KEYEVENTF_UNICODE` injection — what `enigo.text` sends — is unreliable in
/// classic Win32 edit controls: measured against Notepad it silently drops and
/// duplicates characters ("... take    nn entire aaar      a nnngle job." for
/// text that decodes perfectly). Real scancode events take the same path as a
/// physical keyboard and came through byte-exact in every target tried
/// (Notepad, Chromium/WebView2), including runs of the same character and
/// multiple spaces. Only characters the active layout cannot produce (emoji, or
/// Arabic typed while an English layout is active) fall back to the Unicode
/// path, one character at a time.
#[cfg(windows)]
mod key_events {
    use crate::errors::{AppError, AppResult};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MapVirtualKeyExW, SendInput, VkKeyScanExW, HKL, INPUT, INPUT_0,
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
        VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// Scancode of the left shift key on every layout.
    const SHIFT_SCAN: u16 = 0x2A;

    /// Layout of the window being typed into, so its own key map decides which
    /// physical key produces each character.
    pub fn target_layout() -> HKL {
        // SAFETY: both calls are plain queries and tolerate a null window by
        // returning this thread's id.
        unsafe { GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None)) }
    }

    fn key_input(scan: u16, up: bool) -> INPUT {
        let flags = if up {
            KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
        } else {
            KEYEVENTF_SCANCODE
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) -> AppResult<()> {
        // SAFETY: `inputs` is a well-formed array of KEYBDINPUT events and
        // `cbsize` is the real size of INPUT.
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            return Err(AppError::Other(format!(
                "keyboard input rejected: {sent} of {} events",
                inputs.len()
            )));
        }
        Ok(())
    }

    /// Scancode and shift requirement for `ch` on `hkl`, or `None` when the
    /// layout cannot produce it as a plain character.
    fn scan_for(ch: char, hkl: HKL) -> Option<(u16, bool)> {
        let mut buf = [0u16; 2];
        let code = *ch.encode_utf16(&mut buf).first()?;
        // SAFETY: pure layout lookup.
        let vks = unsafe { VkKeyScanExW(code, hkl) };
        if vks == -1 {
            return None;
        }
        let state = (vks >> 8) as u8;
        // Anything beyond shift (ctrl/alt combos) is not plain typing.
        if state & 0xFE != 0 {
            return None;
        }
        // SAFETY: pure layout lookup.
        let scan = unsafe { MapVirtualKeyExW((vks & 0xFF) as u32, MAPVK_VK_TO_VSC_EX, Some(hkl)) };
        if scan == 0 {
            return None;
        }
        Some((scan as u16, state & 1 == 1))
    }

    /// Presses and releases the keys that type `ch`, falling back to the Unicode
    /// path for characters the layout cannot produce.
    pub fn type_char(ch: char, fallback: &mut enigo::Enigo) -> AppResult<()> {
        match scan_for(ch, target_layout()) {
            Some((scan, shift)) => {
                let mut batch = Vec::with_capacity(4);
                if shift {
                    batch.push(key_input(SHIFT_SCAN, false));
                }
                batch.push(key_input(scan, false));
                batch.push(key_input(scan, true));
                if shift {
                    batch.push(key_input(SHIFT_SCAN, true));
                }
                send(&batch)
            }
            None => {
                use enigo::Keyboard;
                fallback
                    .text(&ch.to_string())
                    .map_err(|e| AppError::Other(format!("typing failed: {e}")))
            }
        }
    }
}

#[cfg(windows)]
fn type_text(enigo: &mut enigo::Enigo, text: &str) -> AppResult<()> {
    use enigo::{Direction, Key, Keyboard};
    for ch in text.chars() {
        match ch {
            '\n' => enigo
                .key(Key::Return, Direction::Click)
                .map_err(|e| AppError::Other(format!("typing failed: {e}")))?,
            '\t' => enigo
                .key(Key::Tab, Direction::Click)
                .map_err(|e| AppError::Other(format!("typing failed: {e}")))?,
            '\r' => continue,
            _ => {
                key_events::type_char(ch, enigo)?;
                // 16 ms, measured: at 4-8 ms the receiving app silently drops
                // characters (and `SendInput` still reports every event
                // accepted), at 16 ms and above the text arrives byte-exact.
                std::thread::sleep(Duration::from_millis(16));
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn type_text(enigo: &mut enigo::Enigo, text: &str) -> AppResult<()> {
    use enigo::Keyboard;
    enigo
        .text(text)
        .map_err(|e| AppError::Other(format!("typing failed: {e}")))
}

/// Backspaces the non-common suffix of `prev` and types the non-common suffix
/// of `next`. No-op when equal.
fn reconcile(mut enigo: enigo::Enigo, prev: &str, next: &str) -> AppResult<()> {
    let (p, n): (Vec<char>, Vec<char>) = (prev.chars().collect(), next.chars().collect());
    let common = common_prefix_len(&p, &n);
    backspace(&mut enigo, p.len() - common)?;
    let suffix: String = n[common..].iter().collect();
    if !suffix.is_empty() {
        type_text(&mut enigo, &suffix)?;
    }
    Ok(())
}

/// Reconciles on-screen text from `prev` to `next` without waiting for the
/// hotkey's modifiers to settle — used for live partials while the shortcut
/// may legitimately still be held (hold-to-talk).
fn apply_delta_raw(prev: &str, next: &str) -> AppResult<()> {
    if prev == next {
        return Ok(());
    }
    reconcile(plain_keyboard()?, prev, next)
}

/// Longest prefix of `text` that ends on a word boundary (the boundary
/// character included). Live typing only ever commits whole words: a word the
/// recognizer is still revising never reaches the screen, so it can neither be
/// shown wrong nor have to be backspaced again. Everything after the last
/// boundary is left to the next partial, or to `finish` when the session ends.
fn committed_prefix(text: &str) -> &str {
    match text.char_indices().rev().find(|(_, c)| !is_word_char(*c)) {
        Some((i, c)) => &text[..i + c.len_utf8()],
        None => "",
    }
}

/// Characters that continue a word instead of ending it: alphanumerics plus the
/// apostrophe forms that appear inside words ("don't", "l'autre").
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '\u{2019}' | '\u{02bc}' | '\u{ff07}')
}

/// Same reconciliation, but waits for the hotkey's modifiers to settle first
/// — used once at finalize (session has ended), never per-partial.
fn apply_delta_settled(prev: &str, next: &str) -> AppResult<()> {
    if prev == next {
        return Ok(());
    }
    reconcile(keyboard_settled()?, prev, next)
}

/// Tracks what dictation has typed into the focused app so far, so partials
/// can be shown live and reconciled to the final (possibly corrected) result
/// once the session ends. Only used when `insert_method == "type"` — "paste"
/// stays a single atomic clipboard paste at the end, never fed through here.
/// Live typing is word-committed: only whole words reach the screen.
#[derive(Default)]
pub struct LiveTyper {
    current: Mutex<String>,
    target: Mutex<Option<String>>,
    busy: AtomicBool,
    /// Set when an injection failed, so the on-screen text can no longer be
    /// trusted. Survives `reset` on purpose: the garbage is still there.
    desynced: AtomicBool,
}

impl LiveTyper {
    /// Text currently on screen, best-effort (may lag briefly behind an
    /// in-flight typing operation).
    pub fn snapshot(&self) -> String {
        self.current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Clears tracked state without touching the target application. Call at
    /// the start of a new session so it doesn't inherit stale text.
    pub fn reset(&self) {
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = String::new();
        *self.target.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Brings the on-screen text to `goal`. `settled` waits for the hotkey's
    /// modifiers first (finalize only, never a live partial).
    ///
    /// `current` is advanced only when the injection actually succeeded. A
    /// partial failure leaves the screen in an unknown state — advancing anyway
    /// would make every later diff compute backspaces against text that was
    /// never typed, which is how stray characters accumulate on screen.
    fn apply(&self, goal: &str, settled: bool) -> AppResult<()> {
        let res = if self.desynced.swap(false, Ordering::SeqCst) {
            // Untrustworthy screen: erase what was last confirmed to have
            // landed, then type the goal from scratch.
            let confirmed = self.snapshot().chars().count();
            self.retype(goal, confirmed, settled)
        } else {
            let prev = self.snapshot();
            if prev == goal {
                Ok(())
            } else if settled {
                apply_delta_settled(&prev, goal)
            } else {
                apply_delta_raw(&prev, goal)
            }
        };
        match &res {
            Ok(()) => *self.current.lock().unwrap_or_else(|p| p.into_inner()) = goal.to_string(),
            Err(_) => {
                self.desynced.store(true, Ordering::SeqCst);
            }
        }
        res
    }

    fn retype(&self, goal: &str, erase: usize, settled: bool) -> AppResult<()> {
        let mut enigo = if settled {
            keyboard_settled()?
        } else {
            plain_keyboard()?
        };
        if erase > 0 {
            backspace(&mut enigo, erase)?;
        }
        if goal.is_empty() {
            Ok(())
        } else {
            type_text(&mut enigo, goal)
        }
    }

    /// Queues `text` as the newest partial to type. If a worker is already
    /// draining the queue it picks this up (or a later value) instead of a
    /// second typing operation racing against it — at most one is ever in
    /// flight. A benign race can drop a partial that arrives just as the
    /// worker is about to go idle; harmless, since `finish`/`retract` always
    /// act on the real on-screen text (`current`), never on what is left in
    /// `target`.
    pub fn set_target(&self, app: AppHandle, text: String) {
        *self.target.lock().unwrap_or_else(|p| p.into_inner()) = Some(text);
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            let typer = &app.state::<AppState>().live_typer;
            while let Some(next) = typer
                .target
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take()
            {
                let goal = committed_prefix(&next).to_string();
                if let Err(e) = typer.apply(&goal, false) {
                    tracing::warn!(error = %e, "live-typing dictation partial failed");
                }
            }
            typer.busy.store(false, Ordering::SeqCst);
        });
    }

    /// Waits for any in-flight partial to land, then reconciles the on-screen
    /// text to `final_text` and clears tracked state. Handles both "no
    /// correction" (final ≈ current, a tiny diff) and "correction on"
    /// (backspaces the raw text, types the corrected one).
    pub fn finish(&self, final_text: &str) -> AppResult<()> {
        while self.busy.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let res = self.apply(final_text, true);
        self.reset();
        res
    }

    /// Erases whatever is currently typed (cancel/error path).
    pub fn retract(&self) -> AppResult<()> {
        while self.busy.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let confirmed = self.snapshot().chars().count();
        self.desynced.store(false, Ordering::SeqCst);
        let res = if confirmed == 0 {
            Ok(())
        } else {
            backspace(&mut keyboard_settled()?, confirmed)
        };
        self.reset();
        res
    }
}

/// Types text into the focused application. Blocking.
pub fn insert_text(text: &str, settings: &Settings) -> AppResult<()> {
    use enigo::{Direction, Key, Keyboard};
    if text.is_empty() {
        return Ok(());
    }
    let mut enigo = keyboard_settled()?;
    if settings.insert_method == "paste" {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| AppError::Other(format!("clipboard unavailable: {e}")))?;
        let previous = clipboard.get_text().ok();
        clipboard
            .set_text(text.to_string())
            .map_err(|e| AppError::Other(e.to_string()))?;
        std::thread::sleep(Duration::from_millis(40));
        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;
        let res = enigo
            .key(modifier, Direction::Press)
            .and_then(|_| enigo.key(Key::Unicode('v'), Direction::Click))
            .and_then(|_| enigo.key(modifier, Direction::Release));
        std::thread::sleep(Duration::from_millis(300));
        if let Some(prev) = previous {
            let _ = clipboard.set_text(prev);
        }
        res.map_err(|e| AppError::Other(format!("paste failed: {e}")))
    } else {
        type_text(&mut enigo, text)
    }
}

pub fn finalize_text(text: &str, settings: &Settings) -> String {
    let t = text.trim();
    if t.is_empty() {
        String::new()
    } else if settings.add_trailing_space {
        format!("{t} ")
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize() {
        let s = Settings::default();
        assert_eq!(finalize_text("  hallo ", &s), "hallo ");
        assert_eq!(finalize_text("   ", &s), "");
    }

    #[test]
    fn live_typing_only_commits_whole_words() {
        assert_eq!(committed_prefix("hello"), "");
        assert_eq!(committed_prefix("hello wor"), "hello ");
        assert_eq!(committed_prefix("hello world foo"), "hello world ");
        assert_eq!(committed_prefix("no, i don't"), "no, i ");
        assert_eq!(committed_prefix("done."), "done.");
        assert_eq!(committed_prefix(""), "");
        assert_eq!(committed_prefix("it's"), "");
        assert_eq!(committed_prefix("it's fine"), "it's ");
        assert_eq!(committed_prefix("l\u{2019}autre chose"), "l\u{2019}autre ");
        assert_eq!(committed_prefix("مرحبا بالعالم"), "مرحبا ");
        assert_eq!(committed_prefix("hello 42"), "hello ");
        assert_eq!(committed_prefix("hello 42."), "hello 42.");
    }
}
