# Veyra — Security Model & Vulnerability Prevention

**Document Status:** Approved Architecture  
**Scope:** Security Principles, Threat Vector Mitigation, Permission Architecture

---

## 1. Core Security Philosophy

Veyra is designed with a defense-in-depth security model tailored for modern Linux desktops. As a file manager, Veyra handles untrusted user input, arbitrary archives, complex symlink structures, external mounts, and potentially malicious files.

---

## 2. Threat Mitigation Matrix

### 2.1 Path Traversal Prevention
- **Threat:** Malicious archives (`.zip`, `.tar.gz`, `.7z`) containing relative path sequences such as `../../../../etc/shadow` designed to overwrite system files upon extraction.
- **Mitigation:**
  - Mandatory canonical path resolution and boundary checking before any file creation during extraction.
  - Reject absolute paths or paths containing `..` elements that escape the target destination directory.
  - Unit tests enforcing strict boundary isolation (`Path::starts_with` verification).

### 2.2 Symlink & TOCTOU Attack Prevention
- **Threat:** Symlink poisoning, symlink loop denial of service, and Time-of-Check to Time-of-Use (TOCTOU) race conditions where a symlink is replaced with a sensitive target between path verification and file write/deletion.
- **Mitigation:**
  - Use `O_NOFOLLOW` flag and `lstat()` for all file operation checks to ensure operations operate on the symlink itself rather than silently following it.
  - Depth-limiting for recursive directory walks to prevent symlink cycle deadlocks or stack exhaustion.

### 2.3 Privilege Isolation (Root Handling)
- **Threat:** Running full GUI applications as `root` (`sudo veyra`) exposes GTK, IPC, and X11/Wayland stacks to privilege escalation vulnerabilities.
- **Mitigation:**
  - **Rule:** Veyra MUST NOT run as root by default.
  - Privileged operations (modifying system files, formatting drives, system mounts) are delegated to isolated system helper services using Polkit / D-Bus authentication (`pkexec` / `org.freedesktop.PolicyKit1`).

### 2.4 Command & Shell Injection Prevention
- **Threat:** Malicious file paths containing shell metacharacters (e.g. `file; rm -rf ~;`, `$(whoami)`) causing code execution when invoking terminal emulators, open-with application launchers, or external scripts.
- **Mitigation:**
  - **Rule:** Zero shell execution (`sh -c`) using concatenated strings.
  - Applications and terminals must be launched using direct `std::process::Command` argument arrays or GIO `GAppInfo` APIs with explicit argument vector escaping.

### 2.5 Privacy-Preserving Logging Policy
- **Threat:** Sensitive user data leakage (passwords, auth tokens, private directory paths, file contents) into persistent system logs (`~/.cache/veyra/logs`, `journalctl`).
- **Mitigation:**
  - Production log level defaults to `INFO`.
  - Path logging in production sanitizes user home directory prefixes (`/home/username` -> `~/`).
  - Strict prohibition against logging raw file content payloads or credential strings.

---

## 3. Sandboxing & Packaging Policy

### 3.1 Flatpak Sandbox Enforcement
- Strict entitlement request justification (`--filesystem=host` vs `--filesystem=home`).
- Primary reliance on XDG Desktop Portals (`org.freedesktop.portal.FileChooser`, `org.freedesktop.portal.OpenURI`) where applicable.

### 3.2 Temp File Safety
- All temporary files created in secure XDG Runtime Directories (`$XDG_RUNTIME_DIR/veyra/`) with strict restrictive umask (`0700` permissions).
