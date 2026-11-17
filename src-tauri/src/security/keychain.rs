//! macOS Keychain storage for provider credentials (generic passwords).
//! Secrets are only ever read inside Rust; the frontend learns *whether* a
//! key exists, never its value.

use crate::error::{AppError, AppResult};

pub const SERVICE: &str = "com.macobserver.app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// Regular API key used for connection tests and test requests.
    Api,
    /// Admin / management key used for organisation usage & cost APIs.
    Admin,
}

impl KeyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyKind::Api => "api",
            KeyKind::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "api" => Ok(KeyKind::Api),
            "admin" => Ok(KeyKind::Admin),
            _ => Err(AppError::Invalid(format!("unknown key kind '{s}'"))),
        }
    }
}

fn account(provider: &str, kind: KeyKind) -> String {
    format!("{provider}:{}", kind.as_str())
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    // errSecItemNotFound
    const NOT_FOUND: i32 = -25300;

    pub fn set(provider: &str, kind: KeyKind, secret: &str) -> AppResult<()> {
        set_generic_password(SERVICE, &account(provider, kind), secret.as_bytes())
            .map_err(|e| AppError::Keychain(format!("could not save credential (OSStatus {})", e.code())))
    }

    pub fn get(provider: &str, kind: KeyKind) -> AppResult<Option<String>> {
        match get_generic_password(SERVICE, &account(provider, kind)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| AppError::Keychain("stored credential is not valid UTF-8".into())),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(e) => Err(AppError::Keychain(format!("could not read credential (OSStatus {})", e.code()))),
        }
    }

    pub fn delete(provider: &str, kind: KeyKind) -> AppResult<()> {
        match delete_generic_password(SERVICE, &account(provider, kind)) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Err(e) => Err(AppError::Keychain(format!("could not delete credential (OSStatus {})", e.code()))),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    pub fn set(_: &str, _: KeyKind, _: &str) -> AppResult<()> {
        Err(AppError::Unsupported("Keychain is only available on macOS".into()))
    }
    pub fn get(_: &str, _: KeyKind) -> AppResult<Option<String>> {
        Ok(None)
    }
    pub fn delete(_: &str, _: KeyKind) -> AppResult<()> {
        Ok(())
    }
}

pub use imp::{delete, get, set};

pub fn exists(provider: &str, kind: KeyKind) -> bool {
    matches!(get(provider, kind), Ok(Some(_)))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Writes, reads and deletes a throwaway entry in the login keychain.
    /// Ignored by default because it touches the user's real keychain;
    /// run with `cargo test -- --ignored keychain`.
    #[test]
    #[ignore]
    fn keychain_roundtrip() {
        let p = "mac-observer-selftest";
        set(p, KeyKind::Api, "test-value-123").unwrap();
        assert_eq!(get(p, KeyKind::Api).unwrap().as_deref(), Some("test-value-123"));
        delete(p, KeyKind::Api).unwrap();
        assert_eq!(get(p, KeyKind::Api).unwrap(), None);
    }
}
