//! Collaboration-owned decisions. Source and IAM authority remain owner ports.
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationKind {
    Related,
    Duplicates,
    Blocks,
    DependsOn,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardAction {
    Complete,
    Reassign,
    Cancel,
}
/// A routing decision only: this supplies no source or mutation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterPartition {
    NativeField,
    Legacy,
}
pub fn relation_partition(left: SourceKind, right: SourceKind) -> WriterPartition {
    if matches!(
        (left, right),
        (
            SourceKind::FieldServiceOrder,
            SourceKind::FieldServiceOrder | SourceKind::SupportTicket
        ) | (SourceKind::SupportTicket, SourceKind::FieldServiceOrder)
    ) {
        WriterPartition::NativeField
    } else {
        WriterPartition::Legacy
    }
}
pub fn card_partition(source: SourceKind) -> WriterPartition {
    if source == SourceKind::FieldServiceOrder {
        WriterPartition::NativeField
    } else {
        WriterPartition::Legacy
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentRelation {
    pub id: String,
    pub version: i64,
    pub active: bool,
    pub left: SourceReference,
    pub right: SourceReference,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentCard {
    pub id: String,
    pub version: i64,
    pub open: bool,
    pub owner_id: String,
    pub due_at: i64,
    pub owner_eligible: bool,
    pub source: SourceReference,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedActor {
    pub user_id: String,
    pub company_id: String,
    pub can_review: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub id: String,
    pub version: i64,
    pub status: &'static str,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    CreateRelation {
        left: SourceReference,
        right: SourceReference,
        relation: RelationKind,
        reason: String,
    },
    RemoveRelation {
        id: String,
        expected_version: i64,
        reason: String,
    },
    CreateCard {
        source: SourceReference,
        title: String,
        owner_id: String,
        due_at: i64,
        reason: String,
    },
    ChangeCard {
        id: String,
        expected_version: i64,
        action: CardAction,
        owner_id: Option<String>,
        due_at: Option<i64>,
        evidence: Option<String>,
        reason: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerPlan {
    CreateRelation {
        left: SourceRecord,
        right: SourceRecord,
        relation: RelationKind,
        reason: String,
    },
    RemoveRelation {
        current: CurrentRelation,
        reason: String,
    },
    CreateCard {
        source: SourceRecord,
        title: String,
        owner_id: String,
        due_at: i64,
        reason: String,
    },
    ChangeCard {
        current: CurrentCard,
        action: CardAction,
        owner_id: Option<String>,
        due_at: Option<i64>,
        evidence: Option<String>,
        reason: String,
    },
}
/// Exact owner result prepared before the Workflow ledger writes a receipt.
pub fn plan_receipt(
    key: &erpx_server_shared::public::CommandKey,
    plan: &OwnerPlan,
) -> Result<Receipt, BusinessError> {
    let next = |version: i64| {
        version
            .checked_add(1)
            .filter(|v| version > 0 && *v <= 9_007_199_254_740_991)
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Conflict,
                    "COLLABORATION_SOURCE_CHANGED",
                    "The collaboration record changed. Refresh and retry.",
                )
            })
    };
    Ok(match plan {
        OwnerPlan::CreateRelation { .. } => Receipt {
            id: format!("record-relation-{}", key.as_str()),
            version: 1,
            status: "active",
        },
        OwnerPlan::RemoveRelation { current, .. } => Receipt {
            id: current.id.clone(),
            version: next(current.version)?,
            status: "removed",
        },
        OwnerPlan::CreateCard { .. } => Receipt {
            id: format!("action-card-{}", key.as_str()),
            version: 1,
            status: "open",
        },
        OwnerPlan::ChangeCard {
            current, action, ..
        } => Receipt {
            id: current.id.clone(),
            version: next(current.version)?,
            status: match action {
                CardAction::Complete => "completed",
                CardAction::Cancel => "cancelled",
                CardAction::Reassign => "open",
            },
        },
    })
}
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|c:char| matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
pub fn validate_text(value: &str, min: usize, max: usize) -> Result<(), BusinessError> {
    let n = js_trim(value).encode_utf16().count();
    if n < min || n > max {
        return Err(BusinessError::invalid(
            "COLLABORATION_INPUT_INVALID",
            "Provide a valid collaboration value.",
        ));
    }
    Ok(())
}
pub fn relation_sources(
    mut left: SourceRecord,
    mut right: SourceRecord,
    kind: RelationKind,
) -> Result<(SourceRecord, SourceRecord), BusinessError> {
    if left.company_id != right.company_id {
        return Err(BusinessError::invalid(
            "COLLABORATION_SCOPE_MISMATCH",
            "Both records must belong to the same Company.",
        ));
    }
    if left.kind == right.kind && left.id == right.id {
        return Err(BusinessError::invalid(
            "COLLABORATION_SELF_RELATION",
            "Choose two different records.",
        ));
    }
    if matches!(kind, RelationKind::Related | RelationKind::Duplicates) {
        let a = format!("{}:{}", left.kind.as_str(), left.id);
        let b = format!("{}:{}", right.kind.as_str(), right.id);
        if a.encode_utf16().cmp(b.encode_utf16()).is_gt() {
            std::mem::swap(&mut left, &mut right);
        }
    }
    Ok((left, right))
}
pub fn authorize_card_change(
    actor: &AuthorizedActor,
    card: &CurrentCard,
    action: CardAction,
    expected: i64,
) -> Result<(), BusinessError> {
    if action == CardAction::Complete && (card.owner_id != actor.user_id || !card.owner_eligible) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ACTION_CARD_OWNER_REQUIRED",
            "The assigned owner must complete this action.",
        ));
    }
    if action != CardAction::Complete && !actor.can_review {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ACTION_CARD_REVIEW_REQUIRED",
            "A reviewer must reassign or cancel this action.",
        ));
    }
    if !card.open || card.version != expected {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ACTION_CARD_STATE_CHANGED",
            "Refresh the action card.",
        ));
    }
    Ok(())
}

