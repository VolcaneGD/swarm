//! Read-only Windows UI defaults. No registry or OS preference mutation.
use crate::PlatformError;

#[cfg(windows)]
pub fn japanese() -> bool {
    // Primary language ID: Japanese, including any sublanguage.
    unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff == 0x11 }
}
#[cfg(not(windows))]
pub fn japanese() -> bool {
    false
}

#[cfg(windows)]
pub fn dark() -> Result<bool, PlatformError> {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { RoUninitialize() }
            }
        }
    }
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let owns = match initialized {
        Ok(()) => true,
        Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
        Err(_) => return Err(PlatformError::Io),
    };
    let _apartment = Apartment(owns);
    let foreground = UISettings::new()
        .and_then(|settings| settings.GetColorValue(UIColorType::Foreground))
        .map_err(|_| PlatformError::Io)?;
    Ok(
        5 * u32::from(foreground.G) + 2 * u32::from(foreground.R) + u32::from(foreground.B)
            > 8 * 128,
    )
}
#[cfg(not(windows))]
pub fn dark() -> Result<bool, PlatformError> {
    Err(PlatformError::Unsupported)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn current_windows_defaults_are_readable_without_mutation() {
        super::dark().unwrap();
        let _ = super::japanese();
    }
}
