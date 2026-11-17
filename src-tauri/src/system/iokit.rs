//! Minimal IOKit registry reader (macOS only). Used for AppleSmartBattery.

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use io_kit_sys::{
    kIOMasterPortDefault, IOObjectRelease, IORegistryEntryCreateCFProperties,
    IOServiceGetMatchingService, IOServiceMatching,
};
use std::ffi::CString;

pub struct Properties(CFDictionary<CFString, CFType>);

impl Properties {
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.0
            .find(CFString::new(key))
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.0
            .find(CFString::new(key))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
    }

    pub fn dict(&self, key: &str) -> Option<Properties> {
        let v = self.0.find(CFString::new(key))?;
        let d = v.downcast::<CFDictionary>()?;
        // SAFETY: re-wrap the untyped dictionary with typed keys/values; IOKit
        // registry dictionaries are always keyed by CFString.
        let typed: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(d.as_concrete_TypeRef()) };
        Some(Properties(typed))
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        let v = self.0.find(CFString::new(key))?;
        if let Some(b) = v.downcast::<CFBoolean>() {
            return Some(b.into());
        }
        v.downcast::<CFNumber>().and_then(|n| n.to_i64()).map(|n| n != 0)
    }
}

/// Return the registry properties of the first service of the given IOKit class.
/// Returns `None` when the service does not exist (e.g. no battery).
pub fn service_properties(class_name: &str) -> Option<Properties> {
    let cname = CString::new(class_name).ok()?;
    // SAFETY: IOServiceMatching returns a +1 dictionary which
    // IOServiceGetMatchingService consumes. The returned service is released
    // below; the properties dictionary follows the Create rule.
    unsafe {
        let matching = IOServiceMatching(cname.as_ptr());
        if matching.is_null() {
            return None;
        }
        #[allow(deprecated)]
        let service = IOServiceGetMatchingService(kIOMasterPortDefault, matching);
        if service == 0 {
            return None;
        }
        let mut props = std::ptr::null_mut();
        let kr = IORegistryEntryCreateCFProperties(service, &mut props, std::ptr::null(), 0);
        IOObjectRelease(service);
        if kr != 0 || props.is_null() {
            return None;
        }
        Some(Properties(CFDictionary::wrap_under_create_rule(props as _)))
    }
}
