use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{
    AnnotationEvent, AnnotationProviderError, AnnotationRefreshContext, AnnotationSnapshot,
    AnnotationTarget, ProviderPoll, model::MAX_EVENTS,
};

#[derive(Deserialize)]
struct RawAnnotationEvent {
    time: String,
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    panel_titles: RawPanelTitles,
}

#[derive(Default, Deserialize)]
#[serde(untagged)]
enum RawPanelTitles {
    Null(()),
    Titles(Vec<String>),
    #[default]
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationLoadError {
    source: String,
    line: Option<usize>,
    reason: String,
}

impl AnnotationLoadError {
    #[cfg(test)]
    pub(crate) fn line(&self) -> Option<usize> {
        self.line
    }

    fn at_line(source: &str, line: usize, reason: impl Into<String>) -> Self {
        debug_assert!(!source.is_empty(), "the error names its source");
        debug_assert!(line >= 1, "line numbers start at 1");
        let error = Self {
            source: source.to_string(),
            line: Some(line),
            reason: reason.into(),
        };
        debug_assert!(!error.reason.is_empty(), "the error has a reason");
        error
    }
}

impl fmt::Display for AnnotationLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_assert!(
            self.line.is_none_or(|line| line >= 1),
            "line numbers start at 1"
        );
        debug_assert!(!self.reason.is_empty(), "the error has a reason");
        match self.line {
            Some(line) => write!(formatter, "{}:{line}: {}", self.source, self.reason),
            None => write!(formatter, "{}: {}", self.source, self.reason),
        }
    }
}

impl std::error::Error for AnnotationLoadError {}

pub(crate) fn parse_jsonl(
    source: &str,
    input: &str,
) -> Result<AnnotationSnapshot, AnnotationLoadError> {
    debug_assert!(!source.is_empty(), "the events have a named source");
    // The file and the command stdout readers stop at their size limits.
    debug_assert!(
        crate::num::usize_to_u64(input.len()) <= MAX_FILE_BYTES,
        "the input is inside the annotation file limit"
    );
    let mut events = Vec::new();

    for (index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let line_number = index + 1;
        let event = parse_event(line)
            .map_err(|reason| AnnotationLoadError::at_line(source, line_number, reason))?;
        events.push(event);
    }

    let parsed = events.len();
    let snapshot = AnnotationSnapshot::new(events);
    debug_assert!(
        snapshot.len() == parsed.min(MAX_EVENTS),
        "the snapshot keeps each event up to MAX_EVENTS"
    );
    Ok(snapshot)
}

/// Parses one line of JSON into an event, or returns why the line is not valid.
fn parse_event(line: &str) -> Result<AnnotationEvent, String> {
    let raw: RawAnnotationEvent = serde_json::from_str(line).map_err(|error| {
        format!(
            "invalid annotation event (time, text, tags, and panel_titles must have valid types): {error}"
        )
    })?;
    let time = DateTime::parse_from_rfc3339(&raw.time)
        .map_err(|error| format!("time must be an RFC 3339 timestamp: {error}"))?
        .with_timezone(&Utc);

    if raw.text.trim().is_empty() {
        return Err("text must not be empty".to_string());
    }
    if raw.tags.iter().any(|tag| tag.trim().is_empty()) {
        return Err("tags must not contain empty strings".to_string());
    }

    let target = parse_target(raw.panel_titles)?;
    let event = AnnotationEvent {
        time,
        text: raw.text,
        tags: raw.tags,
        target,
    };
    debug_assert!(!event.text.trim().is_empty(), "the event has text");
    debug_assert!(
        event.tags.iter().all(|tag| !tag.trim().is_empty()),
        "each tag of the event has text"
    );
    Ok(event)
}

