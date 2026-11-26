//! One dictation listening session: microphone -> voice-activity detector ->
//! Whisper. The detector splits speech into utterances; each finished one is
//! transcribed on a worker and shown at once, so the overlay updates while the
//! user is still talking.

use super::engine::Recognizer;
use super::SttService;
use crate::audio::capture::{Capture, CaptureEvent};
use crate::errors::{AppError, AppResult};
use crate::settings::SettingsStore;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VoiceEvent {
    /// "listening" | "transcribing" | "idle". `session` numbers the session, so
    /// a late "idle" of an old one cannot end the next one in the UI.
    State {
        session: u64,
        state: String,
    },
    /// `bands` is the voice spectrum (see `audio::spectrum`).
    Level {
        value: f32,
        bands: Vec<f32>,
    },
    /// Live text while speaking (not final).
    Partial {
        text: String,
    },
    Transcript {
        text: String,
        audio_ms: u64,
        elapsed_ms: u64,
    },
    Error {
        code: String,
        detail: String,
    },
}

pub type VoiceEmit = Arc<dyn Fn(VoiceEvent) + Send + Sync>;

struct Active {
    stop: Arc<AtomicBool>,
    discard: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

pub struct VoiceSessions {
    stt: Arc<SttService>,
    settings: Arc<SettingsStore>,
    vad_path: std::path::PathBuf,
    emit: VoiceEmit,
    active: Mutex<Option<Active>>,
    /// The previous session's thread, joined by the next session (never by the UI).
    previous: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Discard flag of that previous session, so its pending result can be dropped.
    previous_discard: Mutex<Option<Arc<AtomicBool>>>,
    sessions: AtomicU64,
}

const MAX_RECORDING: Duration = Duration::from_secs(300);
const VAD_WINDOW: usize = 512;
/// Level above which the microphone is considered to pick up speech.
const SPEECH_LEVEL: f32 = 0.22;

impl VoiceSessions {
    pub fn new(
        stt: Arc<SttService>,
        settings: Arc<SettingsStore>,
        vad_path: std::path::PathBuf,
        emit: VoiceEmit,
    ) -> Self {
        Self {
            stt,
            settings,
            vad_path,
            emit,
            active: Mutex::new(None),
            previous: Mutex::new(None),
            previous_discard: Mutex::new(None),
            sessions: AtomicU64::new(0),
        }
    }

    /// True while a session is recording or finishing up.
    pub fn is_active(&self) -> bool {
        let mut guard = self.active.lock().unwrap_or_else(|p| p.into_inner());
        if guard.as_ref().is_some_and(|a| a.thread.is_finished()) {
            *guard = None;
        }
        guard.is_some()
    }

    /// True while a stopped session is still transcribing its recording.
    pub fn is_transcribing(&self) -> bool {
        self.previous
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }

    /// Drops the result of the session that is still transcribing: its text is
    /// never emitted. The transcription itself runs to its end.
    pub fn cancel_transcription(&self) {
        if let Some(d) = self
            .previous_discard
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            d.store(true, Ordering::Relaxed);
        }
    }

    /// Starts listening; returns the new session's number.
    pub fn start(&self) -> AppResult<u64> {
        let settings = self.settings.get();
        if !self.stt.is_ready(&settings) {
            return Err(AppError::Stt(
                "no speech model is installed; open Settings and download Whisper tiny, base or small"
                    .into(),
            ));
        }
        // Stop any running session first (discarding its audio) and wait for it
        // here, before opening the microphone again.
        self.stop(true);
        if let Some(prev) = self
            .previous
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            let _ = prev.join();
        }

        let (capture, rx) = Capture::start(settings.microphone.as_deref())?;
        let stop = Arc::new(AtomicBool::new(false));
        let discard = Arc::new(AtomicBool::new(false));
        let session = self.sessions.fetch_add(1, Ordering::SeqCst) + 1;
        let ctx = SessionCtx {
            session,
            emit: self.emit.clone(),
            settings,
            stop: stop.clone(),
            discard: discard.clone(),
        };
        let (stt, vad_path, emit) = (self.stt.clone(), self.vad_path.clone(), self.emit.clone());

        emit(VoiceEvent::State {
            session,
            state: "listening".into(),
        });
        let thread = std::thread::Builder::new()
            .name("voice-session".into())
            .spawn(move || {
                let mut capture = capture;
                let started = Instant::now();
                run_session(&stt, &ctx, &rx, &vad_path);
                capture.stop();
                tracing::info!(
                    session,
                    secs = started.elapsed().as_secs_f32(),
                    stopped = ctx.stop.load(Ordering::Relaxed),
                    discarded = ctx.discard.load(Ordering::Relaxed),
                    "listening session ended"
                );
                emit(VoiceEvent::State {
                    session,
                    state: "idle".into(),
                });
            })
            .map_err(|e| AppError::Audio(e.to_string()))?;
        *self.active.lock().unwrap_or_else(|p| p.into_inner()) = Some(Active {
            stop,
            discard,
            thread,
        });
        Ok(session)
    }

    /// Stops the active session. With `discard`, recorded audio is dropped
    /// instead of transcribed. Never blocks: the session thread finishes on its
    /// own and results arrive as events.
    pub fn stop(&self, discard: bool) -> bool {
        let Some(active) = self.active.lock().unwrap_or_else(|p| p.into_inner()).take() else {
            return false;
        };
        active.discard.store(discard, Ordering::Relaxed);
        active.stop.store(true, Ordering::Relaxed);
        *self
            .previous_discard
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(active.discard.clone());
        *self.previous.lock().unwrap_or_else(|p| p.into_inner()) = Some(active.thread);
        true
    }
}

