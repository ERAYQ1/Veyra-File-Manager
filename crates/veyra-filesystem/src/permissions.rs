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
}