/// The panels that an event applies to, or why its `panel_titles` is not valid.
fn parse_target(panel_titles: RawPanelTitles) -> Result<AnnotationTarget, String> {
    let target = match panel_titles {
        RawPanelTitles::Missing => Ok(AnnotationTarget::All),
        RawPanelTitles::Null(()) => Err("panel_titles must not be null".to_string()),
        RawPanelTitles::Titles(titles) if titles.is_empty() => {
            Err("panel_titles must contain at least one title".to_string())
        }
        RawPanelTitles::Titles(titles) if titles.iter().any(|title| title.trim().is_empty()) => {
            Err("panel_titles must not contain blank strings".to_string())
        }
        RawPanelTitles::Titles(titles) => {
            Ok(AnnotationTarget::PanelTitles(titles.into_iter().collect()))
        }
    };
    debug_assert!(
        !matches!(&target, Ok(AnnotationTarget::PanelTitles(titles)) if titles.is_empty()),
        "a title target lists one title or more"
    );
    debug_assert!(
        !matches!(
            &target,
            Ok(AnnotationTarget::PanelTitles(titles))
                if titles.iter().any(|title| title.trim().is_empty())
        ),
        "each title of a target has text"
    );
    target
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceFingerprint {
    Missing,
    Present {
        len: u64,
        modified: std::time::SystemTime,
    },
}

/// The largest annotation file that the provider reads. It rejects a larger file with a warning.
pub(crate) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct JsonlFileProvider {
    path: PathBuf,
    last_seen: Option<SourceFingerprint>,
    max_bytes: u64,
}

impl JsonlFileProvider {
    pub(crate) fn new(path: PathBuf) -> Self {
        let provider = Self {
            path,
            last_seen: None,
            max_bytes: MAX_FILE_BYTES,
        };
        debug_assert!(
            provider.last_seen.is_none(),
            "a new provider has not seen the file"
        );
        provider
    }

    /// Reads the file, and fails when it gets past `max_bytes`. The limit applies
    /// to the bytes that this function reads, not to the size seen before. So the
    /// limit also holds for a file that grows in the meantime.
    async fn read_limited(&self) -> std::io::Result<Option<String>> {
        use tokio::io::AsyncReadExt;

        let file = tokio::fs::File::open(&self.path).await?;
        let mut bytes = Vec::new();
        file.take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .await?;
        let read = crate::num::usize_to_u64(bytes.len());
        debug_assert!(
            read <= self.max_bytes + 1,
            "the read stops one byte past the limit"
        );
        if read > self.max_bytes {
            return Ok(None);
        }
        let text = String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        debug_assert!(
            crate::num::usize_to_u64(text.len()) <= self.max_bytes,
            "the text is inside the file size limit"
        );
        Ok(Some(text))
    }

    fn too_large(&self) -> AnnotationProviderError {
        debug_assert!(
            self.last_seen.is_some(),
            "the provider records the file before it reports its size"
        );
        debug_assert!(
            self.max_bytes <= MAX_FILE_BYTES,
            "the file size limit is not more than MAX_FILE_BYTES"
        );
        AnnotationProviderError::new(
            AnnotationLoadError {
                source: self.path.display().to_string(),
                line: None,
                reason: format!(
                    "annotation file exceeds the {} MiB limit",
                    self.max_bytes / (1024 * 1024)
                ),
            }
            .to_string(),
        )
    }

    async fn poll_file(&mut self) -> ProviderPoll {
        debug_assert!(self.max_bytes > 0, "the file size limit is not zero");
        let fingerprint = match tokio::fs::metadata(&self.path).await {
            Ok(metadata) => match metadata.modified() {
                Ok(modified) => SourceFingerprint::Present {
                    len: metadata.len(),
                    modified,
                },
                Err(error) => return ProviderPoll::Failed(self.io_error(error)),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                SourceFingerprint::Missing
            }
            Err(error) => return ProviderPoll::Failed(self.io_error(error)),
        };

        if self.last_seen.as_ref() == Some(&fingerprint) {
            return ProviderPoll::Unchanged;
        }

        let poll = match fingerprint {
            SourceFingerprint::Missing => {
                self.last_seen = Some(SourceFingerprint::Missing);
                ProviderPoll::Failed(AnnotationProviderError::new(
                    AnnotationLoadError {
                        source: self.path.display().to_string(),
                        line: None,
                        reason: "annotation file does not exist".to_string(),
                    }
                    .to_string(),
                ))
            }
            SourceFingerprint::Present { len, .. } if len > self.max_bytes => {
                self.last_seen = Some(fingerprint);
                ProviderPoll::Failed(self.too_large())
            }
            present @ SourceFingerprint::Present { .. } => self.load(present).await,
        };
        debug_assert!(
            !matches!(poll, ProviderPoll::Loaded(_)) || self.last_seen.is_some(),
            "the provider records the fingerprint of each file that it loads"
        );
        poll
    }

    /// Reads and parses the file that has the `fingerprint`, and records the
    /// fingerprint when the read gives a result.
    async fn load(&mut self, fingerprint: SourceFingerprint) -> ProviderPoll {
        debug_assert!(
            matches!(fingerprint, SourceFingerprint::Present { len, .. } if len <= self.max_bytes),
            "only a file inside the size limit is read"
        );
        debug_assert!(
            !self.path.as_os_str().is_empty(),
            "a file with metadata has a path"
        );
        let poll = match self.read_limited().await {
            Ok(None) => {
                self.last_seen = Some(fingerprint);
                ProviderPoll::Failed(self.too_large())
            }
            Ok(Some(input)) => {
                self.last_seen = Some(fingerprint);
                match parse_jsonl(&self.path.display().to_string(), &input) {
                    Ok(snapshot) => ProviderPoll::Loaded(snapshot),
                    Err(error) => {
                        ProviderPoll::Failed(AnnotationProviderError::new(error.to_string()))
                    }
                }
            }
            Err(error) => ProviderPoll::Failed(self.io_error(error)),
        };
        debug_assert!(
            !matches!(&poll, ProviderPoll::Unchanged),
            "a read of a changed file loads events or fails"
        );
        poll
    }

    fn io_error(&self, error: std::io::Error) -> AnnotationProviderError {
        AnnotationProviderError::new(
            AnnotationLoadError {
                source: self.path.display().to_string(),
                line: None,
                reason: format!("could not read annotation file: {error}"),
            }
            .to_string(),
        )
    }
}

impl JsonlFileProvider {
    /// Loads the file when it changed since the last refresh. The file has events for each window.
    pub(crate) async fn refresh(&mut self, _context: &AnnotationRefreshContext) -> ProviderPoll {
        self.poll_file().await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::annotations::{AnnotationRefreshContext, AnnotationTarget, ProviderPoll};

    use super::{JsonlFileProvider, parse_jsonl};

    fn temp_path(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "grafana-tui-annotations-{name}-{}-{suffix}.jsonl",
            std::process::id()
        ))
    }

    fn refresh_context() -> AnnotationRefreshContext {
        AnnotationRefreshContext::from_unix_window(200, Duration::from_secs(100))
    }

    #[test]
    fn parses_valid_jsonl_and_ignores_blank_lines_and_unknown_fields() {
        let input = concat!(
            "\n",
            r#"{"time":"2026-07-23T14:30:00+02:00","text":"deploy","tags":["prod"],"extra":1}"#,
            "\n",
            r#"{"time":"2026-07-23T13:00:00Z","text":"rollback"}"#,
            "\n",
        );

        let snapshot = parse_jsonl("events.jsonl", input).unwrap();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot.events()[0].text, "deploy");
        assert_eq!(snapshot.events()[0].tags, vec!["prod"]);
        assert_eq!(snapshot.events()[1].tags, Vec::<String>::new());
    }

    #[test]
    fn parses_optional_panel_titles_and_deduplicates_them() {
        let snapshot = parse_jsonl(
            "events.jsonl",
            concat!(
                r#"{"time":"2026-08-11T14:30:00Z","text":"global"}"#,
                "\n",
                r#"{"time":"2026-08-11T14:31:00Z","text":"targeted","panel_titles":["CPU","CPU","Errors"]}"#,
            ),
        )
        .unwrap();

        assert_eq!(snapshot.events()[0].target, AnnotationTarget::All);
        assert_eq!(
            snapshot.events()[1].target,
            AnnotationTarget::PanelTitles(
                ["CPU".to_string(), "Errors".to_string()]
                    .into_iter()
                    .collect()
            )
        );
    }

    #[test]
    fn rejects_empty_or_blank_panel_titles_with_line_numbers() {
        for input in [
            r#"{"time":"2026-08-11T14:30:00Z","text":"x","panel_titles":[]}"#,
            r#"{"time":"2026-08-11T14:30:00Z","text":"x","panel_titles":["  "]}"#,
        ] {
            let error = parse_jsonl("events.jsonl", input).unwrap_err();
            assert_eq!(error.line(), Some(1));
            assert!(error.to_string().contains("panel_titles"));
        }
    }

    #[test]
    fn rejects_null_panel_titles_with_line_number() {
        let error = parse_jsonl(
            "events.jsonl",
            r#"{"time":"2026-08-11T14:30:00Z","text":"x","panel_titles":null}"#,
        )
        .unwrap_err();

        assert_eq!(error.line(), Some(1));
        assert!(error.to_string().contains("panel_titles"));
    }

    #[test]
    fn rejects_invalid_required_fields_with_line_number() {
        let error = parse_jsonl(
            "events.jsonl",
            "{\"time\":1700000000000,\"text\":\"deploy\"}\n",
        )
        .unwrap_err();

        assert_eq!(error.line(), Some(1));
        assert!(error.to_string().contains("events.jsonl:1"));
        assert!(error.to_string().contains("time"));
    }

    #[test]
    fn rejects_blank_text_and_tags() {
        let blank_text = parse_jsonl(
            "events.jsonl",
            r#"{"time":"2026-07-23T14:30:00Z","text":"  "}"#,
        )
        .unwrap_err();
        assert!(blank_text.to_string().contains("text must not be empty"));

        let blank_tag = parse_jsonl(
            "events.jsonl",
            r#"{"time":"2026-07-23T14:30:00Z","text":"deploy","tags":[""]}"#,
        )
        .unwrap_err();
        assert!(
            blank_tag
                .to_string()
                .contains("tags must not contain empty strings")
        );
    }

    #[tokio::test]
    async fn loads_only_after_metadata_changes() {
        let path = temp_path("changed");
        tokio::fs::write(
            &path,
            "{\"time\":\"2026-07-23T14:30:00Z\",\"text\":\"deploy\"}\n",
        )
        .await
        .unwrap();
        let mut provider = JsonlFileProvider::new(path.clone());
        let context = refresh_context();

        assert!(
            matches!(provider.refresh(&context).await, ProviderPoll::Loaded(snapshot) if snapshot.len() == 1)
        );
        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Unchanged
        ));

        tokio::fs::write(&path, "").await.unwrap();
        assert!(
            matches!(provider.refresh(&context).await, ProviderPoll::Loaded(snapshot) if snapshot.len() == 0)
        );

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn oversized_files_are_rejected_without_rereading() {
        let path = temp_path("oversized");
        std::fs::write(
            &path,
            r#"{"time":"2026-07-23T13:00:00Z","text":"too much"}"#,
        )
        .unwrap();
        let mut provider = JsonlFileProvider::new(path.clone());
        provider.max_bytes = 16;

        let ProviderPoll::Failed(error) = provider.refresh(&refresh_context()).await else {
            panic!("expected a size warning");
        };
        let unchanged = provider.refresh(&refresh_context()).await;

        assert!(error.to_string().contains("limit"), "{error}");
        assert!(matches!(unchanged, ProviderPoll::Unchanged));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn snapshots_keep_only_the_newest_events() {
        use crate::annotations::model::{AnnotationSnapshot, MAX_EVENTS};

        let events = (0..MAX_EVENTS + 3)
            .rev()
            .map(|second| crate::annotations::test_event_at(second as f64, "event"))
            .collect();

        let snapshot = AnnotationSnapshot::new(events);

        assert_eq!(snapshot.len(), MAX_EVENTS);
        assert_eq!(snapshot.events()[0].time.timestamp(), 3);
    }

    #[tokio::test]
    async fn reports_missing_then_loads_when_file_appears() {
        let path = temp_path("appears");
        let mut provider = JsonlFileProvider::new(path.clone());
        let context = refresh_context();

        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Failed(_)
        ));
        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Unchanged
        ));

        tokio::fs::write(
            &path,
            "{\"time\":\"2026-07-23T14:30:00Z\",\"text\":\"deploy\"}\n",
        )
        .await
        .unwrap();
        assert!(
            matches!(provider.refresh(&context).await, ProviderPoll::Loaded(snapshot) if snapshot.len() == 1)
        );

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_replacement_is_reported_as_failed() {
        let path = temp_path("invalid");
        tokio::fs::write(
            &path,
            "{\"time\":\"2026-07-23T14:30:00Z\",\"text\":\"deploy\"}\n",
        )
        .await
        .unwrap();
        let mut provider = JsonlFileProvider::new(path.clone());
        let context = refresh_context();
        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Loaded(_)
        ));

        tokio::fs::write(&path, "{\"time\":").await.unwrap();
        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Failed(_)
        ));
        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Unchanged
        ));

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn retries_read_failure_when_metadata_fingerprint_is_unchanged() {
        let path = temp_path("read-retry");
        let valid = b"{\"time\":\"2026-07-23T14:30:00Z\",\"text\":\"deploy\"}\n";
        let mut invalid_utf8 = valid.to_vec();
        let text_start = invalid_utf8
            .windows(b"deploy".len())
            .position(|window| window == b"deploy")
            .unwrap();
        invalid_utf8[text_start] = 0xff;
        tokio::fs::write(&path, invalid_utf8).await.unwrap();
        let initial_metadata = tokio::fs::metadata(&path).await.unwrap();
        let initial_modified = initial_metadata.modified().unwrap();
        let mut provider = JsonlFileProvider::new(path.clone());
        let context = refresh_context();

        assert!(
            matches!(provider.refresh(&context).await, ProviderPoll::Failed(error) if error.to_string().contains("could not read annotation file"))
        );

        tokio::fs::write(&path, valid).await.unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(initial_modified)
            .unwrap();
        let recovered_metadata = tokio::fs::metadata(&path).await.unwrap();
        assert_eq!(recovered_metadata.len(), initial_metadata.len());
        assert_eq!(recovered_metadata.modified().unwrap(), initial_modified);

        assert!(matches!(
            provider.refresh(&context).await,
            ProviderPoll::Loaded(snapshot) if snapshot.len() == 1
        ));

        tokio::fs::remove_file(path).await.unwrap();
    }
}