struct SessionCtx {
    session: u64,
    emit: VoiceEmit,
    settings: crate::settings::Settings,
    stop: Arc<AtomicBool>,
    discard: Arc<AtomicBool>,
}

fn run_session(
    stt: &SttService,
    ctx: &SessionCtx,
    rx: &Receiver<CaptureEvent>,
    vad_path: &std::path::Path,
) {
    let Some(vad) = create_vad(vad_path, &ctx.settings) else {
        tracing::warn!(path = %vad_path.display(), "voice activity detection model could not be loaded");
        ctx.error(
            "stt_unavailable",
            "the voice activity detection model could not be loaded",
        );
        return;
    };
    let Some(backlog) = ctx.warm_up(stt, rx) else {
        return;
    };
    let result = stt.with_recognizer(&ctx.settings, |rec| ctx.run(rec, rx, vad, backlog));
    if let Err(e) = result {
        tracing::warn!(error = %e, "speech session could not start");
        ctx.error(e.code(), &e.to_string());
    }
}

fn create_vad(
    path: &std::path::Path,
    settings: &crate::settings::Settings,
) -> Option<sherpa_onnx::VoiceActivityDetector> {
    let config = sherpa_onnx::VadModelConfig {
        silero_vad: sherpa_onnx::SileroVadModelConfig {
            model: Some(path.to_string_lossy().to_string()),
            threshold: settings.vad_threshold,
            min_silence_duration: settings.silence_ms as f32 / 1000.0,
            min_speech_duration: 0.25,
            window_size: VAD_WINDOW as i32,
            max_speech_duration: 20.0,
        },
        sample_rate: 16_000,
        num_threads: 1,
        // Always CPU: Silero is tiny and runs every 32 ms, so on CUDA each call
        // is mostly launch/sync overhead.
        provider: Some("cpu".into()),
        ..Default::default()
    };
    sherpa_onnx::VoiceActivityDetector::create(&config, 60.0)
}

/// Watches how long the microphone has heard nothing and ends the session
/// when the configured limit is reached.
struct SilenceGuard {
    last_speech: Instant,
    limit: Option<Duration>,
}

impl SilenceGuard {
    fn new(secs: u32) -> Self {
        Self {
            last_speech: Instant::now(),
            limit: (secs > 0).then(|| Duration::from_secs(secs as u64)),
        }
    }

    fn heard_speech(&mut self) {
        self.last_speech = Instant::now();
    }

    fn expired(&self) -> bool {
        self.limit
            .map(|l| self.last_speech.elapsed() > l)
            .unwrap_or(false)
    }
}

impl SessionCtx {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn error(&self, code: &str, detail: &str) {
        (self.emit)(VoiceEvent::Error {
            code: code.into(),
            detail: detail.into(),
        });
    }

    fn emit_state(&self, state: &str) {
        (self.emit)(VoiceEvent::State {
            session: self.session,
            state: state.into(),
        });
    }

    fn emit_partial(&self, text: &str) {
        (self.emit)(VoiceEvent::Partial {
            text: super::clean_transcript(text),
        });
    }

    fn emit_level(&self, value: f32, bands: Vec<f32>) {
        (self.emit)(VoiceEvent::Level { value, bands });
    }

    /// Loads the speech model on a helper thread while this one keeps the
    /// level meter moving and collects the audio heard meanwhile (a cold
    /// load takes seconds). Returns that audio, or `None` when the session
    /// cannot go on.
    fn warm_up(&self, stt: &SttService, rx: &Receiver<CaptureEvent>) -> Option<Vec<f32>> {
        let settings = &self.settings;
        std::thread::scope(|scope| {
            let loading = scope.spawn(move || stt.with_recognizer(settings, |_| ()));
            let mut backlog = Vec::new();
            let mut failed = false;
            while !loading.is_finished() {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(CaptureEvent::Level(v, bands)) => self.emit_level(v, bands),
                    Ok(CaptureEvent::Samples(s)) => backlog.extend_from_slice(&s),
                    Ok(CaptureEvent::Error(e)) => {
                        tracing::warn!(error = %e, "capture failed during warm-up");
                        self.error("audio", &e);
                        failed = true;
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            match loading.join() {
                Ok(Ok(())) if !failed => Some(backlog),
                Ok(Ok(())) => None,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "speech session could not start");
                    self.error(e.code(), &e.to_string());
                    None
                }
                Err(_) => {
                    self.error("stt_unavailable", "the speech model could not be loaded");
                    None
                }
            }
        })
    }

