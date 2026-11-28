//! Pure Work Projection navigation facts.
//!
//! This mirrors the snapshot-derived fields of `navigationSnapshot` in the
//! retained TypeScript adapter: the personal action count, the attention
//! priority list, waiting count and team-view availability. It performs no I/O
//! and grants no authority. The Home unread cursor count and routing attention
//! are owner facts supplied by the caller; they are never manufactured here.
use crate::domain::work_card_availability::{
    WorkCapabilitiesFact, WorkItemFact, WorkQueue, WorkSnapshotFact, is_my_action, merge_work_items,
};
use std::cmp::Ordering;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkAttentionFact {
    pub items: Vec<WorkItemFact>,
    pub total: usize,
    pub waiting: usize,
    pub incomplete: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkViewAvailabilityFact {
    pub team: bool,
    /// Requires the native flow enrichment (`flow.canManage && flow.current &&
    /// flow.blocker`). Callers pass the owner flow fact; this module never
    /// invents routing attention from availability rows.
    pub routing: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkNavigationFacts<'a> {
    /// Already-authorized snapshot rows; `unavailable` is the exact reader list.
    pub snapshot: &'a WorkSnapshotFact,
    pub unavailable_sources: &'a [&'a str],
    pub capabilities: &'a WorkCapabilitiesFact,
    pub home_unread: i64,
    pub include_attention: bool,
    /// Owner flow fact: any row has `flow.canManage && flow.current && flow.blocker`.
    pub routing_attention: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkNavigationFact {
    pub home: i64,
    pub work: Option<usize>,
    pub attention: Option<WorkAttentionFact>,
    pub work_view_availability: WorkViewAvailabilityFact,
}

/// Earliest due date first, then latest update and the stable record key.
fn compare_work(a: &WorkItemFact, b: &WorkItemFact) -> Ordering {
    let due = |item: &WorkItemFact| item.presentation.as_ref().and_then(|row| row.due_at).unwrap_or(i64::MAX);
    due(a).cmp(&due(b)).then_with(|| b.updated_at.cmp(&a.updated_at)).then_with(|| {
        let left = format!(
            "{}:{}:{}",
            a.kind,
            a.id,
            a.action_kind.as_deref().unwrap_or("work")
        );
        let right = format!(
            "{}:{}:{}",
            b.kind,
            b.id,
            b.action_kind.as_deref().unwrap_or("work")
        );
        left.cmp(&right)
    })
}

fn waits_with_others(item: &WorkItemFact) -> bool {
    item.queue == WorkQueue::Waiting && item.relationships.iter().any(|value| value != "team")
}

/// Mirrors `workAttention` for availability facts (including merged rows).
pub fn work_attention(items: &[WorkItemFact], incomplete: bool) -> WorkAttentionFact {
    let merged = merge_work_items(items);
    let mut attention = merged
        .iter()
        .filter(|item| is_my_action(item))
        .cloned()
        .collect::<Vec<_>>();
    let total = attention.len();
    attention.retain(|item| item.presentation.is_some());
    let incomplete = incomplete || attention.len() != total;
    attention.sort_by(compare_work);
    let waiting = merged.iter().filter(|item| waits_with_others(item)).count();
    WorkAttentionFact {
        items: attention.into_iter().take(3).collect(),
        total,
        waiting,
        incomplete,
    }
}

/// Mirrors the snapshot-derived navigation projection exactly.
pub fn work_navigation(facts: WorkNavigationFacts<'_>) -> WorkNavigationFact {
    let items = merge_work_items(&facts.snapshot.items);
    let incomplete =
        !facts.unavailable_sources.is_empty() || !facts.capabilities.unavailable.is_empty();
    let attention = facts.include_attention.then(|| work_attention(&items, incomplete));
    let work = if !facts.unavailable_sources.is_empty() || attention.as_ref().is_some_and(|value| value.incomplete) {
        None
    } else {
        Some(items.iter().filter(|item| is_my_action(item)).count())
    };
    WorkNavigationFact {
        home: facts.home_unread,
        work,
        attention,
        work_view_availability: WorkViewAvailabilityFact {
            team: facts.capabilities.team_queue
                || items
                    .iter()
                    .any(|item| item.relationships.iter().any(|value| value == "team")),
            routing: facts.routing_attention,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, updated_at: i64, queue: WorkQueue) -> WorkItemFact {
        WorkItemFact {
            id: id.into(),
            kind: "staff_task".into(),
            queue,
            can_act: None,
            action_kind: None,
            relationships: Vec::new(),
            updated_at,
            source_id: None,
            presentation: Some(crate::domain::work_card_availability::WorkItemPresentation {
                title: id.to_owned(),
                detail: "Task details".into(),
                status: "open".into(),
                due_at: None,
                responsible_queue: "Owner".into(),
                next_action: "Review".into(),
                blocker: None,
            }),
            href: format!("/tasks/{id}"),
        }
    }

    #[test]
    fn prioritizes_dated_owner_cards_and_marks_compact_only_actions_incomplete() {
        let mut soon = item("soon", 1, WorkQueue::Todo);
        soon.presentation.as_mut().unwrap().due_at = Some(100);
        let mut later = item("later", 20, WorkQueue::Todo);
        later.presentation.as_mut().unwrap().due_at = Some(200);
        let undated = item("undated", 30, WorkQueue::Todo);
        let mut compact = item("compact", 50, WorkQueue::Todo);
        compact.presentation = None;
        let attention = work_attention(&[later, compact, undated, soon], false);
        assert_eq!(
            attention.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["soon", "later", "undated"]
        );
        assert_eq!(attention.total, 4);
        assert!(attention.incomplete);
    }

    fn snapshot() -> WorkSnapshotFact {
        let mut team = item("team", 3, WorkQueue::Waiting);
        team.relationships = vec!["team".into()];
        WorkSnapshotFact {
            items: vec![
                item("later", 1, WorkQueue::Todo),
                item("first", 2, WorkQueue::Todo),
                team,
            ],
        }
    }

    #[test]
    fn compact_shape_keeps_exact_personal_count_and_team_availability() {
        let snapshot = snapshot();
        let navigation = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 4,
            include_attention: false,
            routing_attention: false,
        });
        assert_eq!(navigation.home, 4);
        assert_eq!(navigation.work, Some(2));
        assert_eq!(navigation.attention, None);
        assert_eq!(
            navigation.work_view_availability,
            WorkViewAvailabilityFact {
                team: true,
                routing: false
            }
        );
    }

    #[test]
    fn attention_items_are_priority_ordered_and_capped_at_three() {
        let snapshot = snapshot();
        let navigation = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 4,
            include_attention: true,
            routing_attention: false,
        });
        let attention = navigation.attention.expect("attention");
        assert_eq!(
            attention
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "later"]
        );
        assert_eq!(attention.total, navigation.work.unwrap());
        assert!(!attention.incomplete);
        assert_eq!(attention.waiting, 0);
    }

    #[test]
    fn missing_sources_retain_unknown_counts_only_where_the_retained_reader_does() {
        let snapshot = snapshot();
        let incomplete = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact {
                unavailable: vec!["capability unavailable".into()],
                ..Default::default()
            },
            home_unread: 4,
            include_attention: true,
            routing_attention: false,
        });
        assert_eq!(incomplete.work, None);
        assert!(incomplete.attention.expect("attention").incomplete);
        let compact = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact {
                unavailable: vec!["capability unavailable".into()],
                ..Default::default()
            },
            home_unread: 4,
            include_attention: false,
            routing_attention: false,
        });
        assert_eq!(compact.work, Some(2));
        let unavailable = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &["tasks"],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 4,
            include_attention: false,
            routing_attention: false,
        });
        assert_eq!(unavailable.work, None);
    }

    #[test]
    fn waiting_count_requires_a_non_team_relationship_like_the_retained_view() {
        let mut requested = item("requested", 1, WorkQueue::Waiting);
        requested.relationships = vec!["requested".into()];
        let mut team = item("team", 2, WorkQueue::Waiting);
        team.relationships = vec!["team".into()];
        let snapshot = WorkSnapshotFact {
            items: vec![requested, team, item("done", 3, WorkQueue::Completed)],
        };
        let attention = work_attention(&snapshot.items, false);
        assert_eq!(attention.waiting, 1);
    }

    #[test]
    fn routing_attention_is_caller_supplied_owner_flow_fact() {
        let snapshot = snapshot();
        let navigation = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 0,
            include_attention: false,
            routing_attention: true,
        });
        assert!(navigation.work_view_availability.routing);
    }
}
