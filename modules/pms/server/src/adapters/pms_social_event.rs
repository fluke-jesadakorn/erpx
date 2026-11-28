//! PMS-owned read ports for the Workflow PMS social projector.
//!
//! Exact port of the `payoutSource` and `source` units in
//! `modules/pms/server/src/adapters/pms/social-events.ts`. Both statements run
//! on the caller's `&mut sqlx::PgConnection`; this adapter never opens or
//! commits a transaction.
use crate::domain::{booking::encode_uri_component, social_sources::PMS_SOURCES_SQL};
use erpx_platform_runtime_server::public::postgres::PersistenceError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgConnection, Row};

/// One row from the retained `pmsSourcesSql` union.
#[derive(Clone, Debug, PartialEq)]
pub struct PmsSocialSource {
    pub id: String,
    pub property_id: String,
    pub kind: String,
    pub company_id: String,
    pub branch_id: String,
    pub property_name: String,
    pub business_date: String,
    pub data_json: Value,
    pub version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsSocialLink {
    pub section: &'static str,
    pub source_kind: String,
    pub version: i64,
    pub label: &'static str,
    pub href: String,
    pub title: String,
}

impl PmsSocialSource {
    /// Capability that owns this Social source's history and current actions.
    pub fn capability(&self) -> Option<&'static str> {
        source_destination(&self.kind).map(|(_, _, capability)| capability)
    }

    /// Build the retained link for an authorized preview. The other fields in
    /// the hotel record stay with the owner.
    pub fn link(&self) -> Option<PmsSocialLink> {
        let (section, label, _) = source_destination(&self.kind)?;
        let mut href = format!(
            "/pms/{section}?property={}&record={}",
            query_component(&self.property_id),
            query_component(&self.id)
        );
        let guest = if self.kind == "guest_account" {
            Some(self.id.as_str())
        } else {
            self.data_json["guestAccountId"]
                .as_str()
                .filter(|id| !id.is_empty())
        };
        if let Some(guest) = guest {
            href.push_str("&guestAccount=");
            href.push_str(&query_component(guest));
        }
        match self.kind.as_str() {
            "task" => {
                href.push_str("&task=");
                href.push_str(&query_component(&self.id));
            }
            "transfer" | "refund" => href.push_str("&queue=approval"),
            "correction" => href.push_str("&queue=corrections"),
            _ => {}
        }
        let mut title = format!("{} · {label}", self.property_name);
        if let Some(number) = self.data_json["number"]
            .as_str()
            .filter(|number| !number.is_empty())
        {
            title.push(' ');
            title.push_str(number);
        }
        Some(PmsSocialLink {
            section,
            source_kind: self.kind.clone(),
            version: self.version,
            label,
            href,
            title,
        })
    }
}

fn source_destination(kind: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match kind {
        "reservation" => ("reservations", "Reservation", "pms.reservations"),
        "stay" => ("front-desk", "Stay", "pms.front-desk"),
        "room" => ("housekeeping", "Room", "pms.rooms"),
        "block" => ("housekeeping", "Room block", "pms.rooms"),
        "task" => ("housekeeping", "Housekeeping task", "pms.housekeeping"),
        "guest_account" => ("guest-accounts", "Guest Account", "pms.guest-accounts"),
        "refund" => ("guest-accounts", "Refund", "pms.guest-accounts"),
        "correction" => ("guest-accounts", "Correction", "pms.guest-accounts"),
        "transfer" => ("guest-accounts", "Transfer", "pms.guest-accounts"),
        "shift" => ("cashier", "Shift", "pms.guest-accounts"),
        "night" => ("night-audit", "Night audit", "pms.night-audit"),
        _ => return None,
    })
}

fn query_component(value: &str) -> String {
    // URLSearchParams uses form encoding. Other delimiters and UTF-8 bytes
    // are already escaped by encode_uri_component.
    encode_uri_component(value)
        .replace("%20", "+")
        .replace('!', "%21")
        .replace('~', "%7E")
        .replace('\'', "%27")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// Exact retained payout source lookup for `pms.refund.pay`.
pub async fn pms_social_payout_source(
    connection: &mut PgConnection,
    id: &str,
    property_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row = sqlx::query(
        "SELECT data_json::jsonb->>'originalId' id FROM pms_records \
         WHERE id=$1 AND property_id=$2 AND kind='entry'",
    )
    .bind(id)
    .bind(property_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(read_failed)?;
    row.map(|row| row.try_get::<Option<String>, _>("id").map_err(read_failed))
        .transpose()
        .map(Option::flatten)
}

/// Exact retained source lookup over the `pmsSourcesSql` union.
pub async fn pms_social_source(
    connection: &mut PgConnection,
    id: Option<&str>,
    company_id: &str,
    property_id: &str,
) -> Result<Option<PmsSocialSource>, BusinessError> {
    source(connection, id, company_id, Some(property_id)).await
}

/// Read a current metadata row for a source already authorized in the same
/// Company. The call does not decide which user may see the record.
pub async fn pms_social_source_for_company(
    connection: &mut PgConnection,
    id: &str,
    company_id: &str,
) -> Result<Option<PmsSocialSource>, BusinessError> {
    source(connection, Some(id), company_id, None).await
}

async fn source(
    connection: &mut PgConnection,
    id: Option<&str>,
    company_id: &str,
    property_id: Option<&str>,
) -> Result<Option<PmsSocialSource>, BusinessError> {
    let sql = format!(
        "SELECT pr.id,pr.property_id,pr.kind,pr.data data_json,pr.version,pr.company_id,pr.branch_id,\
         pr.property_name,pr.business_date FROM ({PMS_SOURCES_SQL}) pr \
         WHERE pr.id=$1 AND pr.company_id=$2 AND ($3::text IS NULL OR pr.property_id=$3)"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(company_id)
        .bind(property_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(read_failed)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(PmsSocialSource {
        id: row.try_get("id").map_err(read_failed)?,
        property_id: row.try_get("property_id").map_err(read_failed)?,
        kind: row.try_get("kind").map_err(read_failed)?,
        company_id: row.try_get("company_id").map_err(read_failed)?,
        branch_id: row.try_get("branch_id").map_err(read_failed)?,
        property_name: row.try_get("property_name").map_err(read_failed)?,
        business_date: row.try_get("business_date").map_err(read_failed)?,
        data_json: row.try_get("data_json").map_err(read_failed)?,
        version: row.try_get("version").map_err(read_failed)?,
    }))
}

fn read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "PMS_SOCIAL_RETRY_REQUIRED",
            "The PMS social source must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "PMS_SOCIAL_READ_FAILED",
            "The PMS social source could not be read.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_union_keeps_all_three_retained_branches() {
        assert_eq!(PMS_SOURCES_SQL.matches("UNION ALL").count(), 2);
        assert!(PMS_SOURCES_SQL.contains("FROM pms_records r JOIN pms_properties p"));
        assert!(PMS_SOURCES_SQL.contains("FROM pms_transfer_requests t"));
        assert!(PMS_SOURCES_SQL.contains("FROM pms_charge_corrections c"));
    }
}