    /// `backlog` is the audio heard while the model was loading; it is
    /// processed first, even when the user already stopped.
    fn run(
        &self,
        rec: &Recognizer,
        rx: &Receiver<CaptureEvent>,
        vad: sherpa_onnx::VoiceActivityDetector,
        backlog: Vec<f32>,
    ) {
        tracing::info!("listening session started");
        let mut backlog = (!backlog.is_empty()).then_some(CaptureEvent::Samples(backlog));
        let started = Instant::now();
        let mut buffer: Vec<f32> = Vec::new(); // audio not yet covered by a finished utterance
        let mut pending: Vec<f32> = Vec::new(); // the VAD needs fixed-size windows
        let mut committed = String::new();
        let mut samples_seen = 0u64;
        let mut silence = SilenceGuard::new(self.settings.auto_stop_silence_secs);

        std::thread::scope(|scope| {
            // Finished utterances are decoded on a worker so this loop keeps
            // the level meter and the VAD running meanwhile.
            let (seg_tx, seg_rx) = std::sync::mpsc::channel::<Vec<f32>>();
            let (text_tx, text_rx) = std::sync::mpsc::channel::<String>();
            scope.spawn(move || {
                for segment in seg_rx {
                    if text_tx
                        .send(super::clean_transcript(&rec.transcribe(&segment)))
                        .is_err()
                    {
                        break;
                    }
                }
            });

            while backlog.is_some()
                || (!self.stopped() && started.elapsed() < MAX_RECORDING && !silence.expired())
            {
                let before = committed.len();
                for text in text_rx.try_iter().filter(|t| !t.is_empty()) {
                    silence.heard_speech();
                    append(&mut committed, &text);
                }
                if committed.len() != before {
                    self.emit_partial(&committed);
                }
                let event = match backlog.take() {
                    Some(e) => Ok(e),
                    None => rx.recv_timeout(Duration::from_millis(100)),
                };
                match event {
                    Ok(CaptureEvent::Level(v, bands)) => {
                        if v > SPEECH_LEVEL {
                            silence.heard_speech();
                        }
                        self.emit_level(v, bands);
                    }
                    Ok(CaptureEvent::Samples(s)) => {
                        samples_seen += s.len() as u64;
                        buffer.extend_from_slice(&s);
                        pending.extend_from_slice(&s);
                        let mut offset = 0;
                        while pending.len() - offset >= VAD_WINDOW {
                            vad.accept_waveform(&pending[offset..offset + VAD_WINDOW]);
                            offset += VAD_WINDOW;
                        }
                        pending.drain(..offset);
                        while !vad.is_empty() {
                            let segment = vad.front().map(|s| s.samples().to_vec());
                            vad.pop();
                            let Some(segment) = segment else { continue };
                            if (segment.len() as u64) * 1000 / 16_000 < super::MIN_AUDIO_MS {
                                continue;
                            }
                            // Utterance finished: drop its audio from the buffer so
                            // it is not transcribed twice; its text shows up live
                            // once the worker has it.
                            let consumed = (segment.len() + 16_000 / 2).min(buffer.len());
                            buffer.drain(..consumed);
                            let _ = seg_tx.send(segment);
                        }
                    }
                    Ok(CaptureEvent::Error(e)) => {
                        tracing::warn!(error = %e, "capture stream failed");
                        self.error("audio", &e);
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }

            while let Ok(CaptureEvent::Samples(s)) = rx.try_recv() {
                samples_seen += s.len() as u64;
                buffer.extend_from_slice(&s);
            }
            // No more utterances: the worker stops once the queued ones are done.
            drop(seg_tx);
            if self.discard.load(Ordering::Relaxed) {
                return;
            }
            self.emit_state("transcribing");
            let t0 = Instant::now();
            for text in text_rx.into_iter().filter(|t| !t.is_empty()) {
                append(&mut committed, &text);
            }
            if buffer.len() as u64 * 1000 / 16_000 >= super::MIN_AUDIO_MS {
                append(
                    &mut committed,
                    &super::clean_transcript(&rec.transcribe(&buffer)),
                );
            }
            // Cancelled while transcribing (new dictation started): drop the text.
            if self.discard.load(Ordering::Relaxed) {
                return;
            }
            (self.emit)(VoiceEvent::Transcript {
                text: committed.clone(),
                audio_ms: samples_seen * 1000 / 16_000,
                elapsed_ms: t0.elapsed().as_millis() as u64,
            });
        });
    }
}

/// Adds an utterance to the text dictated so far.
fn append(text: &mut String, utterance: &str) {
    if utterance.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(utterance);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utterances_join_with_single_spaces() {
        let mut t = String::new();
        append(&mut t, "");
        append(&mut t, "hello");
        append(&mut t, "world");
        assert_eq!(t, "hello world");
    }

    #[test]
    fn silence_guard_zero_means_never() {
        assert!(!SilenceGuard::new(0).expired());
        let mut g = SilenceGuard::new(1);
        g.heard_speech();
        assert!(!g.expired());
    }
}