/// Owner facts for the pure My actions contribution. These are display facts,
/// never a source witness or permission grant for a subsequent command.
#[derive(serde::Deserialize)]
pub struct ActionWorkInput {
    pub id: String,
    pub title: String,
    pub source_type: String,
    pub source_id: String,
    pub status: String,
    pub owner_id: String,
    pub created_by: String,
    pub owner_eligible: i64,
    pub due_at: i64,
    pub updated_at: i64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionWork {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    pub href: &'static str,
    pub status: String,
    pub updated_at: i64,
    pub due_at: i64,
    pub queue: &'static str,
    pub can_act: bool,
    pub responsible_queue: &'static str,
    pub next_action: &'static str,
    pub blocker: Option<&'static str>,
    pub relationships: Vec<&'static str>,
}
pub fn action_work(row: &ActionWorkInput, user: &str, can_review: bool) -> ActionWork {
    let terminal = row.status != "open";
    let own = row.owner_id == user && row.owner_eligible > 0;
    ActionWork {
        id: row.id.clone(),
        kind: "collaboration_action",
        title: row.title.clone(),
        detail: format!("{} · {}", row.source_type, row.source_id),
        href: "/field-service/collaboration",
        status: row.status.clone(),
        updated_at: row.updated_at,
        due_at: row.due_at,
        queue: if terminal {
            "completed"
        } else if own || can_review {
            "todo"
        } else {
            "waiting"
        },
        can_act: !terminal && (own || can_review),
        responsible_queue: "Assigned action owner",
        next_action: if own {
            "Complete with evidence"
        } else {
            "Review routing and source context"
        },
        blocker: if row.owner_eligible > 0 {
            None
        } else {
            Some("Assigned owner is no longer eligible; reviewer reassignment is required")
        },
        relationships: vec![if own {
            "assigned"
        } else if row.created_by == user {
            "requested"
        } else {
            "team"
        }],
    }
}
