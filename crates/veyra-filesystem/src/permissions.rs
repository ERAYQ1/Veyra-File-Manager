const SETUID: u32 = 0o4000;
const SETGID: u32 = 0o2000;
const STICKY: u32 = 0o1000;

/// A POSIX permission mode mask (`rwx` for owner/group/other, plus the
/// setuid/setgid/sticky bits), independent of file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilePermissions {
    /// The low 12 bits of `st_mode` (`0o7777`): permission + special bits.
    mode: u32,
}

impl FilePermissions {
    pub fn from_mode(mode: u32) -> Self {
        Self {
            mode: mode & 0o7777,
        }
    }

    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// Four-digit octal representation, e.g. `"0755"`.
    pub fn octal_string(&self) -> String {
        format!("{:04o}", self.mode)
    }

    /// Nine-character symbolic representation, e.g. `"rwxr-xr-x"`, with
    /// setuid/setgid/sticky folded into the executable-bit position per the
    /// conventional `ls -l` presentation (`s`, `S`, `t`, `T`).
    pub fn symbolic_string(&self) -> String {
        let mut out = String::with_capacity(9);
        out.push(triad_char(self.mode, 0o400, 'r'));
        out.push(triad_char(self.mode, 0o200, 'w'));
        out.push(exec_char(
            self.mode,
            0o100,
            self.mode & SETUID != 0,
            's',
            'S',
        ));

        out.push(triad_char(self.mode, 0o040, 'r'));
        out.push(triad_char(self.mode, 0o020, 'w'));
        out.push(exec_char(
            self.mode,
            0o010,
            self.mode & SETGID != 0,
            's',
            'S',
        ));

        out.push(triad_char(self.mode, 0o004, 'r'));
        out.push(triad_char(self.mode, 0o002, 'w'));
        out.push(exec_char(
            self.mode,
            0o001,
            self.mode & STICKY != 0,
            't',
            'T',
        ));

        out
    }

    pub fn is_owner_readable(&self) -> bool {
        self.mode & 0o400 != 0
    }

    pub fn is_owner_writable(&self) -> bool {
        self.mode & 0o200 != 0
    }

    pub fn is_owner_executable(&self) -> bool {
        self.mode & 0o100 != 0
    }

    pub fn is_group_readable(&self) -> bool {
        self.mode & 0o040 != 0
    }

    pub fn is_group_writable(&self) -> bool {
        self.mode & 0o020 != 0
    }

    pub fn is_group_executable(&self) -> bool {
        self.mode & 0o010 != 0
    }

    pub fn is_other_readable(&self) -> bool {
        self.mode & 0o004 != 0
    }

    pub fn is_other_writable(&self) -> bool {
        self.mode & 0o002 != 0
    }

    pub fn is_other_executable(&self) -> bool {
        self.mode & 0o001 != 0
    }

    /// `true` if any of owner/group/other has the executable bit set.
    pub fn is_executable(&self) -> bool {
        self.is_owner_executable() || self.is_group_executable() || self.is_other_executable()
    }

    /// Returns a copy with `bit` set or cleared according to `enabled`.
    /// Special (setuid/setgid/sticky) bits are preserved untouched — callers
    /// only ever pass one of the nine `rwx` triad bits here (the Faz 12
    /// Permissions page UI doesn't expose the special bits).
    fn with_bit(self, bit: u32, enabled: bool) -> Self {
        let mode = if enabled {
            self.mode | bit
        } else {
            self.mode & !bit
        };
        Self { mode }
    }

    pub fn with_owner_read(self, enabled: bool) -> Self {
        self.with_bit(0o400, enabled)
    }

    pub fn with_owner_write(self, enabled: bool) -> Self {
        self.with_bit(0o200, enabled)
    }

    pub fn with_owner_execute(self, enabled: bool) -> Self {
        self.with_bit(0o100, enabled)
    }

    pub fn with_group_read(self, enabled: bool) -> Self {
        self.with_bit(0o040, enabled)
    }

    pub fn with_group_write(self, enabled: bool) -> Self {
        self.with_bit(0o020, enabled)
    }

    pub fn with_group_execute(self, enabled: bool) -> Self {
        self.with_bit(0o010, enabled)
    }

