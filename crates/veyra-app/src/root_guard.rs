/// Returns `true` if the current process is running with the root effective
/// user ID (0). Veyra must never launch with default root privileges
/// (Veyra Development Rule #23 / #20): privileged operations belong behind a
/// Polkit/D-Bus helper, added in a later phase.
pub fn is_running_as_root() -> bool {
    effective_uid() == 0
}

/// # Safety
/// `geteuid(2)` takes no arguments, performs no pointer dereferences and
/// cannot fail; it is safe to call from any thread at any time.
fn effective_uid() -> libc::uid_t {
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_uid_matches_libc_directly() {
        assert_eq!(effective_uid(), unsafe { libc::geteuid() });
    }
}
