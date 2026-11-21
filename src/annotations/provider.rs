use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use super::{AnnotationSnapshot, command::CommandProvider, jsonl::JsonlFileProvider};
#[cfg(test)]
use test_provider::TestProvider;

pub(crate) const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AnnotationCommandConfig {
    pub(crate) program: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(
        default = "default_command_timeout",
        deserialize_with = "deserialize_duration"
    )]
    pub(crate) timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnnotationSourceConfig {
    File(PathBuf),
    Command(AnnotationCommandConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationRefreshContext {
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
}

impl AnnotationRefreshContext {
    /// Returns the window of length `range` that ends at `end_ts`. An end out of the
    /// chrono range becomes the Unix epoch. A start out of the chrono range becomes
    /// the earliest time that chrono holds.
    pub(crate) fn from_unix_window(end_ts: i64, range: Duration) -> Self {
        let to = DateTime::<Utc>::from_timestamp(end_ts, 0).unwrap_or(DateTime::UNIX_EPOCH);
        let from = TimeDelta::from_std(range)
            .ok()
            .and_then(|span| to.checked_sub_signed(span))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        Self { from, to }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationProviderError(String);

impl AnnotationProviderError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub(crate) fn command(program: &str, reason: impl fmt::Display) -> Self {
        Self(format!("annotation command {program}: {reason}"))
    }
}

impl fmt::Display for AnnotationProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug)]
pub(crate) enum ProviderPoll {
    Unchanged,
    Loaded(AnnotationSnapshot),
    Failed(AnnotationProviderError),
}

/// The source of the annotation events. Each variant is one kind of provider.
#[derive(Debug)]
pub(crate) enum AnnotationProvider {
    /// A JSONL file that the provider reads again when it changes.
    File(JsonlFileProvider),
    /// A command that the provider runs for each refresh.
    Command(CommandProvider),
    /// A provider with a scripted behavior, for tests.
    #[cfg(test)]
    Test(TestProvider),
}

impl AnnotationProvider {
    /// Loads the events of the `context` window.
    ///
    /// The future is `Send` because each variant's future is `Send`.
    /// A refresh task runs it on the tokio runtime. A drop of the future cancels the refresh.
    pub(crate) async fn refresh(&mut self, context: &AnnotationRefreshContext) -> ProviderPoll {
        match self {
            Self::File(provider) => provider.refresh(context).await,
            Self::Command(provider) => provider.refresh(context).await,
            #[cfg(test)]
            Self::Test(provider) => provider.refresh(context).await,
        }
    }
}

/// Builds the provider for the configured annotation source.
pub(crate) fn build_provider(source: AnnotationSourceConfig) -> AnnotationProvider {
    match source {
        AnnotationSourceConfig::File(path) => {
            AnnotationProvider::File(JsonlFileProvider::new(path))
        }
        AnnotationSourceConfig::Command(config) => {
            AnnotationProvider::Command(CommandProvider::new(config))
        }
    }
}

fn default_command_timeout() -> Duration {
    DEFAULT_COMMAND_TIMEOUT
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    humantime::parse_duration(&value).map_err(serde::de::Error::custom)
}

/// The test fakes of the annotation provider, behind `AnnotationProvider::Test`.
#[cfg(test)]
pub(crate) mod test_provider {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::sync::Notify;

    use super::{AnnotationRefreshContext, AnnotationSnapshot, ProviderPoll};

    /// A provider with a scripted behavior for tests.
    #[derive(Debug)]
    pub(crate) enum TestProvider {
        /// Loads an empty snapshot on each refresh.
        Static,
        /// Returns the scripted results in order, then `Unchanged`, and counts each refresh.
        Scripted {
            /// The results that are not returned yet.
            outcomes: VecDeque<ProviderPoll>,
            /// The number of refreshes, when the test reads it.
            refresh_count: Option<Arc<Mutex<usize>>>,
        },
        /// Records the window of each refresh and returns `Unchanged`.
        WindowRecorder(Arc<Mutex<Vec<AnnotationRefreshContext>>>),
        /// Keeps the window of the last refresh and loads an empty snapshot.
        Recording(Arc<Mutex<Option<AnnotationRefreshContext>>>),
        /// Signals `provider_started`, waits for `metrics_started`, then loads an empty snapshot.
        Handshake {
            /// Notified when the refresh starts.
            provider_started: Arc<Notify>,
            /// The refresh waits for this notification.
            metrics_started: Arc<Notify>,
        },
    }

    impl TestProvider {
        /// A provider that returns `outcomes` in order.
        pub(crate) fn scripted(outcomes: Vec<ProviderPoll>) -> Self {
            Self::Scripted {
                outcomes: outcomes.into(),
                refresh_count: None,
            }
        }

        /// A provider that returns `outcomes` in order and counts each refresh in `refresh_count`.
        pub(crate) fn scripted_recording(
            outcomes: Vec<ProviderPoll>,
            refresh_count: Arc<Mutex<usize>>,
        ) -> Self {
            Self::Scripted {
                outcomes: outcomes.into(),
                refresh_count: Some(refresh_count),
            }
        }

        /// Runs the scripted behavior for the `context` window.
        pub(crate) async fn refresh(&mut self, context: &AnnotationRefreshContext) -> ProviderPoll {
            match self {
                Self::Static => ProviderPoll::Loaded(AnnotationSnapshot::new(Vec::new())),
                Self::Scripted {
                    outcomes,
                    refresh_count,
                } => {
                    if let Some(refresh_count) = refresh_count {
                        *refresh_count.lock().unwrap() += 1;
                    }
                    outcomes.pop_front().unwrap_or(ProviderPoll::Unchanged)
                }
                Self::WindowRecorder(windows) => {
                    windows.lock().unwrap().push(context.clone());
                    ProviderPoll::Unchanged
                }
                Self::Recording(captured) => {
                    *captured.lock().unwrap() = Some(context.clone());
                    ProviderPoll::Loaded(AnnotationSnapshot::new(Vec::new()))
                }
                Self::Handshake {
                    provider_started,
                    metrics_started,
                } => {
                    provider_started.notify_one();
                    metrics_started.notified().await;
                    ProviderPoll::Loaded(AnnotationSnapshot::new(Vec::new()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{
        AnnotationCommandConfig, AnnotationProvider, AnnotationRefreshContext,
        AnnotationSourceConfig, ProviderPoll, build_provider,
    };
    use crate::annotations::TestProvider;

    /// Compiles only when `future` is `Send`, as a spawned refresh task needs.
    fn assert_send<T: Send>(future: T) -> T {
        future
    }

    #[test]
    fn windows_out_of_the_chrono_range_use_documented_limits() {
        let context = AnnotationRefreshContext::from_unix_window(i64::MAX, Duration::ZERO);
        assert_eq!(context.to, chrono::DateTime::UNIX_EPOCH);
        assert_eq!(context.from, chrono::DateTime::UNIX_EPOCH);
        let context = AnnotationRefreshContext::from_unix_window(100, Duration::MAX);
        assert_eq!(context.from, chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let context = AnnotationRefreshContext::from_unix_window(100, Duration::from_secs(10));
        assert_eq!(context.from.timestamp(), 90);
    }

    #[tokio::test]
    async fn provider_refresh_futures_are_send() {
        let mut provider = AnnotationProvider::Test(TestProvider::Static);
        let context = AnnotationRefreshContext::from_unix_window(100, Duration::from_secs(10));
        assert!(matches!(
            assert_send(provider.refresh(&context)).await,
            ProviderPoll::Loaded(snapshot) if snapshot.len() == 0
        ));
    }

    fn temp_path(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "grafana-tui-provider-{name}-{}-{suffix}.jsonl",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn builds_file_provider_that_loads_jsonl() {
        let path = temp_path("construction");
        tokio::fs::write(
            &path,
            "{\"time\":\"2026-08-12T10:00:00Z\",\"text\":\"deploy\"}\n",
        )
        .await
        .unwrap();
        let mut provider = build_provider(AnnotationSourceConfig::File(path.clone()));
        let context = AnnotationRefreshContext::from_unix_window(200, Duration::from_secs(10));

        let ProviderPoll::Loaded(snapshot) = provider.refresh(&context).await else {
            panic!("expected file provider to load a snapshot");
        };
        assert_eq!(snapshot.events()[0].text, "deploy");

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn builds_command_provider_that_identifies_spawn_failure() {
        let missing_program = std::env::temp_dir()
            .join(format!(
                "grafana-tui-missing-provider-factory-{}",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned();
        let mut provider =
            build_provider(AnnotationSourceConfig::Command(AnnotationCommandConfig {
                program: missing_program.clone(),
                args: Vec::new(),
                timeout: Duration::from_secs(1),
            }));
        let context = AnnotationRefreshContext::from_unix_window(200, Duration::from_secs(10));

        let ProviderPoll::Failed(error) = provider.refresh(&context).await else {
            panic!("expected command provider spawn failure");
        };
        assert!(error.to_string().contains(&missing_program));
        assert!(error.to_string().contains("could not start"));
    }
}
