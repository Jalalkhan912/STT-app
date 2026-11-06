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
pub fn status() -> PermissionStatus {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    let microphone = unsafe {
        match AVMediaTypeAudio {
            Some(kind) => {
                AVCaptureDevice::authorizationStatusForMediaType(kind)
                    == AVAuthorizationStatus::Authorized
            }
            None => false,
        }
    };
    PermissionStatus {
        microphone,
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
            "microphone" => super::recorder::probe_default_input(),
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
