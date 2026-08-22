//! `ArchiveFormat`: the archive kinds Faz 19 reads and writes, plus
//! extension-based detection shared by extraction (dispatch) and the
//! compress dialog (format picker / default file name).

/// A supported archive container/compression combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    SevenZip,
}

impl ArchiveFormat {
    /// Detects the format from a file name's extension, checking the
    /// longer double extensions (`.tar.gz`) before the single ones so
    /// `.tar.gz` isn't misread as a bare `.gz`.
    pub fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            Some(Self::TarXz)
        } else if lower.ends_with(".tar.zst") {
            Some(Self::TarZst)
        } else if lower.ends_with(".tar") {
            Some(Self::Tar)
        } else if lower.ends_with(".zip") {
            Some(Self::Zip)
        } else if lower.ends_with(".7z") {
            Some(Self::SevenZip)
        } else {
            None
        }
    }

    /// The canonical file extension (with leading dot) for this format,
    /// used to build the default output file name in the compress dialog.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Zip => ".zip",
            Self::Tar => ".tar",
            Self::TarGz => ".tar.gz",
            Self::TarXz => ".tar.xz",
            Self::TarZst => ".tar.zst",
            Self::SevenZip => ".7z",
        }
    }

    /// Human-readable label for the format picker.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Zip => "ZIP (.zip)",
            Self::Tar => "TAR (.tar)",
            Self::TarGz => "TAR.GZ (.tar.gz)",
            Self::TarXz => "TAR.XZ (.tar.xz)",
            Self::TarZst => "TAR.ZST (.tar.zst)",
            Self::SevenZip => "7-Zip (.7z)",
        }
    }

    pub const ALL: [ArchiveFormat; 6] = [
        Self::Zip,
        Self::Tar,
        Self::TarGz,
        Self::TarXz,
        Self::TarZst,
        Self::SevenZip,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_double_extensions_before_single_ones() {
        assert_eq!(
            ArchiveFormat::from_name("a.tar.gz"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_name("a.tgz"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_name("a.tar.xz"),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::from_name("a.tar.zst"),
            Some(ArchiveFormat::TarZst)
        );
        assert_eq!(ArchiveFormat::from_name("a.tar"), Some(ArchiveFormat::Tar));
    }

    #[test]
    fn detects_zip_and_7z_case_insensitively() {
        assert_eq!(ArchiveFormat::from_name("A.ZIP"), Some(ArchiveFormat::Zip));
        assert_eq!(
            ArchiveFormat::from_name("A.7Z"),
            Some(ArchiveFormat::SevenZip)
        );
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert_eq!(ArchiveFormat::from_name("notes.txt"), None);
        assert_eq!(ArchiveFormat::from_name("archive-of-photos"), None);
    }

    #[test]
    fn extension_round_trips_into_a_detectable_name() {
        for format in ArchiveFormat::ALL {
            let name = format!("out{}", format.extension());
            assert_eq!(ArchiveFormat::from_name(&name), Some(format));
        }
    }
}