    pub fn with_other_read(self, enabled: bool) -> Self {
        self.with_bit(0o004, enabled)
    }

    pub fn with_other_write(self, enabled: bool) -> Self {
        self.with_bit(0o002, enabled)
    }

    pub fn with_other_execute(self, enabled: bool) -> Self {
        self.with_bit(0o001, enabled)
    }

    /// Faz 28: `true` if the setuid (set-user-ID) bit is set.
    pub fn is_setuid(&self) -> bool {
        self.mode & SETUID != 0
    }

    /// Faz 28: `true` if the setgid (set-group-ID) bit is set.
    pub fn is_setgid(&self) -> bool {
        self.mode & SETGID != 0
    }

    /// Faz 28: `true` if the sticky bit is set.
    pub fn is_sticky(&self) -> bool {
        self.mode & STICKY != 0
    }

    /// Faz 28: Returns a copy with the setuid bit set or cleared.
    pub fn with_setuid(self, enabled: bool) -> Self {
        self.with_bit(SETUID, enabled)
    }

    /// Faz 28: Returns a copy with the setgid bit set or cleared.
    pub fn with_setgid(self, enabled: bool) -> Self {
        self.with_bit(SETGID, enabled)
    }

    /// Faz 28: Returns a copy with the sticky bit set or cleared.
    pub fn with_sticky(self, enabled: bool) -> Self {
        self.with_bit(STICKY, enabled)
    }

    /// Faz 28: Parses a 3- or 4-digit octal string (e.g. `"755"` or `"0755"`)
    /// into a `FilePermissions`. Rejects non-octal digits, empty strings, and
    /// strings longer than 4 characters. Returns `None` on invalid input.
    pub fn parse_octal(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed.len() > 4 {
            return None;
        }
        // Reject if any character is not an octal digit (0-7).
        if !trimmed.chars().all(|c| ('0'..='7').contains(&c)) {
            return None;
        }
        u32::from_str_radix(trimmed, 8).ok().map(|mode| Self {
            mode: mode & 0o7777,
        })
    }
}

fn triad_char(mode: u32, bit: u32, present: char) -> char {
    if mode & bit != 0 {
        present
    } else {
        '-'
    }
}

