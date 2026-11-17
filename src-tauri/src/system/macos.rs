//! Thin, safe wrappers over macOS kernel interfaces (sysctl, Mach host statistics).

#[cfg(target_os = "macos")]
#[allow(deprecated)] // libc marks mach_host_self deprecated in favour of the mach2 crate
mod imp {
    use std::ffi::CString;

    pub fn sysctl_string(name: &str) -> Option<String> {
        let cname = CString::new(name).ok()?;
        let mut len: libc::size_t = 0;
        // SAFETY: first call queries the required length only.
        let rc = unsafe {
            libc::sysctlbyname(cname.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0)
        };
        if rc != 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        // SAFETY: buf has `len` bytes as reported by the kernel.
        let rc = unsafe {
            libc::sysctlbyname(cname.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0)
        };
        if rc != 0 {
            return None;
        }
        buf.truncate(len);
        while buf.last() == Some(&0) {
            buf.pop();
        }
        String::from_utf8(buf).ok()
    }

    fn sysctl_raw<T: Copy + Default>(name: &str) -> Option<T> {
        let cname = CString::new(name).ok()?;
        let mut value = T::default();
        let mut len = std::mem::size_of::<T>();
        // SAFETY: value is a plain-old-data buffer of exactly `len` bytes.
        let rc = unsafe {
            libc::sysctlbyname(
                cname.as_ptr(),
                (&mut value as *mut T).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (rc == 0 && len == std::mem::size_of::<T>()).then_some(value)
    }

    pub fn sysctl_u32(name: &str) -> Option<u32> {
        sysctl_raw::<u32>(name)
    }

    pub fn sysctl_u64(name: &str) -> Option<u64> {
        sysctl_raw::<u64>(name).or_else(|| sysctl_u32(name).map(u64::from))
    }

    /// Aggregate CPU ticks: (user, system, idle, nice).
    pub fn host_cpu_ticks() -> Option<[u64; 4]> {
        let mut info = libc::host_cpu_load_info { cpu_ticks: [0; 4] };
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        // SAFETY: info is a correctly sized host_cpu_load_info; count matches.
        let rc = unsafe {
            #[allow(deprecated)]
            libc::host_statistics(
                libc::mach_host_self(),
                libc::HOST_CPU_LOAD_INFO,
                (&mut info as *mut libc::host_cpu_load_info).cast(),
                &mut count,
            )
        };
        if rc != libc::KERN_SUCCESS {
            return None;
        }
        let t = info.cpu_ticks;
        Some([
            t[libc::CPU_STATE_USER as usize] as u64,
            t[libc::CPU_STATE_SYSTEM as usize] as u64,
            t[libc::CPU_STATE_IDLE as usize] as u64,
            t[libc::CPU_STATE_NICE as usize] as u64,
        ])
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub struct VmPages {
        pub page_size: u64,
        pub internal: u64,
        pub purgeable: u64,
        pub external: u64,
        pub wired: u64,
        pub compressed: u64,
        pub free: u64,
    }

    pub fn vm_pages() -> Option<VmPages> {
        // SAFETY: zeroed is a valid bit pattern for this POD struct.
        let mut stats: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        // SAFETY: stats is large enough for HOST_VM_INFO64_COUNT integers; the
        // kernel writes at most `count` and updates it.
        let rc = unsafe {
            libc::host_statistics64(
                libc::mach_host_self(),
                libc::HOST_VM_INFO64,
                (&mut stats as *mut libc::vm_statistics64).cast(),
                &mut count,
            )
        };
        if rc != libc::KERN_SUCCESS {
            return None;
        }
        let page_size = sysctl_u64("hw.pagesize").unwrap_or(16384);
        Some(VmPages {
            page_size,
            internal: stats.internal_page_count as u64,
            purgeable: stats.purgeable_count as u64,
            external: stats.external_page_count as u64,
            wired: stats.wire_count as u64,
            compressed: stats.compressor_page_count as u64,
            free: stats.free_count as u64,
        })
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;

#[cfg(not(target_os = "macos"))]
mod imp_stub {
    pub fn sysctl_string(_: &str) -> Option<String> { None }
    pub fn sysctl_u32(_: &str) -> Option<u32> { None }
    pub fn sysctl_u64(_: &str) -> Option<u64> { None }
}
#[cfg(not(target_os = "macos"))]
pub use imp_stub::*;

/// Model identifiers of fanless Macs (MacBook Air on Apple Silicon).
/// Source: Apple "Identify your MacBook Air model" support article.
const FANLESS_MODELS: &[&str] = &[
    "MacBookAir10,1", // M1
    "Mac14,2",        // M2 13"
    "Mac14,15",       // M2 15"
    "Mac15,12",       // M3 13"
    "Mac15,13",       // M3 15"
    "Mac16,12",       // M4 13"
    "Mac16,13",       // M4 15"
];

pub fn is_fanless_model(model: &str) -> Option<bool> {
    if FANLESS_MODELS.contains(&model) {
        Some(true)
    } else if model.starts_with("MacBookPro")
        || model.starts_with("Macmini")
        || model.starts_with("iMac")
        || model.starts_with("MacPro")
    {
        Some(false)
    } else {
        None
    }
}

pub fn pressure_label(level: u32) -> &'static str {
    match level {
        1 => "normal",
        2 => "warning",
        4 => "critical",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanless_lookup() {
        assert_eq!(is_fanless_model("MacBookAir10,1"), Some(true));
        assert_eq!(is_fanless_model("MacBookPro18,3"), Some(false));
        assert_eq!(is_fanless_model("Mac99,1"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_real_kernel_values() {
        assert!(sysctl_string("machdep.cpu.brand_string").is_some());
        assert!(sysctl_u64("hw.memsize").unwrap() > 0);
        let t = host_cpu_ticks().unwrap();
        assert!(t.iter().sum::<u64>() > 0);
        let vm = vm_pages().unwrap();
        assert!(vm.page_size >= 4096);
        assert!(vm.wired > 0);
    }
}
