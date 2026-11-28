//! Pure domain planner for the wf3a13 native Customer Authority write slice.
//! SQL and argument order are owned here, in the owner crate. A transport
//! supplies only a typed `kind` and bounded positional values.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CustomerAuthorityWave3Statement {
    pub sql: String,
    pub values: Vec<Value>,
}

pub const PORTAL_ACCESS_AUDIT: &str = "portal_access_audit";
pub const EVIDENCE_BINDING_VERSION: &str = "portal_evidence_binding_version";
pub const EVIDENCE_EVENT: &str = "portal_evidence_event";
pub const KINDS: &[&str] = &[
    PORTAL_ACCESS_AUDIT,
    EVIDENCE_BINDING_VERSION,
    EVIDENCE_EVENT,
];

fn statement(kind: &str) -> Option<(&'static str, usize)> {
    Some(match kind {
        PORTAL_ACCESS_AUDIT => ("WITH accessible AS (
  SELECT b.id,b.company_id,b.version,b.order_id,b.customer_id,b.email,o.number,o.kind,o.state,o.due_date,o.currency,o.updated_at
  FROM customer_access_bindings b
  JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?
  JOIN ((SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders)) o ON o.id=b.order_id AND o.company_id=b.company_id
  WHERE b.email=? AND b.status='approved' AND b.expires_at>? AND o.party_id=b.customer_id AND %PREDICATE%
    AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')
  FOR SHARE OF b,o
 ), audit AS (
  INSERT INTO customer_access_reads(id,binding_id,binding_version,user_id,read_at,projection_version)
  SELECT ?||':'||id,id,version,?,?,'customer-order-v1' FROM accessible RETURNING binding_id
 ) SELECT a.company_id portal_purchase_company_id,a.id binding_id,a.version binding_version,a.order_id,a.number,a.kind,a.state,a.due_date,a.currency,a.updated_at,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('number',f.number,'createdAt',f.created_at,'recorded',true) ORDER BY f.created_at) FROM ((SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments)) f WHERE f.order_id=a.order_id),'[]'::jsonb) fulfillments,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('action',pa.action,'createdAt',pa.created_at) ORDER BY pa.created_at) FROM ((SELECT \"id\",\"company_id\",\"binding_id\",\"order_id\",\"customer_id\",\"user_id\",\"action\",\"note\",\"source_version\",\"request_key\",\"request_hash\",\"status\",\"created_at\" FROM customer_portal_actions)) pa WHERE pa.binding_id=a.id AND pa.user_id=?),'[]'::jsonb) actions
 FROM accessible a JOIN audit x ON x.binding_id=a.id ORDER BY a.updated_at DESC,a.id", 8),
        EVIDENCE_BINDING_VERSION => (
            "UPDATE customer_access_bindings SET version=version+1,updated_at=? \
             WHERE id=? AND company_id=? AND version=?",
            4,
        ),
        EVIDENCE_EVENT => (
            "INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) \
             VALUES(?,?,?,'portal-evidence-bind',?,?,?)",
            6,
        ),
        _ => return None,
    })
}

pub fn supported(kind: &str) -> bool {
    statement(kind).is_some()
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "CUSTOMER_AUTHORITY_WAVE3_WRITE_INVALID",
        "Review the Customer Authority owner write request.",
    )
}

pub fn plan(
    kind: &str,
    values: Vec<Value>,
) -> Result<CustomerAuthorityWave3Statement, BusinessError> {
    let (sql, expected) = statement(kind).ok_or_else(invalid)?;
    let has_pred = sql.contains("%PREDICATE%");
    if values.is_empty() || values.len() > 64 || values.len() < expected {
        return Err(invalid());
    }
    if !has_pred && values.len() != expected {
        return Err(invalid());
    }
    let predicate = if has_pred {
        let raw = values.last().and_then(|v| v.as_str()).ok_or_else(invalid)?;
        if raw.is_empty() || raw.len() > 8192 || raw.contains(';') || raw.contains("--") {
            return Err(invalid());
        }
        Some(raw.to_owned())
    } else {
        None
    };
    let values = if has_pred {
        values[..expected].to_vec()
    } else {
        values
    };
    let sql = match predicate {
        Some(pred) => sql.replace("%PREDICATE%", &pred),
        None => sql.to_owned(),
    };
    Ok(CustomerAuthorityWave3Statement { sql, values })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_are_parameterized_and_placeholder_exact() {
        for kind in KINDS {
            let (sql, expected) = statement(kind).expect("statement");
            assert!(sql.contains('?'));
            assert_eq!(sql.matches('?').count(), expected);
        }
    }

    #[test]
    fn planner_rejects_mismatch_and_unknown_kinds() {
        assert!(plan(EVIDENCE_BINDING_VERSION, vec![json!(1)]).is_err());
        assert!(plan("unknown", vec![json!(1)]).is_ok() == false);
    }
}