fn exec_char(
    mode: u32,
    exec_bit: u32,
    special_set: bool,
    set_char: char,
    set_no_exec: char,
) -> char {
    match (mode & exec_bit != 0, special_set) {
        (true, true) => set_char,
        (false, true) => set_no_exec,
        (true, false) => 'x',
        (false, false) => '-',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rwxr_xr_x_from_0755() {
        let perms = FilePermissions::from_mode(0o755);
        assert_eq!(perms.octal_string(), "0755");
        assert_eq!(perms.symbolic_string(), "rwxr-xr-x");
        assert!(perms.is_executable());
    }

    #[test]
    fn rw_r_r_from_0644() {
        let perms = FilePermissions::from_mode(0o644);
        assert_eq!(perms.symbolic_string(), "rw-r--r--");
        assert!(!perms.is_executable());
    }

    #[test]
    fn setuid_bit_renders_lowercase_s_when_executable() {
        let perms = FilePermissions::from_mode(0o4755);
        assert_eq!(perms.octal_string(), "4755");
        assert_eq!(perms.symbolic_string(), "rwsr-xr-x");
    }

    #[test]
    fn setuid_bit_renders_uppercase_s_without_exec() {
        let perms = FilePermissions::from_mode(0o4655);
        assert_eq!(perms.symbolic_string(), "rwSr-xr-x");
    }

    #[test]
    fn sticky_bit_on_world_writable_dir() {
        let perms = FilePermissions::from_mode(0o1777);
        assert_eq!(perms.symbolic_string(), "rwxrwxrwt");
    }

    #[test]
    fn mode_masks_out_file_type_bits() {
        // S_IFREG (0o100000) | 0o644 as raw st_mode input.
        let perms = FilePermissions::from_mode(0o100644);
        assert_eq!(perms.octal_string(), "0644");
    }

    #[test]
    fn read_write_getters_match_octal_bits() {
        let perms = FilePermissions::from_mode(0o640);
        assert!(perms.is_owner_readable() && perms.is_owner_writable());
        assert!(perms.is_group_readable() && !perms.is_group_writable());
        assert!(!perms.is_other_readable() && !perms.is_other_writable());
    }

    #[test]
    fn with_bit_setters_toggle_independently() {
        let perms = FilePermissions::from_mode(0o644)
            .with_owner_execute(true)
            .with_other_read(false);
        assert_eq!(perms.octal_string(), "0740");
    }

    #[test]
    fn with_bit_setters_preserve_special_bits() {
        let perms = FilePermissions::from_mode(0o4755).with_other_write(true);
        assert_eq!(perms.octal_string(), "4757");
    }

    #[test]
    fn is_setuid_getter() {
        assert!(FilePermissions::from_mode(0o4755).is_setuid());
        assert!(!FilePermissions::from_mode(0o0755).is_setuid());
    }

    #[test]
    fn is_setgid_getter() {
        assert!(FilePermissions::from_mode(0o2755).is_setgid());
        assert!(!FilePermissions::from_mode(0o0755).is_setgid());
    }

    #[test]
    fn is_sticky_getter() {
        assert!(FilePermissions::from_mode(0o1777).is_sticky());
        assert!(!FilePermissions::from_mode(0o0777).is_sticky());
    }

    #[test]
    fn with_setuid_setter() {
        let perms = FilePermissions::from_mode(0o755).with_setuid(true);
        assert_eq!(perms.octal_string(), "4755");
        assert!(perms.is_setuid());

        let perms = FilePermissions::from_mode(0o4755).with_setuid(false);
        assert_eq!(perms.octal_string(), "0755");
        assert!(!perms.is_setuid());
    }

    #[test]
    fn with_setgid_setter() {
        let perms = FilePermissions::from_mode(0o755).with_setgid(true);
        assert_eq!(perms.octal_string(), "2755");
        assert!(perms.is_setgid());

        let perms = FilePermissions::from_mode(0o2755).with_setgid(false);
        assert_eq!(perms.octal_string(), "0755");
        assert!(!perms.is_setgid());
    }

    #[test]
    fn with_sticky_setter() {
        let perms = FilePermissions::from_mode(0o777).with_sticky(true);
        assert_eq!(perms.octal_string(), "1777");
        assert!(perms.is_sticky());

        let perms = FilePermissions::from_mode(0o1777).with_sticky(false);
        assert_eq!(perms.octal_string(), "0777");
        assert!(!perms.is_sticky());
    }

    #[test]
    fn parse_octal_three_digit() {
        let perms = FilePermissions::parse_octal("755").unwrap();
        assert_eq!(perms.octal_string(), "0755");
        assert_eq!(perms.mode(), 0o755);
    }

    #[test]
    fn parse_octal_four_digit() {
        let perms = FilePermissions::parse_octal("0755").unwrap();
        assert_eq!(perms.octal_string(), "0755");
        assert_eq!(perms.mode(), 0o755);
    }

    #[test]
    fn parse_octal_with_special_bits() {
        let perms = FilePermissions::parse_octal("4755").unwrap();
        assert_eq!(perms.octal_string(), "4755");
        assert!(perms.is_setuid());

        let perms = FilePermissions::parse_octal("1777").unwrap();
        assert_eq!(perms.octal_string(), "1777");
        assert!(perms.is_sticky());
    }

    #[test]
    fn parse_octal_roundtrip() {
        for mode in [0o644, 0o755, 0o4755, 0o2755, 0o1777] {
            let perms1 = FilePermissions::from_mode(mode);
            let octal = perms1.octal_string();
            let perms2 = FilePermissions::parse_octal(&octal).unwrap();
            assert_eq!(perms1, perms2);
        }
    }

    #[test]
    fn parse_octal_rejects_non_octal_digits() {
        assert!(FilePermissions::parse_octal("999").is_none());
        assert!(FilePermissions::parse_octal("888").is_none());
    }

    #[test]
    fn parse_octal_rejects_invalid_input() {
        assert!(FilePermissions::parse_octal("").is_none());
        assert!(FilePermissions::parse_octal("   ").is_none());
        assert!(FilePermissions::parse_octal("12345").is_none());
        assert!(FilePermissions::parse_octal("abc").is_none());
        assert!(FilePermissions::parse_octal("77x").is_none());
    }
}
