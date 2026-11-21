use std::path::PathBuf;

mod command;
mod details;
mod diagnostics;
mod filter;
mod jsonl;
mod modal;
mod model;
mod projection;
mod provider;

pub(crate) use details::format_cluster_detail_lines;
pub(crate) use details::format_event_time;
pub(crate) use diagnostics::{AnnotationTargetWarning, target_warnings};
#[cfg(test)]
pub(crate) use filter::tag_catalogue;
pub(crate) use filter::{TagCatalogueEntry, TagFilter};
pub(crate) use modal::TagFilterModalState;
pub(crate) use modal::{AnnotationModal, ClusterModalState, visible_range};
pub(crate) use model::{
    AnnotationEvent, AnnotationPanelContext, AnnotationSnapshot, AnnotationTarget,
};
pub(crate) use projection::{AnnotationCluster, cluster_projected_events};
#[cfg(test)]
pub(crate) use provider::test_provider::TestProvider;
pub(crate) use provider::{
    AnnotationCommandConfig, AnnotationProvider, AnnotationProviderError, AnnotationRefreshContext,
    AnnotationSourceConfig, DEFAULT_COMMAND_TIMEOUT, ProviderPoll, build_provider,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AnnotationStatus {
    loaded_events: usize,
    source_warning: Option<String>,
    target_warnings: Vec<AnnotationTargetWarning>,
}

#[derive(Debug)]
pub(crate) enum AnnotationState {
    Disabled,
    Active {
        provider: Option<AnnotationProvider>,
        snapshot: AnnotationSnapshot,
        filter: TagFilter,
        visible: bool,
        status: AnnotationStatus,
    },
}

impl AnnotationState {
    pub(crate) fn from_provider(provider: Option<AnnotationProvider>) -> Self {
        let configured = provider.is_some();
        let state = match provider {
            Some(provider) => Self::Active {
                provider: Some(provider),
                snapshot: AnnotationSnapshot::new(Vec::new()),
                filter: TagFilter::default(),
                visible: true,
                status: AnnotationStatus::default(),
            },
            None => Self::Disabled,
        };
        debug_assert!(
            matches!(state, Self::Active { .. }) == configured,
            "only a provider turns the annotations on"
        );
        debug_assert!(
            !matches!(&state, Self::Active { snapshot, .. } if snapshot.len() > 0),
            "a new annotation state has no events"
        );
        state
    }

    pub(crate) fn from_source(source: Option<AnnotationSourceConfig>) -> Self {
        Self::from_provider(source.map(build_provider))
    }

    pub(crate) fn from_path(path: Option<PathBuf>) -> Self {
        Self::from_source(path.map(AnnotationSourceConfig::File))
    }

    /// Takes the provider out so a refresh can run in the background. Returns
    /// `None` when annotations are off or a refresh already holds it.
    pub(crate) fn take_provider(&mut self) -> Option<AnnotationProvider> {
        let taken = match self {
            Self::Active { provider, .. } => provider.take(),
            Self::Disabled => None,
        };
        debug_assert!(
            matches!(self, Self::Disabled | Self::Active { provider: None, .. }),
            "a refresh holds the provider"
        );
        debug_assert!(
            taken.is_none() || matches!(self, Self::Active { .. }),
            "only active annotations have a provider"
        );
        taken
    }

    /// Returns true when the loaded event count is inside `MAX_EVENTS`.
    fn within_event_limit(&self) -> bool {
        !matches!(self, Self::Active { status, .. } if status.loaded_events > model::MAX_EVENTS)
    }

    /// Returns the provider taken by `take_provider` and applies its result.
    /// Returns whether it loaded a new snapshot.
    pub(crate) fn finish_refresh(
        &mut self,
        returned: AnnotationProvider,
        poll: ProviderPoll,
    ) -> bool {
        debug_assert!(
            !matches!(&poll, ProviderPoll::Loaded(snapshot) if snapshot.len() > model::MAX_EVENTS),
            "a loaded snapshot holds MAX_EVENTS events or fewer"
        );
        if let Self::Active { provider, .. } = self {
            *provider = Some(returned);
        }

        let loaded = match (&mut *self, poll) {
            (
                Self::Active {
                    snapshot, status, ..
                },
                ProviderPoll::Loaded(next_snapshot),
            ) => {
                *snapshot = next_snapshot;
                status.loaded_events = snapshot.len();
                status.source_warning = None;
                true
            }
            (Self::Active { status, .. }, ProviderPoll::Failed(error)) => {
                status.source_warning = Some(format!(
                    "{error}; using {} previous event(s)",
                    status.loaded_events
                ));
                false
            }
            (_, ProviderPoll::Unchanged) | (Self::Disabled, _) => false,
        };
        debug_assert!(
            self.within_event_limit(),
            "the status counts MAX_EVENTS events or fewer"
        );
        debug_assert!(
            !loaded
                || matches!(self, Self::Active { snapshot, status, .. }
                    if status.loaded_events == snapshot.len()),
            "the status counts the events of the new snapshot"
        );
        debug_assert!(
            !loaded
                || matches!(self, Self::Active { status, .. } if status.source_warning.is_none()),
            "a new snapshot clears the source warning"
        );
        loaded
    }

    pub(crate) fn reconcile_targets(&mut self, eligible_panel_titles: &[String]) {
        debug_assert!(
            self.within_event_limit(),
            "the status counts MAX_EVENTS events or fewer"
        );
        if let Self::Active {
            snapshot, status, ..
        } = self
        {
            status.target_warnings = target_warnings(snapshot, eligible_panel_titles);
            debug_assert!(
                status.target_warnings.iter().all(|warning| match warning {
                    AnnotationTargetWarning::MissingPanel { title } =>
                        !eligible_panel_titles.contains(title),
                    AnnotationTargetWarning::AmbiguousPanel { matches, .. } => *matches >= 2,
                }),
                "each warning names a title that does not match exactly one panel"
            );
        }
    }

    pub(crate) fn footer_status(&self) -> Option<String> {
        let footer = match self {
            Self::Disabled => None,
            Self::Active { filter, status, .. } => {
                let mut parts = status
                    .source_warning
                    .iter()
                    .cloned()
                    .chain(status.target_warnings.iter().map(ToString::to_string))
                    .collect::<Vec<_>>();
                let warning_count = parts.len();
                parts.truncate(1);
                if let Some(first) = parts.first_mut().filter(|_| warning_count > 1) {
                    first.push_str(&format!(" (+{} more)", warning_count - 1));
                }
                if !filter.is_empty() {
                    parts.push(format!("tags {}", filter.summary()));
                }
                debug_assert!(
                    parts.len() <= 2,
                    "the footer shows the first warning and the tag filter at most"
                );
                (!parts.is_empty()).then(|| parts.join(" | "))
            }
        };
        debug_assert!(
            footer.as_ref().is_none_or(|footer| !footer.is_empty()),
            "a footer status has text"
        );
        footer
    }

    pub(crate) fn toggle_visibility(&mut self) {
        if let Self::Active { visible, .. } = self {
            *visible = !*visible;
        }
    }

    pub(crate) fn set_filter(&mut self, next: TagFilter) {
        if let Self::Active { filter, .. } = self {
            *filter = next;
        }
    }

    pub(crate) fn applied_filter(&self) -> Option<&TagFilter> {
        match self {
            Self::Active { filter, .. } => Some(filter),
            Self::Disabled => None,
        }
    }

    pub(crate) fn new_tag_filter_modal(&self) -> Option<TagFilterModalState> {
        let modal = match self {
            Self::Disabled => None,
            Self::Active {
                snapshot, filter, ..
            } => Some(TagFilterModalState::new(
                filter::tag_catalogue(snapshot, filter),
                filter.clone(),
            )),
        };
        debug_assert!(
            modal.is_some() == matches!(self, Self::Active { .. }),
            "only active annotations open a tag filter"
        );
        debug_assert!(
            modal.as_ref().is_none_or(|modal| modal.selected() == 0),
            "a new tag filter selects the first tag"
        );
        modal
    }

    pub(crate) fn effective_filter(&self, panel: AnnotationPanelContext<'_>) -> Option<&TagFilter> {
        let _panel_index = panel.index;
        self.applied_filter()
    }

    pub(crate) fn events_for_panel(
        &self,
        panel: AnnotationPanelContext<'_>,
        bounds: [f64; 2],
    ) -> Vec<&AnnotationEvent> {
        let Some(filter) = self.effective_filter(panel) else {
            return Vec::new();
        };

        match self {
            Self::Active {
                snapshot,
                visible: true,
                ..
            } => projection::events_for_panel(snapshot, filter, panel, bounds),
            Self::Disabled | Self::Active { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
pub(crate) use test_support::test_event_at;

/// Helpers that build annotation events for tests in other modules.
#[cfg(test)]
mod test_support {
    use super::{
        AnnotationEvent, AnnotationRefreshContext, AnnotationSnapshot, AnnotationState,
        AnnotationStatus, AnnotationTarget, TagFilter,
    };

    /// The test helpers of the annotation state.
    impl AnnotationState {
        pub(crate) async fn refresh(&mut self, context: &AnnotationRefreshContext) -> bool {
            let Some(mut provider) = self.take_provider() else {
                return false;
            };
            let poll = provider.refresh(context).await;
            self.finish_refresh(provider, poll)
        }

        pub(crate) fn snapshot(&self) -> Option<&AnnotationSnapshot> {
            match self {
                Self::Disabled => None,
                Self::Active { snapshot, .. } => Some(snapshot),
            }
        }

        pub(crate) fn is_configured(&self) -> bool {
            matches!(self, Self::Active { .. })
        }

        pub(crate) fn is_visible(&self) -> bool {
            matches!(self, Self::Active { visible: true, .. })
        }

        pub(crate) fn from_events_for_test(events: Vec<AnnotationEvent>) -> Self {
            let snapshot = AnnotationSnapshot::new(events);
            let event_count = snapshot.len();
            Self::Active {
                provider: None,
                snapshot,
                filter: TagFilter::default(),
                visible: true,
                status: AnnotationStatus {
                    loaded_events: event_count,
                    ..AnnotationStatus::default()
                },
            }
        }

        pub(crate) fn warning_for_test(message: &str) -> Self {
            Self::Active {
                provider: None,
                snapshot: AnnotationSnapshot::new(Vec::new()),
                filter: TagFilter::default(),
                visible: true,
                status: AnnotationStatus {
                    source_warning: Some(message.to_string()),
                    ..AnnotationStatus::default()
                },
            }
        }
    }

    /// Returns an event for all panels at `timestamp_secs`.
    pub(crate) fn test_event_at(timestamp_secs: f64, text: &str) -> AnnotationEvent {
        let millis = (timestamp_secs * 1_000.0).round() as i64;
        AnnotationEvent {
            time: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis).unwrap(),
            text: text.to_string(),
            tags: Vec::new(),
            target: AnnotationTarget::All,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        AnnotationPanelContext, AnnotationProvider, AnnotationProviderError,
        AnnotationRefreshContext, AnnotationSnapshot, AnnotationState, AnnotationTarget,
        ProviderPoll, TagFilter, TestProvider,
    };

    fn snapshot_with(text: &str) -> AnnotationSnapshot {
        AnnotationSnapshot::new(vec![super::test_event_at(100.0, text)])
    }

    fn refresh_context() -> AnnotationRefreshContext {
        AnnotationRefreshContext::from_unix_window(200, Duration::from_secs(100))
    }

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

    #[tokio::test]
    async fn command_state_retains_snapshot_on_failure_and_clears_warning_on_recovery() {
        let context = refresh_context();
        let provider = TestProvider::scripted(vec![
            ProviderPoll::Loaded(snapshot_with("initial")),
            ProviderPoll::Failed(AnnotationProviderError::new("command failed")),
            ProviderPoll::Loaded(AnnotationSnapshot::new(Vec::new())),
        ]);
        let mut state = AnnotationState::from_provider(Some(AnnotationProvider::Test(provider)));

        assert!(state.refresh(&context).await);
        assert_eq!(state.snapshot().unwrap().len(), 1);
        assert_eq!(state.snapshot().unwrap().events()[0].text, "initial");
        assert!(state.footer_status().is_none());

        state.refresh(&context).await;
        assert_eq!(state.snapshot().unwrap().len(), 1);
        assert!(
            state
                .footer_status()
                .unwrap()
                .contains("using 1 previous event")
        );

        assert!(state.refresh(&context).await);
        assert_eq!(state.snapshot().unwrap().len(), 0);
        assert!(state.footer_status().is_none());
    }

    #[tokio::test]
    async fn hidden_state_refreshes_provider_every_time() {
        let refresh_count = Arc::new(Mutex::new(0));
        let provider = TestProvider::scripted_recording(
            vec![ProviderPoll::Unchanged, ProviderPoll::Unchanged],
            Arc::clone(&refresh_count),
        );
        let mut state = AnnotationState::from_provider(Some(AnnotationProvider::Test(provider)));
        state.toggle_visibility();

        state.refresh(&refresh_context()).await;
        state.refresh(&refresh_context()).await;

        assert!(!state.is_visible());
        assert_eq!(*refresh_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn state_from_file_source_loads_snapshot() {
        let path = temp_path("source-construction");
        tokio::fs::write(
            &path,
            "{\"time\":\"2026-08-12T10:00:00Z\",\"text\":\"source\"}\n",
        )
        .await
        .unwrap();
        let mut state =
            AnnotationState::from_source(Some(super::AnnotationSourceConfig::File(path.clone())));

        assert!(state.refresh(&refresh_context()).await);
        assert_eq!(state.snapshot().unwrap().events()[0].text, "source");

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn state_composes_source_target_warnings_and_filter_summary() {
        let path = temp_path("state-status");
        tokio::fs::write(
            &path,
            concat!(
                r#"{"time":"2026-08-11T14:30:00Z","text":"deploy","panel_titles":["CPU","Missing"]}"#,
                "\n",
            ),
        )
        .await
        .unwrap();
        let mut state = AnnotationState::from_path(Some(path.clone()));
        let context = refresh_context();

        assert!(state.refresh(&context).await);
        state.reconcile_targets(&["CPU".to_string(), "CPU".to_string()]);
        assert_eq!(
            state.footer_status(),
            Some(
                "target \"CPU\" matches 2 graph/timeseries panels; applied to all (+1 more)"
                    .to_string()
            )
        );

        tokio::fs::write(&path, "{").await.unwrap();
        assert!(!state.refresh(&context).await);
        let source_first_status = state.footer_status().unwrap();
        assert!(source_first_status.starts_with(&format!("{}:1:", path.display())));
        assert!(source_first_status.contains("using 1 previous event(s)"));
        assert!(source_first_status.contains("(+2 more)"));

        tokio::fs::write(
            &path,
            concat!(
                r#"{"time":"2026-08-11T14:30:00Z","text":"deploy","panel_titles":["Memory"]}"#,
                "\n",
            ),
        )
        .await
        .unwrap();
        assert!(state.refresh(&context).await);
        state.reconcile_targets(&["Memory".to_string()]);
        assert_eq!(state.footer_status(), None);

        state.set_filter(TagFilter::from_selected([
            "incident".to_string(),
            "deploy".to_string(),
        ]));
        assert_eq!(
            state.footer_status(),
            Some("tags deploy|incident".to_string())
        );

        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn state_keeps_reloading_while_hidden_and_retains_snapshot_if_removed() {
        let path = temp_path("state-hidden");
        tokio::fs::write(
            &path,
            "{\"time\":\"2026-07-23T14:30:00Z\",\"text\":\"deploy\"}\n",
        )
        .await
        .unwrap();
        let mut state = AnnotationState::from_path(Some(path.clone()));
        let context = refresh_context();
        state.toggle_visibility();

        state.refresh(&context).await;
        assert_eq!(state.snapshot().unwrap().len(), 1);
        assert!(!state.is_visible());

        tokio::fs::remove_file(&path).await.unwrap();
        state.refresh(&context).await;
        assert_eq!(state.snapshot().unwrap().len(), 1);
        assert!(
            state
                .footer_status()
                .unwrap()
                .contains("using 1 previous event")
        );
    }

    #[tokio::test]
    async fn disabled_state_has_no_source_work_or_visibility() {
        let mut state = AnnotationState::from_path(None);
        let context = refresh_context();
        assert!(!state.is_configured());
        assert!(!state.is_visible());
        assert!(state.snapshot().is_none());
        assert!(state.footer_status().is_none());
        assert!(!state.refresh(&context).await);
        state.toggle_visibility();
        assert!(!state.is_visible());
    }

    #[test]
    fn active_state_applies_one_global_filter_when_routing_each_panel() {
        let mut deploy = super::test_event_at(10.0, "deploy");
        deploy.tags = vec!["deploy".to_string()];
        deploy.target = AnnotationTarget::PanelTitles(["CPU".to_string()].into_iter().collect());

        let mut incident = super::test_event_at(20.0, "incident");
        incident.tags = vec!["incident".to_string()];
        let mut state = AnnotationState::from_events_for_test(vec![deploy, incident]);
        state.set_filter(TagFilter::from_selected(["deploy".to_string()]));

        assert_eq!(state.applied_filter().unwrap().summary(), "deploy");
        assert_eq!(
            state
                .effective_filter(AnnotationPanelContext {
                    index: 0,
                    title: "CPU",
                })
                .unwrap()
                .summary(),
            "deploy"
        );
        assert_eq!(
            state
                .events_for_panel(
                    AnnotationPanelContext {
                        index: 0,
                        title: "CPU",
                    },
                    [0.0, 100.0],
                )
                .iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec!["deploy"]
        );
        assert!(
            state
                .events_for_panel(
                    AnnotationPanelContext {
                        index: 1,
                        title: "Memory",
                    },
                    [0.0, 100.0],
                )
                .is_empty()
        );
    }
}
