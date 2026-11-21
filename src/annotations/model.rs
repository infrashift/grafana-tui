use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnnotationTarget {
    All,
    PanelTitles(BTreeSet<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationEvent {
    pub(crate) time: DateTime<Utc>,
    pub(crate) text: String,
    pub(crate) tags: Vec<String>,
    pub(crate) target: AnnotationTarget,
}

impl AnnotationEvent {
    pub(crate) fn timestamp_secs(&self) -> f64 {
        crate::num::i64_to_f64(self.time.timestamp())
            + f64::from(self.time.timestamp_subsec_nanos()) / 1_000_000_000.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AnnotationSnapshot {
    events: Vec<AnnotationEvent>,
}

/// Most events a snapshot keeps; beyond it the oldest are dropped.
pub(crate) const MAX_EVENTS: usize = 100_000;

impl AnnotationSnapshot {
    pub(crate) fn new(mut events: Vec<AnnotationEvent>) -> Self {
        let given = events.len();
        events.sort_by_key(|event| event.time);
        if events.len() > MAX_EVENTS {
            events.drain(..events.len() - MAX_EVENTS);
        }
        debug_assert!(
            events.len() == given.min(MAX_EVENTS),
            "the snapshot drops only the oldest events past MAX_EVENTS"
        );
        debug_assert!(
            events
                .iter()
                .zip(events.iter().skip(1))
                .all(|(earlier, later)| earlier.time <= later.time),
            "the events are in time order"
        );
        Self { events }
    }

    pub(crate) fn events(&self) -> &[AnnotationEvent] {
        &self.events
    }

    pub(crate) fn len(&self) -> usize {
        self.events.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnnotationPanelContext<'a> {
    pub(crate) index: usize,
    pub(crate) title: &'a str,
}

impl AnnotationTarget {
    pub(crate) fn applies_to(&self, panel: AnnotationPanelContext<'_>) -> bool {
        debug_assert!(
            !matches!(self, Self::PanelTitles(titles) if titles.is_empty()),
            "a target lists one panel title or more"
        );
        let applies = match self {
            Self::All => true,
            Self::PanelTitles(titles) => titles.contains(panel.title),
        };
        debug_assert!(
            !applies || matches!(self, Self::All) || !panel.title.is_empty(),
            "a title target applies only to a panel with a title"
        );
        applies
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{AnnotationEvent, AnnotationPanelContext, AnnotationSnapshot, AnnotationTarget};

    fn event(time: &str, text: &str) -> AnnotationEvent {
        AnnotationEvent {
            time: DateTime::parse_from_rfc3339(time)
                .unwrap()
                .with_timezone(&Utc),
            text: text.to_string(),
            tags: Vec::new(),
            target: AnnotationTarget::All,
        }
    }

    #[test]
    fn snapshot_sorts_events_stably() {
        let first = event("2026-07-23T14:30:00Z", "first");
        let earlier = event("2026-07-23T14:00:00Z", "earlier");
        let second = event("2026-07-23T14:30:00Z", "second");

        let snapshot = AnnotationSnapshot::new(vec![first, earlier, second]);
        let texts: Vec<_> = snapshot
            .events()
            .iter()
            .map(|event| event.text.as_str())
            .collect();

        assert_eq!(texts, vec!["earlier", "first", "second"]);
    }

    #[test]
    fn timestamp_secs_preserves_fraction_beyond_milliseconds() {
        let event = event("2026-07-23T14:30:00.123456789Z", "deploy");
        let expected = event.time.timestamp() as f64 + 0.123_456_789;

        assert!((event.timestamp_secs() - expected).abs() < 1e-7);
    }

    #[test]
    fn panel_title_target_matches_exactly_and_case_sensitively() {
        let target = AnnotationTarget::PanelTitles(["CPU".to_string()].into_iter().collect());

        assert!(target.applies_to(AnnotationPanelContext {
            index: 0,
            title: "CPU",
        }));
        assert!(!target.applies_to(AnnotationPanelContext {
            index: 1,
            title: "cpu",
        }));
    }
}
