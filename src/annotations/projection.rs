use std::collections::BTreeMap;

use super::{AnnotationEvent, AnnotationPanelContext, AnnotationSnapshot, TagFilter};

#[derive(Debug)]
pub(crate) struct AnnotationCluster<'a> {
    pub(crate) coordinate: u32,
    pub(crate) events: Vec<&'a AnnotationEvent>,
}

pub(crate) fn events_for_panel<'a>(
    snapshot: &'a AnnotationSnapshot,
    filter: &TagFilter,
    panel: AnnotationPanelContext<'_>,
    bounds: [f64; 2],
) -> Vec<&'a AnnotationEvent> {
    snapshot
        .events()
        .iter()
        .filter(|event| {
            let timestamp = event.timestamp_secs();
            timestamp >= bounds[0]
                && timestamp <= bounds[1]
                && filter.matches(event)
                && event.target.applies_to(panel)
        })
        .collect()
}

/// Groups events by their projected coordinate, in coordinate order.
///
/// Each item is an event and its coordinate, or `None` when the event is off the plot.
pub(crate) fn cluster_projected_events<'a>(
    projected: impl IntoIterator<Item = (Option<u32>, &'a AnnotationEvent)>,
) -> Vec<AnnotationCluster<'a>> {
    let mut grouped: BTreeMap<u32, Vec<&'a AnnotationEvent>> = BTreeMap::new();
    for (coordinate, event) in projected {
        if let Some(coordinate) = coordinate {
            grouped.entry(coordinate).or_default().push(event);
        }
    }
    let clusters: Vec<AnnotationCluster<'a>> = grouped
        .into_iter()
        .map(|(coordinate, events)| AnnotationCluster { coordinate, events })
        .collect();
    debug_assert!(
        clusters
            .iter()
            .zip(clusters.iter().skip(1))
            .all(|(left, right)| left.coordinate < right.coordinate),
        "the clusters are in coordinate order, one for each coordinate"
    );
    debug_assert!(
        clusters.iter().all(|cluster| !cluster.events.is_empty()),
        "each cluster holds one event or more"
    );
    clusters
}

#[cfg(test)]
mod tests {
    use super::{cluster_projected_events, events_for_panel};
    use crate::annotations::{
        AnnotationEvent, AnnotationPanelContext, AnnotationSnapshot, AnnotationTarget,
        TagCatalogueEntry, TagFilter, tag_catalogue,
    };

    fn event(timestamp: f64, text: &str) -> AnnotationEvent {
        crate::annotations::test_event_at(timestamp, text)
    }

    #[test]
    fn routes_all_targets_and_filters_inclusive_time_bounds() {
        let snapshot = AnnotationSnapshot::new(vec![
            event(0.0, "start"),
            event(50.0, "middle"),
            event(100.0, "end"),
            event(101.0, "outside"),
        ]);

        let events = events_for_panel(
            &snapshot,
            &TagFilter::default(),
            AnnotationPanelContext {
                index: 0,
                title: "CPU",
            },
            [0.0, 100.0],
        );

        assert_eq!(
            events
                .iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec!["start", "middle", "end"]
        );
    }

    #[test]
    fn filters_by_exact_or_tags_before_panel_routing() {
        let mut deploy = event(10.0, "deploy");
        deploy.tags = vec!["deploy".to_string()];
        deploy.target = AnnotationTarget::PanelTitles(["CPU".to_string()].into_iter().collect());

        let mut incident = event(20.0, "incident");
        incident.tags = vec!["incident".to_string()];
        let snapshot = AnnotationSnapshot::new(vec![deploy, incident]);
        let filter = TagFilter::from_selected(["deploy".to_string()]);

        let cpu = events_for_panel(
            &snapshot,
            &filter,
            AnnotationPanelContext {
                index: 0,
                title: "CPU",
            },
            [0.0, 100.0],
        );
        let memory = events_for_panel(
            &snapshot,
            &filter,
            AnnotationPanelContext {
                index: 1,
                title: "Memory",
            },
            [0.0, 100.0],
        );

        assert_eq!(
            cpu.iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec!["deploy"]
        );
        assert!(memory.is_empty());
    }

    #[test]
    fn catalogue_is_available_through_annotations_facade() {
        assert_eq!(
            tag_catalogue(&AnnotationSnapshot::default(), &TagFilter::default()),
            Vec::<TagCatalogueEntry>::new()
        );
    }

    #[test]
    fn clusters_events_by_projected_coordinate_in_order() {
        let snapshot = AnnotationSnapshot::new(vec![
            event(10.0, "one"),
            event(11.0, "two"),
            event(90.0, "three"),
        ]);
        let events = events_for_panel(
            &snapshot,
            &TagFilter::default(),
            AnnotationPanelContext {
                index: 0,
                title: "CPU",
            },
            [0.0, 100.0],
        );

        let clusters = cluster_projected_events(events.into_iter().map(|event| {
            let timestamp = event.timestamp_secs();
            (Some(if timestamp < 50.0 { 4 } else { 9 }), event)
        }));

        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].coordinate, 4);
        assert_eq!(
            clusters[0]
                .events
                .iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec!["one", "two"]
        );
    }
}
