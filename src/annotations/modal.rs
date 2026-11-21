use std::ops::Range;

use super::{AnnotationEvent, TagCatalogueEntry, TagFilter, model::MAX_EVENTS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnnotationModal {
    Cluster(ClusterModalState),
    TagFilter(TagFilterModalState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClusterModalState {
    events: Vec<AnnotationEvent>,
    selected: usize,
}

impl ClusterModalState {
    pub(crate) fn new(events: Vec<AnnotationEvent>) -> Option<Self> {
        debug_assert!(
            events.len() <= MAX_EVENTS,
            "the cluster holds MAX_EVENTS events or fewer"
        );
        let modal = (!events.is_empty()).then_some(Self {
            events,
            selected: 0,
        });
        debug_assert!(
            modal
                .as_ref()
                .is_none_or(|modal| modal.selected < modal.events.len()),
            "the selected event is in the list"
        );
        modal
    }

    pub(crate) fn move_by(&mut self, delta: isize) {
        self.selected = moved_index(self.selected, self.events.len(), delta);
    }

    pub(crate) fn move_page(&mut self, direction: isize, rows: usize) {
        debug_assert!(
            self.selected < self.events.len(),
            "the selected event is in the list"
        );
        let page_delta = direction.unsigned_abs().saturating_mul(rows);
        if direction < 0 {
            self.selected = self.selected.saturating_sub(page_delta);
        } else {
            self.selected = self
                .selected
                .saturating_add(page_delta)
                .min(self.events.len().saturating_sub(1));
        }
        debug_assert!(
            self.selected < self.events.len(),
            "the selected event stays in the list"
        );
    }

    pub(crate) fn events(&self) -> &[AnnotationEvent] {
        &self.events
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_event(&self) -> Option<&AnnotationEvent> {
        self.events.get(self.selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TagFilterModalState {
    entries: Vec<TagCatalogueEntry>,
    selected: usize,
    draft: TagFilter,
}

impl TagFilterModalState {
    pub(crate) fn new(entries: Vec<TagCatalogueEntry>, draft: TagFilter) -> Self {
        debug_assert!(
            entries
                .iter()
                .zip(entries.iter().skip(1))
                .all(|(left, right)| left.tag < right.tag),
            "the catalogue lists each tag one time, in tag order"
        );
        debug_assert!(
            entries.iter().all(|entry| !entry.tag.is_empty()),
            "each catalogue tag has text"
        );
        Self {
            entries,
            selected: 0,
            draft,
        }
    }

    pub(crate) fn move_by(&mut self, delta: isize) {
        self.selected = moved_index(self.selected, self.entries.len(), delta);
    }

    pub(crate) fn toggle_selected(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            self.draft.toggle(&entry.tag);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.draft.clear();
    }

    pub(crate) fn entries(&self) -> &[TagCatalogueEntry] {
        &self.entries
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn draft(&self) -> &TagFilter {
        &self.draft
    }
}

pub(crate) fn visible_range(total: usize, selected: usize, rows: usize) -> Range<usize> {
    if total == 0 || rows == 0 {
        return 0..0;
    }
    let selected = selected.min(total - 1);
    let start = (selected / rows) * rows;
    let range = start..start.saturating_add(rows).min(total);
    debug_assert!(
        range.contains(&selected),
        "the visible rows include the selected row"
    );
    debug_assert!(
        range.end <= total && range.len() <= rows,
        "the visible rows are a page of the list or less"
    );
    range
}

fn moved_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let moved = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta.unsigned_abs()).min(len - 1)
    };
    debug_assert!(current < len, "the current index is in the list");
    debug_assert!(moved < len, "the move stays in the list");
    debug_assert!(
        if delta < 0 {
            moved <= current
        } else {
            moved >= current.min(len - 1)
        },
        "the move goes in the direction of delta"
    );
    moved
}

#[cfg(test)]
mod tests {
    use super::{ClusterModalState, TagFilterModalState, visible_range};
    use crate::annotations::{TagCatalogueEntry, TagFilter};

    #[test]
    fn cluster_modal_owns_events_and_bounds_navigation() {
        let events = vec![
            crate::annotations::test_event_at(10.0, "one"),
            crate::annotations::test_event_at(11.0, "two"),
            crate::annotations::test_event_at(12.0, "three"),
        ];
        let mut modal = ClusterModalState::new(events).unwrap();

        modal.move_by(1);
        assert_eq!(modal.selected_event().unwrap().text, "two");
        modal.move_by(100);
        assert_eq!(modal.selected_event().unwrap().text, "three");
        modal.move_page(-1, 2);
        assert_eq!(modal.selected_event().unwrap().text, "one");
    }

    #[test]
    fn cluster_modal_rejects_empty_events() {
        assert!(ClusterModalState::new(Vec::new()).is_none());
    }

    #[test]
    fn cluster_modal_clamps_extreme_page_sizes_in_each_direction() {
        let events = vec![
            crate::annotations::test_event_at(10.0, "one"),
            crate::annotations::test_event_at(11.0, "two"),
            crate::annotations::test_event_at(12.0, "three"),
        ];
        let mut modal = ClusterModalState::new(events).unwrap();

        modal.move_page(1, usize::MAX);
        assert_eq!(modal.selected_event().unwrap().text, "three");
        modal.move_page(-1, usize::MAX);
        assert_eq!(modal.selected_event().unwrap().text, "one");
        modal.move_page(isize::MAX, usize::MAX);
        assert_eq!(modal.selected_event().unwrap().text, "three");
        modal.move_page(isize::MIN, usize::MAX);
        assert_eq!(modal.selected_event().unwrap().text, "one");
    }

    #[test]
    fn tag_modal_edits_a_draft_without_changing_the_applied_filter() {
        let applied = TagFilter::from_selected(["deploy".to_string()]);
        let entries = vec![
            TagCatalogueEntry {
                tag: "deploy".to_string(),
                count: 2,
            },
            TagCatalogueEntry {
                tag: "incident".to_string(),
                count: 1,
            },
        ];
        let mut modal = TagFilterModalState::new(entries, applied.clone());

        modal.move_by(1);
        modal.toggle_selected();
        assert_eq!(applied.summary(), "deploy");
        assert_eq!(modal.draft().summary(), "deploy|incident");
        modal.clear();
        assert!(modal.draft().is_empty());
    }

    #[test]
    fn tag_modal_ignores_toggles_for_an_empty_catalogue() {
        let applied = TagFilter::from_selected(["deploy".to_string()]);
        let mut modal = TagFilterModalState::new(Vec::new(), applied.clone());

        modal.toggle_selected();

        assert_eq!(modal.draft(), &applied);
    }

    #[test]
    fn visible_range_is_empty_without_items_or_rows_and_contains_selection() {
        assert_eq!(visible_range(0, 0, 3), 0..0);
        assert_eq!(visible_range(3, 0, 0), 0..0);
        assert_eq!(visible_range(5, 3, 2), 2..4);
        assert_eq!(visible_range(5, 100, 2), 4..5);
    }
}
