//! Query port and projection for the native active leave-policy read.
//! No SQL lives here.

use super::domain::{
    ActiveLeavePolicies, LeavePolicyHistory, LeavePolicyHistoryRow, LeavePolicySetting, MAX_ROWS,
    invalid, unavailable,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LeavePolicyFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg-backed implementation; the
/// router transports only a bounded Company identity and never sees a query.
pub trait LeavePolicyRead: Send + Sync {
    fn active<'a>(
        &'a self,
        company: &'a str,
    ) -> LeavePolicyFuture<'a, Vec<LeavePolicySetting>>;

    fn history<'a>(
        &'a self,
        company: &'a str,
    ) -> LeavePolicyFuture<'a, Vec<LeavePolicyHistoryRow>>;
}

/// Application handler: validate the bounded query, then shape the complete
/// owner payload. The projection is byte-identical to the resolved value of the
/// migrated `activeLeavePolicySettings` TS operation (`{ name, version, status }`
/// rows) so it is a stable parity oracle.
pub fn project(rows: Vec<LeavePolicySetting>) -> Result<serde_json::Value, BusinessError> {
    if rows.len() > MAX_ROWS {
        return Err(unavailable());
    }
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                serde_json::json!({
                    "name": row.name,
                    "version": row.version,
                    "status": row.status,
                })
            })
            .collect(),
    ))
}

/// Application handler for the history read. The projection is byte-identical
/// to the resolved value of the migrated `leavePolicySettingsHistory` TS
/// operation (`{ title, version, actor, at, kind }` rows).
pub fn project_history(
    rows: Vec<LeavePolicyHistoryRow>,
) -> Result<serde_json::Value, BusinessError> {
    if rows.len() > MAX_ROWS {
        return Err(unavailable());
    }
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                serde_json::json!({
                    "title": row.title,
                    "version": row.version,
                    "actor": row.actor,
                    "at": row.at,
                    "kind": row.kind,
                })
            })
            .collect(),
    ))
}

/// Validate a query before it reaches the reader. Kept in the application layer
/// so the HTTP adapter owns transport only.
pub fn validate(query: &ActiveLeavePolicies) -> Result<(), BusinessError> {
    query.validate().map_err(|_| invalid())
}

/// Same bounded validation for the history query.
pub fn validate_history(query: &LeavePolicyHistory) -> Result<(), BusinessError> {
    query.validate().map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_pins_the_full_ts_payload() {
        let rows = vec![
            LeavePolicySetting {
                name: "standard".into(),
                version: 3,
                status: "active".into(),
            },
            LeavePolicySetting {
                name: "senior".into(),
                version: 1,
                status: "active".into(),
            },
        ];
        let value = project(rows).unwrap();
        let body = serde_json::to_string(&value).unwrap();
        assert_eq!(
            body,
            r#"[{"name":"standard","status":"active","version":3},{"name":"senior","status":"active","version":1}]"#
        );
        use sha2::Digest;
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, "612db4773b9afc880d2427b30b53308a75ccaea9c5ab7bdfe4bb622c6583efc2");
    }

    #[test]
    fn oversized_reply_fails_closed() {
        let rows = (0..=MAX_ROWS)
            .map(|i| LeavePolicySetting {
                name: format!("policy-{i}"),
                version: 1,
                status: "active".into(),
            })
            .collect::<Vec<_>>();
        assert!(project(rows).is_err());
    }

    #[test]
    fn history_projection_pins_the_full_ts_payload() {
        let rows = vec![LeavePolicyHistoryRow {
            title: "Level 1".into(),
            version: 2,
            actor: "u1".into(),
            at: 1_700_000_000,
            kind: "Leave policy".into(),
        }];
        let body = serde_json::to_string(&project_history(rows).unwrap()).unwrap();
        assert_eq!(
            body,
            r#"[{"actor":"u1","at":1700000000,"kind":"Leave policy","title":"Level 1","version":2}]"#
        );
        use sha2::Digest;
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, "c3739e2484c1a7b7062db0e404e39d2467b376e50d85ebb4885eb427dce28a33");
    }

    #[test]
    fn history_reply_fails_closed_when_oversized() {
        let rows = (0..=MAX_ROWS)
            .map(|i| LeavePolicyHistoryRow {
                title: format!("policy-{i}"),
                version: 1,
                actor: "u1".into(),
                at: 1_700_000_000,
                kind: "Leave policy".into(),
            })
            .collect::<Vec<_>>();
        assert!(project_history(rows).is_err());
    }
}
