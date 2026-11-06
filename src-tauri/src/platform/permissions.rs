//! OS permission status and deep links to the settings panes.
use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug)]
pub struct PermissionStatus {
    pub microphone: bool,
    /// Needed to send the paste keystroke.
    pub accessibility: bool,
    /// Needed to observe the global hotkey (macOS "Input Monitoring").
    pub input_monitoring: bool,
}

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn AXIsProcessTrustedWithOptions(
            options: core_foundation::dictionary::CFDictionaryRef,
        ) -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // macOS 10.15+
        pub fn CGPreflightListenEventAccess() -> bool;
        pub fn CGRequestListenEventAccess() -> bool;
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicAuth {
    NotDetermined,
    Denied,
    Authorized,
}

/// Current AVCaptureDevice authorization for audio.
#[cfg(target_os = "macos")]
pub fn mic_auth() -> MicAuth {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    let Some(kind) = (unsafe { AVMediaTypeAudio }) else {
        return MicAuth::Denied;
    };
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(kind) };
    if status == AVAuthorizationStatus::Authorized {
        MicAuth::Authorized
    } else if status == AVAuthorizationStatus::NotDetermined {
        MicAuth::NotDetermined
    } else {
        // Denied or Restricted.
        MicAuth::Denied
    }
}

/// Asks macOS to show the microphone prompt (only ever shown once per app;
/// afterwards the answer is fixed until changed in System Settings). The
/// user's answer shows up in `status()`.
#[cfg(target_os = "macos")]
pub fn request_mic_access() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let Some(kind) = (unsafe { AVMediaTypeAudio }) else {
        return;
    };
    let handler = RcBlock::new(|_granted: Bool| {});
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(kind, &handler) };
}

#[cfg(target_os = "macos")]
pub fn status() -> PermissionStatus {
    PermissionStatus {
        microphone: mic_auth() == MicAuth::Authorized,
        accessibility: unsafe { mac::AXIsProcessTrusted() },
        input_monitoring: unsafe { mac::CGPreflightListenEventAccess() },
    }
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> PermissionStatus {
    PermissionStatus {
        microphone: true,
        accessibility: true,
        input_monitoring: true,
    }
}

/// Opens the matching System Settings pane (no-op on Windows).
pub fn open_settings(kind: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let pane = match kind {
            "microphone" => "Privacy_Microphone",
            "accessibility" => "Privacy_Accessibility",
            "input_monitoring" => "Privacy_ListenEvent",
            other => return Err(format!("unknown permission {other}")),
        };
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}

/// Triggers the system prompt for a permission that was never asked
/// (no-op on Windows). The user's answer shows up in `status()`.
pub fn request(kind: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match kind {
            "microphone" => request_mic_access(),
            "accessibility" => {
                use core_foundation::base::TCFType;
                use core_foundation::boolean::CFBoolean;
                use core_foundation::dictionary::CFDictionary;
                use core_foundation::string::CFString;
                // Value of kAXTrustedCheckOptionPrompt.
                let key = CFString::new("AXTrustedCheckOptionPrompt");
                let opts = CFDictionary::from_CFType_pairs(&[(
                    key.as_CFType(),
                    CFBoolean::true_value().as_CFType(),
                )]);
                unsafe { mac::AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) };
            }
            "input_monitoring" => {
                unsafe { mac::CGRequestListenEventAccess() };
            }
            other => return Err(format!("unknown permission {other}")),
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}
