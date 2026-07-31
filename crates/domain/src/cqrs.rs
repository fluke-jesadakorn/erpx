use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionCheckpoint {
    pub projection_name: String,
    pub last_event_id: Option<Uuid>,
    pub last_aggregate_version: u64,
    pub updated_at: DateTime<Utc>,
}

impl ProjectionCheckpoint {
    pub fn accepts(&self, event: &OutboxEvent) -> bool {
        event.aggregate_version > self.last_aggregate_version
    }

    pub fn advance(&mut self, event: &OutboxEvent, at: DateTime<Utc>) {
        self.last_event_id = Some(event.id);
        self.last_aggregate_version = event.aggregate_version;
        self.updated_at = at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_checkpoint_is_idempotent_for_replayed_versions() {
        let aggregate = Uuid::new_v4();
        let event = OutboxEvent {
            id: Uuid::new_v4(),
            aggregate_type: "customer".into(),
            aggregate_id: aggregate,
            aggregate_version: 2,
            event_type: "customer.updated".into(),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
        };
        let mut checkpoint = ProjectionCheckpoint {
            projection_name: "customer_360".into(),
            last_event_id: None,
            last_aggregate_version: 1,
            updated_at: Utc::now(),
        };
        assert!(checkpoint.accepts(&event));
        checkpoint.advance(&event, Utc::now());
        assert!(!checkpoint.accepts(&event));
    }
}
