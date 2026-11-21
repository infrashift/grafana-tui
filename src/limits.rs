//! Named limits that give the memory of the program a ceiling (ADR-0002, rule R3).
//!
//! The program checks each limit where the data comes in. Other modules keep
//! the limits that only they use, such as `MAX_RESPONSE_BYTES` in `prom`.

use anyhow::{Result, ensure};
use std::io::Read;
use std::path::Path;

/// Largest dashboard file: the largest example has 148 KiB, and large public dashboards have about 1 MiB.
pub(crate) const MAX_DASHBOARD_BYTES: u64 = 16 * 1024 * 1024;

/// Largest config file: a config with many custom themes has a few KiB.
pub(crate) const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Largest PEM file: a full public CA bundle has about 220 KiB.
pub(crate) const MAX_PEM_BYTES: u64 = 1024 * 1024;

/// Most dashboard files for one start: each file is a tab, and the largest example set has 8.
pub(crate) const MAX_DASHBOARD_FILES: usize = 64;

/// Most user variables from `--var` or from the config: a dashboard seldom has more than 30.
pub(crate) const MAX_USER_VARS: usize = 256;

/// Reads the file at `path`, and fails when it has more than `limit` bytes.
///
/// The check uses the length in the metadata first. The read stops after
/// `limit + 1` bytes, so a file that grows during the read also fails.
pub(crate) fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    check_file_size(file.metadata()?.len(), limit)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    check_file_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX), limit)?;
    Ok(bytes)
}

/// Reads the UTF-8 text file at `path`, and fails when it has more than `limit` bytes.
pub(crate) fn read_text_capped(path: &Path, limit: u64) -> Result<String> {
    Ok(String::from_utf8(read_capped(path, limit)?)?)
}

/// Fails when a file of `size` bytes is larger than `limit`.
fn check_file_size(size: u64, limit: u64) -> Result<()> {
    ensure!(
        size <= limit,
        "the file is larger than the limit of {limit} bytes"
    );
    Ok(())
}

/// Fails when `count` items of `what` are more than `limit`.
pub(crate) fn check_count(count: usize, limit: usize, what: &str) -> Result<()> {
    ensure!(
        count <= limit,
        "{count} {what} are more than the limit of {limit}"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A file in the temp directory, which the test removes.
    pub(crate) fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("grafana-tui-limits-{}-{name}", std::process::id()))
    }

    /// A sparse file of `len` bytes, which uses almost no disk space.
    pub(crate) fn sparse_file(name: &str, len: u64) -> std::path::PathBuf {
        let path = temp_file(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(len).unwrap();
        path
    }

    #[test]
    fn a_file_at_the_limit_reads() {
        let path = temp_file("at-limit");
        std::fs::write(&path, "abcd").unwrap();
        assert_eq!(read_text_capped(&path, 4).unwrap(), "abcd");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_file_over_the_limit_fails_before_the_read() {
        let path = sparse_file("over-limit", 5);
        let error = read_capped(&path, 4).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the file is larger than the limit of 4 bytes"
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_grows_during_the_read_fails() {
        // A device has a metadata length of 0, but the read does not end.
        let error = read_capped(Path::new("/dev/zero"), 4).unwrap_err();
        assert!(error.to_string().contains("limit of 4 bytes"));
    }

    #[test]
    fn a_missing_file_and_bad_text_fail() {
        assert!(read_capped(&temp_file("missing"), 4).is_err());
        let path = temp_file("not-utf8");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(read_text_capped(&path, 4).is_err());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn counts_over_the_limit_fail() {
        assert!(check_count(2, 2, "files").is_ok());
        assert_eq!(
            check_count(3, 2, "files").unwrap_err().to_string(),
            "3 files are more than the limit of 2"
        );
    }
}
