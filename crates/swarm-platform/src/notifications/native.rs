use super::{Identity, NotificationError, ToastContent};
use std::path::{Path, PathBuf};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::{DateTime, IReference, PropertyValue, TypedEventHandler};
use windows::UI::Notifications::{
    NotificationSetting, ToastFailedEventArgs, ToastNotification, ToastNotificationManager,
    ToastNotifier,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::{InitPropVariantFromCLSID, PROPVARIANT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemAlloc, CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::System::WinRT::{
    IActivationFactory, RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHCNE_ASSOCCHANGED, SHCNE_CREATE, SHCNF_FLUSH,
    SHCNF_IDLIST, SHCNF_PATHW, SHChangeNotify, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{BSTR, GUID, HSTRING, Interface, PWSTR};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_CREATED_NEW_KEY, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteKeyW, RegDeleteValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegQueryValueExW, RegSetValueExW,
};

const PROPERTY_SET: GUID = GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3);
const APP_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: PROPERTY_SET,
    pid: 5,
};
const ACTIVATOR: PROPERTYKEY = PROPERTYKEY {
    fmtid: PROPERTY_SET,
    pid: 26,
};
// Derived from the canonical icon.png by the existing desktop asset pipeline.
// Embed it so debug, runtime and installed GUI builds need no loose resource.
const ICON: &[u8] = include_bytes!("../../../../apps/desktop/src-tauri/icons/128x128.png");
const ICON_BACKGROUND: &str = "00000000";

