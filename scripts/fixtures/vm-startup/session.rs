//! Active-session identity; physical console is not an enhanced/RDP session.
#[derive(Debug, PartialEq)]
pub struct Session {
    pub id: u32,
    pub active: bool,
    pub user: String,
    pub domain: String,
    pub protocol: u16,
}
pub fn select(sessions: &[Session]) -> Option<&Session> {
    let mut found = sessions.iter().filter(|s| {
        s.id != 0
            && s.active
            && s.user.eq_ignore_ascii_case("GPT")
            && s.domain.eq_ignore_ascii_case("DESKTOP-1TAP8BI")
            && matches!(s.protocol, 0 | 2)
    });
    let first = found.next()?;
    if found.next().is_some() {
        None
    } else {
        Some(first)
    }
}
#[cfg(windows)]
pub fn query() -> Result<Vec<Session>, Box<dyn std::error::Error>> {
    #[repr(C)]
    struct Info {
        id: u32,
        name: *mut u16,
        state: i32,
    }
    #[link(name = "wtsapi32")]
    unsafe extern "system" {
        fn WTSEnumerateSessionsW(
            server: isize,
            reserved: u32,
            version: u32,
            info: *mut *mut Info,
            count: *mut u32,
        ) -> i32;
        fn WTSQuerySessionInformationW(
            server: isize,
            id: u32,
            class: u32,
            buffer: *mut *mut u16,
            bytes: *mut u32,
        ) -> i32;
        fn WTSFreeMemory(buffer: *mut std::ffi::c_void);
    }
    struct Buffer(*mut std::ffi::c_void);
    impl Drop for Buffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { WTSFreeMemory(self.0) }
            }
        }
    }
    fn field(id: u32, class: u32) -> Result<Vec<u16>, Box<dyn std::error::Error>> {
        unsafe {
            let mut p = std::ptr::null_mut();
            let mut bytes = 0;
            let success = WTSQuerySessionInformationW(0, id, class, &mut p, &mut bytes);
            let _owned = Buffer(p.cast());
            if success == 0 || p.is_null() || bytes % 2 != 0 || bytes > 65536 {
                return Err("Session query failed".into());
            }
            Ok(std::slice::from_raw_parts(p, bytes as usize / 2).to_vec())
        }
    }
    fn text(v: Vec<u16>) -> Result<String, Box<dyn std::error::Error>> {
        let end = v
            .iter()
            .position(|c| *c == 0)
            .ok_or("Session string not terminated")?;
        Ok(String::from_utf16(&v[..end])?)
    }
    unsafe {
        let mut p = std::ptr::null_mut();
        let mut count = 0;
        let success = WTSEnumerateSessionsW(0, 0, 1, &mut p, &mut count);
        let _owned = Buffer(p.cast());
        if success == 0 || count > 1024 || (count > 0 && p.is_null()) {
            return Err("Session enumeration failed".into());
        }
        let mut result = Vec::new();
        if count == 0 {
            return Ok(result);
        }
        for s in std::slice::from_raw_parts(p, count as usize) {
            if s.id == 0 || s.state != 0 {
                continue;
            }
            let user = text(field(s.id, 5)?)?;
            // Do not emit or query unrelated users' domain/protocol.
            if !user.eq_ignore_ascii_case("GPT") {
                continue;
            }
            let domain = text(field(s.id, 7)?)?;
            let protocol = field(s.id, 16)?;
            if protocol.len() != 1 {
                return Err("Session protocol width differs".into());
            }
            result.push(Session {
                id: s.id,
                active: true,
                user,
                domain,
                protocol: protocol[0],
            });
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn s(id: u32, active: bool, user: &str, protocol: u16) -> Session {
        Session {
            id,
            active,
            user: user.into(),
            domain: "DESKTOP-1TAP8BI".into(),
            protocol,
        }
    }
    #[test]
    fn enhanced_gpt_selected_without_physical_console() {
        assert_eq!(
            select(&[s(1, true, "", 0), s(3, true, "GPT", 2)]).map(|s| s.id),
            Some(3)
        );
    }
    #[test]
    fn disconnected_and_listener_are_not_logon_proof() {
        assert!(
            select(&[
                s(3, false, "GPT", 2),
                s(1, true, "", 0),
                s(0, true, "GPT", 0)
            ])
            .is_none()
        );
    }
    #[test]
    fn ambiguous_sessions_refuse_instead_of_logging_off_arbitrary_user() {
        assert!(select(&[s(3, true, "GPT", 2), s(4, true, "GPT", 0)]).is_none());
    }
    #[test]
    fn exact_guest_domain_and_supported_protocol_required() {
        let mut a = s(3, true, "GPT", 2);
        a.domain = "OTHER".into();
        assert!(select(&[a, s(4, true, "GPT", 1), s(5, true, "OTHER", 2)]).is_none());
    }
}
