//! Is there a microphone to dictate into? Watches the chosen input device (or
//! the system default) and reports when it is unplugged or disabled, muted, or
//! blocked by Windows' privacy setting. The tray icon and the taskbar button
//! turn red while it is not usable (`lib::start_mic_watch`).

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MicStatus {
    /// Present, on and allowed.
    Ok,
    /// Switched off in Windows' sound settings (muted).
    Muted,
    /// Unplugged, disabled, or no input device at all.
    Missing,
    /// Windows' privacy setting denies microphone access to desktop apps.
    Blocked,
}

impl MicStatus {
    pub fn is_ok(self) -> bool {
        self == Self::Ok
    }

    /// One line for a tooltip: what is wrong and what to do about it.
    pub fn problem(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::Muted => Some("The microphone is muted. Unmute it in Windows Sound settings."),
            Self::Missing => {
                Some("No microphone found. Plug it in, or enable it in Windows Sound settings.")
            }
            Self::Blocked => Some(
                "Windows blocks microphone access. Allow it in Settings > Privacy > Microphone.",
            ),
        }
    }
}

/// Checks the device with this id (`None`, or an id that is gone: the system
/// default, which is what recording falls back to).
#[cfg(windows)]
pub fn check(device_id: Option<&str>) -> MicStatus {
    if privacy_blocked() {
        return MicStatus::Blocked;
    }
    match endpoint_muted(device_id) {
        Some(false) => MicStatus::Ok,
        Some(true) => MicStatus::Muted,
        None => MicStatus::Missing,
    }
}

#[cfg(not(windows))]
pub fn check(_device_id: Option<&str>) -> MicStatus {
    MicStatus::Ok
}

/// `Some(muted)` for an active capture endpoint, `None` when there is none.
#[cfg(windows)]
fn endpoint_muted(device_id: Option<&str>) -> Option<bool> {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    // SAFETY: plain COM calls on interfaces created and dropped here. COM is
    // initialised for the calling thread (a repeat call just reports so).
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let named: Option<IMMDevice> = device_id.and_then(|id| {
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            enumerator.GetDevice(PCWSTR(wide.as_ptr())).ok()
        });
        let active = |d: &IMMDevice| d.GetState().is_ok_and(|s| s == DEVICE_STATE_ACTIVE);
        let device = match named.filter(|d| active(d)) {
            Some(d) => d,
            None => enumerator
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .ok()
                .filter(|d| active(d))?,
        };
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        Some(volume.GetMute().ok()?.as_bool())
    }
}

/// Windows' "let apps access your microphone" switches (all apps, and desktop
/// apps), per machine and per user. `Deny` in any of them blocks recording.
#[cfg(windows)]
fn privacy_blocked() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };
    const BASE: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let value = wide("Value");
    let denied = |root: HKEY, sub: &str| -> bool {
        let sub = wide(sub);
        let mut buf = [0u16; 16];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the buffer and its byte length are passed together.
        let r = unsafe {
            RegGetValueW(
                root,
                PCWSTR(sub.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                Some(&mut len),
            )
        };
        r.is_ok() && String::from_utf16_lossy(&buf).trim_end_matches('\0') == "Deny"
    };
    let non_packaged = format!("{BASE}\\NonPackaged");
    [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .any(|root| denied(root, BASE) || denied(root, &non_packaged))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ok_is_usable_and_every_problem_has_a_fix() {
        assert!(MicStatus::Ok.is_ok());
        assert_eq!(MicStatus::Ok.problem(), None);
        for s in [MicStatus::Muted, MicStatus::Missing, MicStatus::Blocked] {
            assert!(!s.is_ok());
            assert!(s.problem().is_some_and(|p| p.len() > 20));
        }
    }
}

#[cfg(all(test, windows))]
mod live {
    #[test]
    #[ignore = "reads this PC's real microphone state"]
    fn prints_the_real_status() {
        println!("default: {:?}", super::check(None));
        println!("bogus id: {:?}", super::check(Some("{no-such-device}")));
    }
}