pub struct Registration {
    identity: Identity,
    shortcut: PathBuf,
    registry_path: String,
    app_registry_path: String,
}
impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationRegistration([redacted])")
    }
}
impl Registration {
    fn icon_path(&self) -> PathBuf {
        use sha2::{Digest, Sha256};
        self.identity
            .root
            .join("notification-state")
            .join(format!("icon-{:x}.png", Sha256::digest(ICON)))
    }
    fn icon_matches(&self) -> Result<bool, NotificationError> {
        use std::os::windows::fs::MetadataExt;
        let path = self.icon_path();
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(NotificationError::Storage),
            Ok(metadata)
                if !metadata.is_file()
                    || metadata.file_attributes() & 0x400 != 0
                    || metadata.len() != ICON.len() as u64 =>
            {
                return Err(NotificationError::Conflict);
            }
            Ok(_) => {}
        }
        if std::fs::read(path).map_err(|_| NotificationError::Storage)? != ICON {
            return Err(NotificationError::Conflict);
        }
        Ok(true)
    }
    fn install_icon(&self) -> Result<(), NotificationError> {
        use std::io::Write;
        if self.icon_matches()? {
            return Ok(());
        }
        let path = self.icon_path();
        let temporary = path.with_file_name(format!(".icon-{}.tmp", uuid::Uuid::new_v4()));
        let mut created = false;
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| NotificationError::Storage)?;
            created = true;
            file.write_all(ICON)
                .map_err(|_| NotificationError::Storage)?;
            file.sync_all().map_err(|_| NotificationError::Storage)?;
            drop(file);
            match std::fs::hard_link(&temporary, &path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if self.icon_matches()? {
                        Ok(())
                    } else {
                        Err(NotificationError::Conflict)
                    }
                }
                Err(_) => Err(NotificationError::Storage),
            }
        })();
        let cleanup = if created {
            std::fs::remove_file(temporary).map_err(|_| NotificationError::Storage)
        } else {
            Ok(())
        };
        result.and(cleanup)
    }
    /// Construction is read-only. Registration must precede notification delivery.
    pub fn new(identity: Identity) -> Result<Self, NotificationError> {
        let _apartment = Apartment::com()?;
        let programs = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }
            .map_err(api)?;
        let value = unsafe { programs.to_string() };
        unsafe { CoTaskMemFree(Some(programs.0.cast())) };
        let programs = PathBuf::from(value.map_err(|_| NotificationError::InvalidInput)?);
        Ok(Self {
            shortcut: programs.join(format!("Swarm-{}.lnk", identity.suffix)),
            registry_path: format!("Software\\Classes\\{}", identity.scheme()),
            app_registry_path: format!("Software\\Classes\\AppUserModelId\\{}", identity.app_id()),
            identity,
        })
    }
    fn command(&self) -> Result<String, NotificationError> {
        Ok(format!(
            "{} --data-dir {} --notification-activation \"%1\"",
            quote_path(&self.identity.executable)?,
            quote_path(&self.identity.root)?
        ))
    }
    fn shortcut_args(&self) -> Result<String, NotificationError> {
        Ok(format!("--data-dir {}", quote_path(&self.identity.root)?))
    }
    fn activator(&self) -> GUID {
        let mut bytes = [0; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&self.identity.suffix[index * 2..index * 2 + 2], 16)
                .expect("SHA256 hexadecimal");
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        GUID::from_u128(u128::from_be_bytes(bytes))
    }
    fn shortcut_matches(&self) -> Result<bool, NotificationError> {
        match std::fs::symlink_metadata(&self.shortcut) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(NotificationError::Storage),
            Ok(metadata) => {
                use std::os::windows::fs::MetadataExt;
                if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
                    return Err(NotificationError::Conflict);
                }
            }
        }
        let _apartment = Apartment::com()?;
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.map_err(api)?;
        let file: IPersistFile = link.cast().map_err(api)?;
        unsafe { file.Load(&HSTRING::from(self.shortcut.as_os_str()), STGM_READ) }
            .map_err(|_| NotificationError::Conflict)?;
        let mut path = vec![0u16; 32768];
        let mut args = vec![0u16; 32768];
        unsafe {
            link.GetPath(&mut path, std::ptr::null_mut(), 0)
                .map_err(api)?;
            link.GetArguments(&mut args).map_err(api)?;
        }
        let path = terminated_text(&path)?;
        let args = terminated_text(&args)?;
        let properties: IPropertyStore = link.cast().map_err(api)?;
        let app = unsafe { properties.GetValue(&APP_ID) }.map_err(api)?;
        let clsid = unsafe { properties.GetValue(&ACTIVATOR) }.map_err(api)?;
        let expected = unsafe { InitPropVariantFromCLSID(&self.activator()) }.map_err(api)?;
        if std::fs::canonicalize(path).ok().as_ref() != Some(&self.identity.executable)
            || args != self.shortcut_args()?
            || BSTR::try_from(&app).map_err(|_| NotificationError::Conflict)?
                != self.identity.app_id().as_str()
            || clsid != expected
        {
            return Err(NotificationError::Conflict);
        }
        Ok(true)
    }
    fn protocol_matches(&self) -> Result<bool, NotificationError> {
        let Some(key) = Key::open(&self.registry_path)? else {
            return Ok(false);
        };
        if key.get("SwarmOwner")?.as_deref() != Some(&self.command()?) {
            return Err(NotificationError::Conflict);
        }
        let mut complete = true;
        for (name, expected) in [("", "URL:Swarm notification"), ("URL Protocol", "")] {
            match key.get(name)? {
                Some(value) if value == expected => {}
                None => complete = false,
                _ => return Err(NotificationError::Conflict),
            }
        }
        match Key::open(&format!("{}\\shell\\open\\command", self.registry_path))? {
            Some(key) => match key.get("")? {
                Some(value) if value == self.command()? => {}
                None => complete = false,
                _ => return Err(NotificationError::Conflict),
            },
            None => complete = false,
        }
        Ok(complete)
    }
    fn activator_text(&self) -> String {
        format!("{{{:?}}}", self.activator())
    }
    fn app_matches(&self) -> Result<bool, NotificationError> {
        let Some(key) = Key::open(&self.app_registry_path)? else {
            return Ok(false);
        };
        if key.get("SwarmOwner")?.as_deref() != Some(&self.command()?) {
            return Err(NotificationError::Conflict);
        }
        let mut complete = true;
        for (name, expected) in [
            ("DisplayName", "Swarm".to_owned()),
            ("CustomActivator", self.activator_text()),
            ("IconUri", shell_path(&self.icon_path())?),
            ("IconBackgroundColor", ICON_BACKGROUND.to_owned()),
        ] {
            match key.get(name)? {
                Some(value) if value == expected => {}
                None => complete = false,
                _ => return Err(NotificationError::Conflict),
            }
        }
        Ok(complete)
    }
    pub fn is_registered(&self) -> Result<bool, NotificationError> {
        let shortcut = self.shortcut_matches()?;
        let protocol = self.protocol_matches()?;
        let app = self.app_matches()?;
        let icon = self.icon_matches()?;
        Ok(shortcut && protocol && app && icon)
    }
    /// Existing mismatched registration/shortcut is always refused.
    pub fn install(&self) -> Result<(), NotificationError> {
        let store = crate::CredentialStore::open(&self.identity.root.join("notification-state"))
            .map_err(|_| NotificationError::Storage)?;
        let _gate = store
            .lock("registration")
            .map_err(|_| NotificationError::Storage)?;
        let shortcut_exists = self.shortcut_matches()?;
        self.protocol_matches()?;
        self.app_matches()?;
        self.install_icon()?;
        let (app, created) = Key::create(&self.app_registry_path)?;
        if created {
            app.ensure("SwarmOwner", &self.command()?)?;
        } else if app.get("SwarmOwner")?.as_deref() != Some(&self.command()?) {
            return Err(NotificationError::Conflict);
        }
        app.ensure("DisplayName", "Swarm")?;
        app.ensure("CustomActivator", &self.activator_text())?;
        app.ensure("IconUri", &shell_path(&self.icon_path())?)?;
        app.ensure("IconBackgroundColor", ICON_BACKGROUND)?;
        let (key, created) = Key::create(&self.registry_path)?;
        if created {
            key.ensure("SwarmOwner", &self.command()?)?;
        } else if key.get("SwarmOwner")?.as_deref() != Some(&self.command()?) {
            return Err(NotificationError::Conflict);
        }
        key.ensure("", "URL:Swarm notification")?;
        key.ensure("URL Protocol", "")?;
        let (command, _) = Key::create(&format!("{}\\shell\\open\\command", self.registry_path))?;
        command.ensure("", &self.command()?)?;
        if !shortcut_exists {
            let _apartment = Apartment::com()?;
            let link: IShellLinkW =
                unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.map_err(api)?;
            let properties: IPropertyStore = link.cast().map_err(api)?;
            let clsid = unsafe { InitPropVariantFromCLSID(&self.activator()) }.map_err(api)?;
            unsafe {
                link.SetPath(&HSTRING::from(shell_path(&self.identity.executable)?))
                    .map_err(|e| api_step(e, "SetPath"))?;
                link.SetArguments(&HSTRING::from(self.shortcut_args()?))
                    .map_err(|e| api_step(e, "SetArguments"))?;
                properties
                    .SetValue(&APP_ID, &string_variant(&self.identity.app_id())?)
                    .map_err(|e| api_step(e, "AppUserModelID"))?;
                properties
                    .SetValue(&ACTIVATOR, &clsid)
                    .map_err(|e| api_step(e, "ActivatorCLSID"))?;
                properties
                    .Commit()
                    .map_err(|e| api_step(e, "PropertyCommit"))?;
            }
            let temporary = self
                .shortcut
                .with_file_name(format!(".swarm-{}.tmp.lnk", uuid::Uuid::new_v4()));
            let file: IPersistFile = link.cast().map_err(api)?;
            unsafe { file.Save(&HSTRING::from(temporary.as_os_str()), true) }
                .map_err(|e| api_step(e, "ShortcutSave"))?;
            // Fail if the destination was created concurrently; never overwrite it.
            let result = std::fs::hard_link(&temporary, &self.shortcut);
            std::fs::remove_file(&temporary).map_err(|_| NotificationError::Storage)?;
            result.map_err(|_| NotificationError::Conflict)?;
            let path = HSTRING::from(shell_path(&self.shortcut)?);
            unsafe {
                SHChangeNotify(
                    SHCNE_CREATE,
                    SHCNF_PATHW | SHCNF_FLUSH,
                    Some(path.as_ptr().cast()),
                    None,
                );
                SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST | SHCNF_FLUSH, None, None);
            }
        }
        if self.is_registered()? {
            Ok(())
        } else {
            Err(NotificationError::Conflict)
        }
    }
    /// Removes only this exact shortcut and known protocol values/empty keys.
    pub fn remove(&self) -> Result<(), NotificationError> {
        let store = crate::CredentialStore::open(&self.identity.root.join("notification-state"))
            .map_err(|_| NotificationError::Storage)?;
        let _gate = store
            .lock("registration")
            .map_err(|_| NotificationError::Storage)?;
        let shortcut = self.shortcut_matches()?;
        self.protocol_matches()?;
        self.app_matches()?;
        let icon = self.icon_matches()?;
        if shortcut {
            std::fs::remove_file(&self.shortcut).map_err(|_| NotificationError::Storage)?;
        }
        if let Some(command) = Key::open(&format!("{}\\shell\\open\\command", self.registry_path))?
        {
            command.delete_exact("", &self.command()?)?;
        }
        for suffix in ["\\shell\\open\\command", "\\shell\\open", "\\shell"] {
            delete_empty_key(&format!("{}{suffix}", self.registry_path))?;
        }
        if let Some(key) = Key::open(&self.registry_path)? {
            key.delete_exact("", "URL:Swarm notification")?;
            key.delete_exact("URL Protocol", "")?;
            key.delete_exact("SwarmOwner", &self.command()?)?;
        }
        delete_empty_key(&self.registry_path)?;
        if let Some(app) = Key::open(&self.app_registry_path)? {
            app.delete_exact("DisplayName", "Swarm")?;
            app.delete_exact("CustomActivator", &self.activator_text())?;
            app.delete_exact("IconUri", &shell_path(&self.icon_path())?)?;
            app.delete_exact("IconBackgroundColor", ICON_BACKGROUND)?;
            app.delete_exact("SwarmOwner", &self.command()?)?;
        }
        delete_empty_key(&self.app_registry_path)?;
        if icon {
            std::fs::remove_file(self.icon_path()).map_err(|_| NotificationError::Storage)?;
        }
        Ok(())
    }
    pub fn show(&self, content: &ToastContent) -> Result<(), NotificationError> {
        if content.app_id != self.identity.app_id() {
            return Err(NotificationError::InvalidInput);
        }
        if !self.is_registered()? {
            return Err(NotificationError::Conflict);
        }
        let _apartment = Apartment::winrt()?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
            self.identity.app_id(),
        ))
        .map_err(|e| api_step(e, "CreateToastNotifier"))?;
        let setting = match notifier.Setting() {
            Ok(setting) => setting,
            Err(error) if error.code().0 as u32 == 0x80070490 => {
                self.initialize_notification_profile(&notifier)?;
                notifier
                    .Setting()
                    .map_err(|e| api_step(e, "NotifierSettingAfterProfile"))?
            }
            Err(error) => return Err(api_step(error, "NotifierSetting")),
        };
        if setting != NotificationSetting::Enabled {
            return Err(NotificationError::Disabled);
        }
        let xml = new_xml_document()?;
        xml.LoadXml(&HSTRING::from(&content.xml)).map_err(api)?;
        let toast = ToastNotification::CreateToastNotification(&xml).map_err(api)?;
        toast.SetTag(&HSTRING::from(&content.tag)).map_err(api)?;
        toast.SetGroup(&HSTRING::from("Swarm")).map_err(api)?;
        toast
            .SetSuppressPopup(content.suppress_popup)
            .map_err(api)?;
        let expected_xml = xml.GetXml().map_err(api)?;
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failed_callback = failed.clone();
        let event = toast
            .Failed(
                &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, _| {
                    failed_callback.store(true, std::sync::atomic::Ordering::Release);
                    Ok(())
                }),
            )
            .map_err(api)?;
        let result = (|| {
            notifier
                .Show(&toast)
                .map_err(|e| api_step(e, "NotifierShow"))?;
            let history = ToastNotificationManager::History().map_err(api)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if failed.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(NotificationError::WindowsApi);
                }
                let entries = history
                    .GetHistoryWithId(&HSTRING::from(self.identity.app_id()))
                    .map_err(api)?;
                for entry in entries {
                    if entry.Tag().map_err(api)? == content.tag.as_str()
                        && entry.Group().map_err(api)? == "Swarm"
                        && entry.Content().map_err(api)?.GetXml().map_err(api)? == expected_xml
                    {
                        return Ok(());
                    }
                }
                if std::time::Instant::now() >= deadline {
                    return Err(NotificationError::WindowsApi);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })();
        toast.RemoveFailed(event).map_err(api)?;
        result
    }
    fn initialize_notification_profile(
        &self,
        notifier: &ToastNotifier,
    ) -> Result<(), NotificationError> {
        // Unpackaged senders acquire their notification profile on their first Show.
        // This probe contains no target or post data and never requests a popup.
        let xml = new_xml_document()?;
        xml.LoadXml(&HSTRING::from("<toast><visual><binding template=\"ToastGeneric\"><text>Swarm</text></binding></visual></toast>"))
            .map_err(api)?;
        let toast = ToastNotification::CreateToastNotification(&xml).map_err(api)?;
        toast.SetTag(&HSTRING::from("SwarmSetup")).map_err(api)?;
        toast.SetGroup(&HSTRING::from("Swarm")).map_err(api)?;
        toast.SetSuppressPopup(true).map_err(api)?;
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NotificationError::WindowsApi)?
            .as_secs();
        let expires = DateTime {
            UniversalTime: ((unix + 11_644_473_600 + 15) * 10_000_000) as i64,
        };
        let expiration: IReference<DateTime> = PropertyValue::CreateDateTime(expires)
            .map_err(api)?
            .cast()
            .map_err(api)?;
        toast.SetExpirationTime(&expiration).map_err(api)?;
        notifier
            .Show(&toast)
            .map_err(|e| api_step(e, "ProfileShow"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let result = loop {
            match notifier.Setting() {
                Ok(_) => break Ok(()),
                Err(error)
                    if error.code().0 as u32 == 0x80070490
                        && std::time::Instant::now() < deadline => {}
                Err(error) => break Err(api_step(error, "ProfileSetting")),
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        ToastNotificationManager::History()
            .map_err(api)?
            .RemoveGroupedTagWithId(
                &HSTRING::from("SwarmSetup"),
                &HSTRING::from("Swarm"),
                &HSTRING::from(self.identity.app_id()),
            )
            .map_err(api)?;
        result
    }
    pub fn clear_history(&self) -> Result<(), NotificationError> {
        if !self.is_registered()? {
            return Err(NotificationError::Conflict);
        }
        let _apartment = Apartment::winrt()?;
        ToastNotificationManager::History()
            .map_err(api)?
            .ClearWithId(&HSTRING::from(self.identity.app_id()))
            .map_err(api)
    }
}
fn new_xml_document() -> Result<XmlDocument, NotificationError> {
    // Avoid the generated process cache: MSXML factories did not survive a
    // balanced apartment teardown in the real consecutive-submission test.
    let factory = windows::core::factory::<XmlDocument, IActivationFactory>().map_err(api)?;
    unsafe { factory.ActivateInstance() }
        .map_err(api)?
        .cast()
        .map_err(api)
}
fn api(error: windows::core::Error) -> NotificationError {
    #[cfg(test)]
    eprintln!(
        "notification API test HRESULT: {:08x}",
        error.code().0 as u32
    );
    #[cfg(not(test))]
    let _ = error;
    NotificationError::WindowsApi
}
fn api_step(error: windows::core::Error, step: &str) -> NotificationError {
    #[cfg(test)]
    eprintln!("notification API step: {step}");
    #[cfg(not(test))]
    let _ = step;
    api(error)
}
struct Apartment(Option<bool>);
impl Apartment {
    fn com() -> Result<Self, NotificationError> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.0 as u32 == 0x80010106 {
            // Shell interfaces also work in the caller's existing apartment.
            return Ok(Self(None));
        }
        result.ok().map_err(api)?;
        Ok(Self(Some(false)))
    }
    fn winrt() -> Result<Self, NotificationError> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(api)?;
        Ok(Self(Some(true)))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            match self.0 {
                Some(true) => RoUninitialize(),
                Some(false) => CoUninitialize(),
                None => {}
            }
        }
    }
}
fn terminated_text(units: &[u16]) -> Result<String, NotificationError> {
    let end = units
        .iter()
        .position(|&u| u == 0)
        .ok_or(NotificationError::Conflict)?;
    String::from_utf16(&units[..end]).map_err(|_| NotificationError::Conflict)
}
fn string_variant(text: &str) -> Result<PROPVARIANT, NotificationError> {
    let units = wide(text);
    let allocation = unsafe { CoTaskMemAlloc(units.len() * 2) }.cast::<u16>();
    if allocation.is_null() {
        return Err(NotificationError::WindowsApi);
    }
    let mut value = PROPVARIANT::default();
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), allocation, units.len());
        (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
        (*value.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(allocation);
    }
    Ok(value)
}
fn quote_path(path: &Path) -> Result<String, NotificationError> {
    let text = shell_path(path)?;
    if text.contains(['\0', '"']) {
        return Err(NotificationError::InvalidInput);
    }
    let slashes = text.chars().rev().take_while(|&ch| ch == '\\').count();
    Ok(format!("\"{text}{}\"", "\\".repeat(slashes)))
}
fn shell_path(path: &Path) -> Result<String, NotificationError> {
    let text = path.to_str().ok_or(NotificationError::InvalidInput)?;
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return Ok(format!(r"\\{unc}"));
    }
    Ok(text.strip_prefix(r"\\?\").unwrap_or(text).to_owned())
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}
impl Key {
    fn open(path: &str) -> Result<Option<Self>, NotificationError> {
        let mut key = std::ptr::null_mut();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(path).as_ptr(),
                0,
                KEY_READ | KEY_WRITE,
                &mut key,
            )
        };
        match result {
            0 => Ok(Some(Self(key))),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
            _ => Err(NotificationError::Storage),
        }
    }
    fn create(path: &str) -> Result<(Self, bool), NotificationError> {
        let mut key = std::ptr::null_mut();
        let mut disposition = 0;
        let result = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide(path).as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_READ | KEY_WRITE,
                std::ptr::null(),
                &mut key,
                &mut disposition,
            )
        };
        if result == 0 {
            Ok((Self(key), disposition == REG_CREATED_NEW_KEY))
        } else {
            Err(NotificationError::Storage)
        }
    }
    fn get(&self, name: &str) -> Result<Option<String>, NotificationError> {
        let name = wide(name);
        let mut kind = 0;
        let mut size = 0;
        let result = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != 0 || kind != REG_SZ || !(2..=65536).contains(&size) || size % 2 != 0 {
            return Err(NotificationError::Conflict);
        }
        let mut data = vec![0u8; size as usize];
        let mut actual = size;
        if unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                data.as_mut_ptr(),
                &mut actual,
            )
        } != 0
            || actual != size
            || kind != REG_SZ
        {
            return Err(NotificationError::Conflict);
        }
        let units = data
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect::<Vec<_>>();
        if units.last() != Some(&0) || units[..units.len() - 1].contains(&0) {
            return Err(NotificationError::Conflict);
        }
        Ok(Some(terminated_text(&units)?))
    }
    fn ensure(&self, name: &str, value: &str) -> Result<(), NotificationError> {
        if let Some(existing) = self.get(name)? {
            return if existing == value {
                Ok(())
            } else {
                Err(NotificationError::Conflict)
            };
        }
        let data = wide(value);
        if unsafe {
            RegSetValueExW(
                self.0,
                wide(name).as_ptr(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(NotificationError::Storage)
        }
    }
    fn delete_exact(&self, name: &str, expected: &str) -> Result<(), NotificationError> {
        match self.get(name)? {
            None => return Ok(()),
            Some(value) if value == expected => {}
            _ => return Err(NotificationError::Conflict),
        }
        if unsafe { RegDeleteValueW(self.0, wide(name).as_ptr()) } == 0 {
            Ok(())
        } else {
            Err(NotificationError::Storage)
        }
    }
}
fn delete_empty_key(path: &str) -> Result<(), NotificationError> {
    if let Some(key) = Key::open(path)? {
        let mut keys = 0;
        let mut values = 0;
        let code = unsafe {
            RegQueryInfoKeyW(
                key.0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut keys,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut values,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if code != 0 {
            return Err(NotificationError::Storage);
        }
        if keys != 0 || values != 0 {
            return Err(NotificationError::Conflict);
        }
    }
    let result = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(path).as_ptr()) };
    match result {
        0 | ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(()),
        _ => Err(NotificationError::Conflict),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_registration_is_idempotent_and_refuses_foreign_command_and_shortcut() {
        let root = tempfile::tempdir().unwrap();
        let mut registration = Registration::new(
            Identity::new(root.path(), &std::env::current_exe().unwrap()).unwrap(),
        )
        .unwrap();
        registration.shortcut = root.path().join("Swarm.lnk");
        registration.registry_path = format!("Software\\Swarm\\Tests\\{}", uuid::Uuid::new_v4());
        registration.app_registry_path = format!("{}-app", registration.registry_path);
        assert!(!registration.is_registered().unwrap());
        registration.install().unwrap();
        registration.install().unwrap();
        assert!(registration.is_registered().unwrap());
        let app = Key::open(&registration.app_registry_path).unwrap().unwrap();
        let icon = app
            .get("IconUri")
            .unwrap()
            .expect("notification icon registered");
        assert_eq!(
            std::fs::read(&icon).unwrap(),
            include_bytes!("../../../../apps/desktop/src-tauri/icons/128x128.png")
        );
        assert_eq!(
            app.get("IconBackgroundColor").unwrap().as_deref(),
            Some("00000000")
        );
        // Upgrade an owned legacy registration without changing its identity/shortcut.
        app.delete_exact("IconUri", &icon).unwrap();
        app.delete_exact("IconBackgroundColor", "00000000").unwrap();
        assert!(!registration.is_registered().unwrap());
        registration.install().unwrap();
        assert!(registration.is_registered().unwrap());
        app.delete_exact("IconUri", &icon).unwrap();
        app.ensure("IconUri", "foreign.png").unwrap();
        assert_eq!(
            registration.install().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(
            registration.remove().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(app.get("IconUri").unwrap().as_deref(), Some("foreign.png"));
        app.delete_exact("IconUri", "foreign.png").unwrap();
        app.ensure("IconUri", &icon).unwrap();
        drop(app);
        // Missing assets are repaired only at explicit registration, never on reads.
        std::fs::remove_file(&icon).unwrap();
        assert!(!registration.is_registered().unwrap());
        assert!(!Path::new(&icon).exists());
        registration.install().unwrap();
        // Same-size corrupt bytes must also fail before changing registry/shortcut.
        let mut corrupt = ICON.to_vec();
        corrupt[0] ^= 1;
        std::fs::write(&icon, &corrupt).unwrap();
        assert_eq!(
            registration.install().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(
            registration.remove().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(std::fs::read(&icon).unwrap(), corrupt);
        std::fs::write(&icon, ICON).unwrap();
        let path = format!("{}\\shell\\open\\command", registration.registry_path);
        let key = Key::open(&path).unwrap().unwrap();
        key.delete_exact("", &registration.command().unwrap())
            .unwrap();
        key.ensure("", "foreign command").unwrap();
        assert_eq!(
            registration.install().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(
            registration.remove().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(key.get("").unwrap().unwrap(), "foreign command");
        key.delete_exact("", "foreign command").unwrap();
        key.ensure("", &registration.command().unwrap()).unwrap();
        drop(key);
        registration.remove().unwrap();
        assert!(!registration.is_registered().unwrap());
        assert!(!Path::new(&icon).exists());
        registration.remove().unwrap();
        std::fs::write(&registration.shortcut, b"foreign shortcut").unwrap();
        assert_eq!(
            registration.install().unwrap_err(),
            NotificationError::Conflict
        );
        assert!(Key::open(&registration.registry_path).unwrap().is_none());
        assert_eq!(
            std::fs::read(&registration.shortcut).unwrap(),
            b"foreign shortcut"
        );
    }

    #[test]
    fn icon_asset_refuses_directory_and_oversized_file_without_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let registration = Registration::new(
            Identity::new(root.path(), &std::env::current_exe().unwrap()).unwrap(),
        )
        .unwrap();
        let _store = crate::CredentialStore::open(&root.path().join("notification-state")).unwrap();
        let icon = registration.icon_path();
        std::fs::create_dir(&icon).unwrap();
        assert_eq!(
            registration.install_icon().unwrap_err(),
            NotificationError::Conflict
        );
        assert!(icon.is_dir());
        std::fs::remove_dir(&icon).unwrap();
        std::fs::write(&icon, vec![0; ICON.len() + 1]).unwrap();
        assert_eq!(
            registration.install_icon().unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(
            std::fs::metadata(&icon).unwrap().len(),
            ICON.len() as u64 + 1
        );
    }

    #[test]
    fn foreign_values_are_preserved_by_empty_key_cleanup() {
        let path = format!("Software\\Swarm\\Tests\\{}", uuid::Uuid::new_v4());
        let (key, _) = Key::create(&path).unwrap();
        key.ensure("Foreign", "keep").unwrap();
        assert_eq!(
            delete_empty_key(&path).unwrap_err(),
            NotificationError::Conflict
        );
        assert_eq!(key.get("Foreign").unwrap().as_deref(), Some("keep"));
        key.delete_exact("Foreign", "keep").unwrap();
        drop(key);
        delete_empty_key(&path).unwrap();
    }

    #[test]
    #[ignore = "opt-in real Windows identity and silent notification history test"]
    fn real_windows_silent_submission_replaces_same_id_in_owned_history() {
        let root = tempfile::tempdir().unwrap();
        let identity = Identity::new(root.path(), &std::env::current_exe().unwrap()).unwrap();
        let registration = Registration::new(identity.clone()).unwrap();
        registration.install().unwrap();
        let target = super::super::ActivationTarget::new(
            &uuid::Uuid::new_v4().to_string(),
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let id = format!("batch:{}", uuid::Uuid::new_v4());
        let mut content = ToastContent::new(&identity, &target, &id, 2, 1, None, false).unwrap();
        content.suppress_popup = true;
        let result = (|| {
            registration.show(&content)?;
            content = ToastContent::new(&identity, &target, &id, 3, 1, None, false).unwrap();
            content.suppress_popup = true;
            registration.show(&content)?;
            let _apartment = Apartment::winrt()?;
            let entries = ToastNotificationManager::History()
                .map_err(api)?
                .GetHistoryWithId(&HSTRING::from(identity.app_id()))
                .map_err(api)?;
            assert_eq!(entries.Size().map_err(api)?, 1);
            assert_eq!(
                entries.GetAt(0).map_err(api)?.Tag().map_err(api)?,
                HSTRING::from(id)
            );
            Ok::<_, NotificationError>(())
        })();
        let history_cleanup = registration.clear_history();
        let registration_cleanup = registration.remove();
        assert!(
            history_cleanup.is_ok(),
            "history cleanup: {history_cleanup:?}"
        );
        assert!(
            registration_cleanup.is_ok(),
            "registration cleanup: {registration_cleanup:?}"
        );
        result.unwrap();
    }
}
