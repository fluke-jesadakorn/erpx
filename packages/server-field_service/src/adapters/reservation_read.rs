//! Service-owned scoped order and exact reservation readback. No actor is constructed here.
use crate::application::reservation_read::{
    FieldServiceReservationRead, FieldServiceReserveEvent, FieldServiceReserveResult,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::record_origin::{
    PgRawRecordOrigin, RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
    verify_raw_record_origin,
};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
fn denied() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "FIELD_SERVICE_ORDER_NOT_FOUND",
        "Service order not found in this scope.",
    )
    .into()
}
fn invalid() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Field Service source changed.",
    )
    .into()
}
fn unit_required(c: &PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_SERVICE_OWNER_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    unit_required(c)?;
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FIELD_SERVICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Acquire the owner absence/source fence before the Identity snapshot.
pub async fn lock_field_service_reservation_read_sources(
    c: &mut PgConnection,
) -> Result<(), Error> {
    unit_required(c)?;
    sqlx::query("LOCK TABLE field_service_orders,field_service_order_events,field_service_part_reservations IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(PartialEq, Eq)]
struct Image {
    order: FieldServiceReserveResult,
    reservation: FieldServiceReservationRead,
    creator: String,
    reason: String,
    branch: String,
    digest: String,
    bytes: usize,
}
async fn image(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    reservation: &str,
    budget: usize,
) -> Result<Image, Error> {
    let scope = current.selection().workspace();
    let row=sqlx::query("SELECT CASE WHEN b.bytes<=$6 THEN o.id END id,CASE WHEN b.bytes<=$6 THEN o.status END status,o.version,CASE WHEN b.bytes<=$6 THEN r.id END reservation_id,CASE WHEN b.bytes<=$6 THEN r.warehouse_id END warehouse_id,CASE WHEN b.bytes<=$6 THEN r.product_id END product_id,CASE WHEN b.bytes<=$6 THEN r.created_by END creator,CASE WHEN b.bytes<=$6 THEN r.reason END reason,CASE WHEN b.bytes<=$6 THEN o.branch_id END branch_id,b.bytes,r.quantity_milli,r.consumed_milli,r.released_milli,r.expires_at,encode(sha256(convert_to((to_jsonb(o)||jsonb_build_object('reservation',to_jsonb(r)))::text,'UTF8')),'hex') digest FROM field_service_orders o JOIN field_service_part_reservations r ON r.order_id=o.id CROSS JOIN LATERAL (SELECT (2*octet_length(o.id)::bigint+octet_length(o.status)+octet_length(r.id)+octet_length(r.warehouse_id)+octet_length(r.product_id)+octet_length(r.created_by)+octet_length(r.reason)+octet_length(o.branch_id)+1024) bytes) b WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4) AND r.id=$5 FOR SHARE OF o,r")
        .bind(order).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(|v|v.as_str())).bind(scope.department_id.as_ref().map(|v|v.as_str())).bind(reservation).bind(i64::try_from(budget).unwrap_or(i64::MAX)).fetch_optional(c).await?.ok_or_else(denied)?;
    let bounded = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(invalid)
    };
    let id = bounded("id")?;
    let status = bounded("status")?;
    let version: i64 = row.try_get("version")?;
    let quantity: i64 = row.try_get("quantity_milli")?;
    let consumed: i64 = row.try_get("consumed_milli")?;
    let released: i64 = row.try_get("released_milli")?;
    let expires: i64 = row.try_get("expires_at")?;
    if !(1..=9_007_199_254_740_991).contains(&version)
        || !(1..=9_007_199_254_740_991).contains(&quantity)
        || !(0..=quantity).contains(&consumed)
        || !(0..=quantity).contains(&released)
        || consumed.checked_add(released).is_none_or(|n| n > quantity)
        || !(1..=9_007_199_254_740_991).contains(&expires)
    {
        return Err(invalid());
    }
    Ok(Image {
        order: FieldServiceReserveResult {
            id: id.clone(),
            status,
            version,
        },
        reservation: FieldServiceReservationRead {
            id: bounded("reservation_id")?,
            order_id: id,
            warehouse_id: bounded("warehouse_id")?,
            product_id: bounded("product_id")?,
            quantity_milli: quantity,
            consumed_milli: consumed,
            released_milli: released,
            expires_at: expires,
        },
        creator: bounded("creator")?,
        reason: bounded("reason")?,
        branch: bounded("branch_id")?,
        digest: row.try_get("digest")?,
        bytes: usize::try_from(row.try_get::<i64, _>("bytes")?).map_err(|_| invalid())?,
    })
}
/// Read-only retained source and the actual Workflow receipt; neither grants permission.
pub struct CurrentFieldServiceReservationRead {
    unit: (String, i32, String, String),
    image: Image,
    origin: PgRawRecordOrigin,
    receipt: FieldServiceReserveResult,
    event_digest: String,
}
async fn event(
    c: &mut PgConnection,
    image: &Image,
    receipt: &FieldServiceReserveResult,
) -> Result<String, Error> {
    let key = image
        .reservation
        .id
        .strip_prefix("field-part-")
        .ok_or_else(invalid)?;
    let digest:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM field_service_order_events e JOIN field_service_part_reservations r ON r.id=$2 AND r.order_id=e.order_id WHERE e.id=$1 AND e.order_id=$3 AND e.version=$4 AND e.action='reserve-part' AND e.actor_id=r.created_by AND e.reason=r.reason AND e.created_at=r.created_at AND e.status=$5 AND e.details_json->>'warehouseId'=r.warehouse_id AND e.details_json->>'productId'=r.product_id AND e.details_json->'quantityMilli'=to_jsonb(r.quantity_milli) FOR SHARE OF e,r").bind(format!("service-order-event-{key}")).bind(&image.reservation.id).bind(&image.order.id).bind(receipt.version).bind(&receipt.status).fetch_optional(c).await?;
    digest.ok_or_else(invalid)
}
impl CurrentFieldServiceReservationRead {
    pub fn order(&self) -> &FieldServiceReserveResult {
        &self.image.order
    }
    pub fn reservation(&self) -> &FieldServiceReservationRead {
        &self.image.reservation
    }
    pub fn receipt(&self) -> &FieldServiceReserveResult {
        &self.receipt
    }
    pub fn event(&self) -> FieldServiceReserveEvent {
        FieldServiceReserveEvent {
            version: self.receipt.version,
            actor_id: self.image.creator.clone(),
            reason: self.image.reason.clone(),
            status: self.receipt.status.clone(),
        }
    }
    pub fn branch_id(&self) -> &str {
        &self.image.branch
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        remaining: usize,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                current,
                &self.image.order.id,
                &self.image.reservation.id,
                remaining.saturating_sub(
                    self.image
                        .bytes
                        .saturating_add(self.receipt.id.len())
                        .saturating_add(self.receipt.status.len())
                        .saturating_add(self.image.creator.len())
                        .saturating_add(self.image.reservation.id.len())
                        .saturating_add(2048),
                ),
            )
            .await?
                != self.image
        {
            return Err(invalid());
        }
        if event(c, &self.image, &self.receipt).await? != self.event_digest {
            return Err(invalid());
        }
        verify_raw_record_origin(c, &self.origin).await
    }
}
pub async fn read_field_service_reservation(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    reservation: &str,
) -> Result<CurrentFieldServiceReservationRead, Error> {
    current.require_permission("records.read")?;
    if !current.modules().iter().any(|m| m == "field_service") {
        return Err(denied());
    }
    let witness = unit(c).await?;
    let image = image(c, current, order, reservation, 2 * 1024 * 1024).await?;
    let key = reservation
        .strip_prefix("field-part-")
        .filter(|v| !v.is_empty())
        .ok_or_else(denied)?;
    let (origin, result) = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: key.into(),
            operation: "field.order.reserve-part".into(),
            record_id: order.into(),
            creator_user_id: image.creator.clone(),
            company_id: current.selection().workspace().company_id.clone(),
        },
        (2 * 1024 * 1024_usize).saturating_sub(image.bytes),
    )
    .await?
    .ok_or_else(denied)?;
    let original_id = result
        .property("id")?
        .and_then(|v| v.string_utf16_units().ok().flatten())
        .ok_or_else(invalid)?;
    if original_id != order.encode_utf16().collect::<Vec<_>>() {
        return Err(invalid());
    }
    let status = result
        .property("status")?
        .and_then(|v| v.string_utf8_storage().ok().flatten())
        .ok_or_else(invalid)?;
    let version = result
        .property("version")?
        .and_then(|v| v.as_str().parse::<i64>().ok())
        .ok_or_else(invalid)?;
    let receipt = FieldServiceReserveResult {
        id: order.into(),
        status,
        version,
    };
    if receipt.id != image.order.id
        || receipt.version < 2
        || receipt.version > image.order.version
        || !matches!(receipt.status.as_str(), "open" | "scheduled")
    {
        return Err(invalid());
    }
    let event_digest = event(c, &image, &receipt).await?;
    Ok(CurrentFieldServiceReservationRead {
        unit: witness,
        image,
        origin,
        receipt,
        event_digest,
    })
}
