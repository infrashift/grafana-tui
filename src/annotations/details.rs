use super::{AnnotationCluster, AnnotationEvent, model::MAX_EVENTS};

const ELLIPSIS: char = '…';

pub(crate) fn format_cluster_detail_lines(
    cluster: &AnnotationCluster<'_>,
    character_budget: usize,
) -> [String; 2] {
    debug_assert!(
        cluster.events.len() <= MAX_EVENTS,
        "the cluster holds MAX_EVENTS events or fewer"
    );
    let Some(first) = cluster.events.first() else {
        return [String::new(), String::new()];
    };
    let heading = if cluster.events.len() == 1 {
        format_event_time(first)
    } else {
        format!(
            "{} events near {}",
            cluster.events.len(),
            first.time.format("%Y-%m-%d %H:%M:%S UTC")
        )
    };

    let lines = [
        bounded_text(&heading, character_budget),
        format_event_details(cluster, character_budget),
    ];
    debug_assert!(
        lines
            .iter()
            .all(|line| line.chars().count() <= character_budget),
        "each detail line fits the character budget"
    );
    debug_assert!(
        character_budget == 0 || lines.first().is_some_and(|heading| !heading.is_empty()),
        "the heading has text when the budget is not zero"
    );
    lines
}

pub(crate) fn format_event_time(event: &AnnotationEvent) -> String {
    let text = if event.time.timestamp_subsec_millis() == 0 {
        event.time.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        event.time.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
    };
    debug_assert!(text.ends_with(" UTC"), "the time names its time zone");
    debug_assert!(
        text.len() >= "YYYY-MM-DD HH:MM:SS UTC".len(),
        "the time has a full date and a full time of day"
    );
    text
}

fn format_event_details(cluster: &AnnotationCluster<'_>, character_budget: usize) -> String {
    let mut output = BoundedText::new(character_budget);
    for (event_index, event) in cluster.events.iter().enumerate() {
        if event_index > 0 && !output.push_str(" · ") {
            break;
        }
        if !output.push_str(&event.text) {
            break;
        }
        if event.tags.is_empty() {
            continue;
        }
        if !output.push_str(" [") {
            break;
        }
        for (tag_index, tag) in event.tags.iter().enumerate() {
            if tag_index > 0 && !output.push_str(", ") {
                return output.finish();
            }
            if !output.push_str(tag) {
                return output.finish();
            }
        }
        if !output.push_str("]") {
            break;
        }
    }
    let details = output.finish();
    debug_assert!(
        details.chars().count() <= character_budget,
        "the details fit the character budget"
    );
    debug_assert!(
        !cluster.events.is_empty() || details.is_empty(),
        "an empty cluster has no details"
    );
    details
}

fn bounded_text(value: &str, character_budget: usize) -> String {
    let mut output = BoundedText::new(character_budget);
    output.push_str(value);
    output.finish()
}

struct BoundedText {
    value: String,
    characters: usize,
    budget: usize,
    omitted: bool,
}

impl BoundedText {
    fn new(budget: usize) -> Self {
        Self {
            value: String::new(),
            characters: 0,
            budget,
            omitted: false,
        }
    }

    fn push_str(&mut self, value: &str) -> bool {
        debug_assert!(
            self.characters <= self.budget,
            "the text fits the character budget"
        );
        if self.omitted {
            return false;
        }
        for character in value.chars() {
            if self.characters == self.budget {
                self.mark_omitted();
                return false;
            }
            self.value.push(character);
            self.characters += 1;
        }
        debug_assert!(
            self.characters <= self.budget,
            "the text fits the character budget"
        );
        debug_assert!(
            self.value.chars().count() == self.characters,
            "the character count agrees with the text"
        );
        true
    }

    fn mark_omitted(&mut self) {
        debug_assert!(
            self.characters <= self.budget,
            "the text fits the character budget"
        );
        if self.omitted || self.budget == 0 {
            return;
        }
        if self.characters == self.budget {
            self.value.pop();
            self.characters -= 1;
        }
        self.value.push(ELLIPSIS);
        self.characters += 1;
        self.omitted = true;
        debug_assert!(
            self.value.ends_with(ELLIPSIS) && self.characters <= self.budget,
            "the cut text ends with an ellipsis inside the budget"
        );
    }

    fn finish(self) -> String {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{AnnotationEvent, AnnotationTarget};

    fn event(time: &str, text: &str, tags: &[&str]) -> AnnotationEvent {
        AnnotationEvent {
            time: chrono::DateTime::parse_from_rfc3339(time)
                .unwrap()
                .with_timezone(&chrono::Utc),
            text: text.to_string(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            target: AnnotationTarget::All,
        }
    }

    #[test]
    fn preserves_precise_single_event_details_when_they_fit() {
        let events = [event(
            "2026-07-23T14:30:00.125Z",
            "deploy",
            &["prod", "api"],
        )];
        let cluster = AnnotationCluster {
            coordinate: 4,
            events: events.iter().collect(),
        };

        let lines = format_cluster_detail_lines(&cluster, 80);

        assert_eq!(lines[0], "2026-07-23 14:30:00.125 UTC");
        assert_eq!(lines[1], "deploy [prod, api]");
    }

    #[test]
    fn preserves_cluster_order_and_tags_when_they_fit() {
        let events = [
            event("2026-07-23T14:30:00Z", "deploy", &["prod"]),
            event("2026-07-23T14:30:01Z", "rollback", &[]),
        ];
        let cluster = AnnotationCluster {
            coordinate: 4,
            events: events.iter().collect(),
        };

        let lines = format_cluster_detail_lines(&cluster, 80);

        assert_eq!(lines[0], "2 events near 2026-07-23 14:30:00 UTC");
        assert_eq!(lines[1], "deploy [prod] · rollback");
    }

    #[test]
    fn bounds_large_clusters_without_losing_exact_count_or_order() {
        let events = (0..100)
            .map(|index| {
                event(
                    "2026-07-23T14:30:00Z",
                    &format!("event-{index:03}-{}", "x".repeat(80)),
                    &[],
                )
            })
            .collect::<Vec<_>>();
        let cluster = AnnotationCluster {
            coordinate: 4,
            events: events.iter().collect(),
        };

        let lines = format_cluster_detail_lines(&cluster, 36);

        assert!(lines[0].starts_with("100 events near"));
        assert!(lines[0].ends_with(ELLIPSIS));
        assert!(lines[1].starts_with("event-000-"));
        assert!(lines[1].ends_with(ELLIPSIS));
        assert!(!lines[1].contains("event-001-"));
        assert!(lines.iter().all(|line| line.chars().count() <= 36));
    }
}
