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

/// Scoped issue confirmation. The original event and Workflow receipt identify
/// the exact command independently of the current order/reservation image.
pub struct CurrentFieldServiceIssueRead {
    unit: (String, i32, String, String),
    reservation: CurrentFieldServiceReservationRead,
    receipt: FieldServiceReserveResult,
    event: crate::application::reservation_read::FieldServiceIssueEvent,
    event_digest: String,
    origin: PgRawRecordOrigin,
    key: String,
}
async fn issue_event(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    key: &str,
) -> Result<
    (
        crate::application::reservation_read::FieldServiceIssueEvent,
        String,
    ),
    Error,
> {
    use crate::application::reservation_read::FieldServiceIssueEvent;
    let scope = current.selection().workspace();
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.actor_id END actor_id, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.reason END reason, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.status END status, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.details_json->>'reservationId' END reservation_id, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.details_json->>'consumptionId' END consumption_id, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.details_json->>'journalId' END journal_id, e.version, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.details_json->>'quantityMilli' END quantity, CASE WHEN octet_length(to_jsonb(e)::text)<=262144 THEN e.details_json->>'costMinor' END cost, encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') digest FROM field_service_order_events e JOIN field_service_orders o ON o.id=e.order_id WHERE e.id=$1 AND e.order_id=$2 AND e.action='issue-part' AND o.company_id=$3 AND ($4::text IS NULL OR o.branch_id=$4) AND ($5::text IS NULL OR o.department_id=$5) AND o.version>=e.version FOR SHARE OF e,o")
        .bind(format!("service-order-event-{key}")).bind(order).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(|v|v.as_str())).bind(scope.department_id.as_ref().map(|v|v.as_str())).fetch_optional(c).await?.ok_or_else(denied)?;
    let text = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?
            .filter(|v| !v.is_empty())
            .ok_or_else(invalid)
    };
    let number = |name| -> Result<i64, Error> {
        let n = text(name)?.parse::<i64>().map_err(|_| invalid())?;
        if !(1..=9_007_199_254_740_991).contains(&n) {
            return Err(invalid());
        }
        Ok(n)
    };
    let version: i64 = row.try_get("version")?;
    if !(1..=9_007_199_254_740_991).contains(&version) {
        return Err(invalid());
    }
    let event = FieldServiceIssueEvent {
        version,
        actor_id: text("actor_id")?,
        reason: text("reason")?,
        status: text("status")?,
        reservation_id: text("reservation_id")?,
        consumption_id: text("consumption_id")?,
        journal_id: text("journal_id")?,
        quantity_milli: number("quantity")?,
        cost_minor: number("cost")?,
    };
    if event.consumption_id != format!("operational-part-{key}") {
        return Err(invalid());
    }
    Ok((event, row.try_get("digest")?))
}
impl CurrentFieldServiceIssueRead {
    pub fn reservation(&self) -> &CurrentFieldServiceReservationRead {
        &self.reservation
    }
    pub fn receipt(&self) -> &FieldServiceReserveResult {
        &self.receipt
    }
    pub fn event(&self) -> &crate::application::reservation_read::FieldServiceIssueEvent {
        &self.event
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        remaining: usize,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(invalid());
        }
        self.reservation
            .require_current(c, current, remaining)
            .await?;
        let (event, digest) = issue_event(c, current, &self.receipt.id, &self.key).await?;
        if event != self.event || digest != self.event_digest {
            return Err(invalid());
        }
        verify_raw_record_origin(c, &self.origin).await
    }
}
pub async fn read_field_service_issue(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    key: &str,
) -> Result<CurrentFieldServiceIssueRead, Error> {
    current.require_permission("records.read")?;
    if !current.modules().iter().any(|m| m == "field_service") {
        return Err(denied());
    }
    erpx_server_shared::public::CommandKey::new(key.to_owned())?;
    let witness = unit(c).await?;
    let (event, event_digest) = issue_event(c, current, order, key).await?;
    let reservation =
        read_field_service_reservation(c, current, order, &event.reservation_id).await?;
    let (origin, result) = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: key.into(),
            operation: "field.order.issue-part".into(),
            record_id: order.into(),
            creator_user_id: event.actor_id.clone(),
            company_id: current.selection().workspace().company_id.clone(),
        },
        1024 * 1024,
    )
    .await?
    .ok_or_else(denied)?;
    let result_id = result
        .property("id")?
        .and_then(|v| v.string_utf16_units().ok().flatten())
        .ok_or_else(invalid)?;
    let status = result
        .property("status")?
        .and_then(|v| v.string_utf8_storage().ok().flatten())
        .ok_or_else(invalid)?;
    let version = result
        .property("version")?
        .and_then(|v| v.as_str().parse::<i64>().ok())
        .ok_or_else(invalid)?;
    if result_id != order.encode_utf16().collect::<Vec<_>>()
        || version != event.version
        || status != event.status
        || event.version > reservation.order().version
        || reservation.reservation().consumed_milli < event.quantity_milli
    {
        return Err(invalid());
    }
    Ok(CurrentFieldServiceIssueRead {
        unit: witness,
        reservation,
        receipt: FieldServiceReserveResult {
            id: order.into(),
            status,
            version,
        },
        event,
        event_digest,
        origin,
        key: key.into(),
    })
}

#[derive(PartialEq, Eq)]
struct VisitImage {
    has_asset: bool,
    order: FieldServiceReserveResult,
    event: crate::application::reservation_read::FieldServiceVisitEvent,
    digest: String,
}
async fn visit_image(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    key: &str,
) -> Result<VisitImage, Error> {
    let scope = current.selection().workspace();
    let row=sqlx::query("SELECT b.bounded,coalesce(o.asset_id<>'',false) has_asset,CASE WHEN b.bounded THEN o.id END order_id,CASE WHEN b.bounded THEN o.status END order_status,o.version::bigint order_version,CASE WHEN b.bounded THEN e.action END action,CASE WHEN b.bounded THEN e.actor_id END actor_id,CASE WHEN b.bounded THEN e.reason END reason,CASE WHEN b.bounded THEN e.status END event_status,CASE WHEN b.bounded THEN e.technician_id END technician_id,e.scheduled_at::bigint scheduled_at,e.version::bigint event_version,CASE WHEN b.bounded AND e.action='complete' THEN e.details_json->>'laborMinutes' END labor_minutes,CASE WHEN b.bounded AND e.action='complete' THEN e.details_json->>'evidence' END evidence,CASE WHEN b.bounded AND e.action='complete' THEN e.details_json->>'customerConfirmation' END confirmation,encode(sha256(convert_to(jsonb_build_array(to_jsonb(o),to_jsonb(e))::text,'UTF8')),'hex') digest FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id CROSS JOIN LATERAL (SELECT octet_length(o.id)::bigint+octet_length(o.status)::bigint+octet_length(e.action)::bigint+octet_length(e.actor_id)::bigint+octet_length(e.reason)::bigint+octet_length(e.status)::bigint+octet_length(e.technician_id)::bigint+coalesce(octet_length(e.details_json->>'laborMinutes'),0)::bigint+coalesce(octet_length(e.details_json->>'evidence'),0)::bigint+coalesce(octet_length(e.details_json->>'customerConfirmation'),0)::bigint<=2097152 AS bounded) b WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4) AND e.id=$5 AND e.action IN ('schedule','dispatch','start','complete','exception','resume','cancel') AND (e.action<>'complete' OR (jsonb_typeof(e.details_json->'laborMinutes')='number' AND jsonb_typeof(e.details_json->'evidence')='string' AND jsonb_typeof(e.details_json->'customerConfirmation')='string')) FOR SHARE OF o,e").bind(order).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(|v|v.as_str())).bind(scope.department_id.as_ref().map(|v|v.as_str())).bind(format!("service-order-event-{key}")).fetch_optional(c).await?.ok_or_else(denied)?;
    if !row.try_get::<bool, _>("bounded")? {
        return Err(invalid());
    }
    let text =
        |n| -> Result<String, Error> { row.try_get::<Option<String>, _>(n)?.ok_or_else(invalid) };
    let order = FieldServiceReserveResult {
        id: text("order_id")?,
        status: text("order_status")?,
        version: row.try_get("order_version")?,
    };
    let event = crate::application::reservation_read::FieldServiceVisitEvent {
        version: row.try_get("event_version")?,
        action: text("action")?,
        actor_id: text("actor_id")?,
        reason: text("reason")?,
        status: text("event_status")?,
        technician_id: text("technician_id")?,
        scheduled_at: row.try_get("scheduled_at")?,
        labor_minutes: if text("action")? == "complete" {
            Some(text("labor_minutes")?.parse().map_err(|_| invalid())?)
        } else {
            None
        },
        evidence: if text("action")? == "complete" {
            Some(text("evidence")?)
        } else {
            None
        },
        customer_confirmation: if text("action")? == "complete" {
            Some(text("confirmation")?)
        } else {
            None
        },
    };
    if !(1..=9_007_199_254_740_991).contains(&order.version)
        || !(1..=order.version).contains(&event.version)
        || event
            .scheduled_at
            .is_some_and(|n| !(1..=9_007_199_254_740_991).contains(&n))
        || event
            .labor_minutes
            .is_some_and(|n| !(0..=9_007_199_254_740_991).contains(&n))
        || event.status
            != match event.action.as_str() {
                "schedule" => "scheduled",
                "dispatch" => "dispatched",
                "start" => "in_visit",
                "complete" => "completed",
                "exception" => "exception",
                "resume" => "in_visit",
                "cancel" => "cancelled",
                _ => return Err(invalid()),
            }
    {
        return Err(invalid());
    }
    Ok(VisitImage {
        has_asset: row.try_get("has_asset")?,
        order,
        event,
        digest: row.try_get("digest")?,
    })
}
pub struct CurrentFieldServiceVisitRead {
    unit: (String, i32, String, String),
    image: VisitImage,
    origin: PgRawRecordOrigin,
    receipt: FieldServiceReserveResult,
    key: String,
}
impl CurrentFieldServiceVisitRead {
    pub fn has_asset(&self) -> bool {
        self.image.has_asset
    }
    pub fn order(&self) -> &FieldServiceReserveResult {
        &self.image.order
    }
    pub fn receipt(&self) -> &FieldServiceReserveResult {
        &self.receipt
    }
    pub fn event(&self) -> &crate::application::reservation_read::FieldServiceVisitEvent {
        &self.image.event
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || visit_image(c, current, &self.image.order.id, &self.key).await? != self.image
        {
            return Err(invalid());
        }
        verify_raw_record_origin(c, &self.origin).await
    }
}
pub async fn read_field_service_visit(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    order: &str,
    key: &str,
) -> Result<CurrentFieldServiceVisitRead, Error> {
    current.require_permission("records.read")?;
    if !current.modules().iter().any(|m| m == "field_service") {
        return Err(denied());
    }
    erpx_server_shared::public::CommandKey::new(key.to_owned())?;
    let unit = unit(c).await?;
    let image = visit_image(c, current, order, key).await?;
    let (origin, result) = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: key.into(),
            operation: format!("field.order.{}", image.event.action),
            record_id: order.into(),
            creator_user_id: image.event.actor_id.clone(),
            company_id: current.selection().workspace().company_id.clone(),
        },
        2 * 1024 * 1024,
    )
    .await?
    .ok_or_else(denied)?;
    let id = result
        .property("id")?
        .and_then(|v| v.string_utf16_units().ok().flatten())
        .ok_or_else(invalid)?;
    let status = result
        .property("status")?
        .and_then(|v| v.string_utf8_storage().ok().flatten())
        .ok_or_else(invalid)?;
    let version = result
        .property("version")?
        .and_then(|v| v.as_str().parse::<i64>().ok())
        .ok_or_else(invalid)?;
    if id != order.encode_utf16().collect::<Vec<_>>()
        || version != image.event.version
        || status != image.event.status
    {
        return Err(invalid());
    }
    Ok(CurrentFieldServiceVisitRead {
        unit,
        image,
        origin,
        receipt: FieldServiceReserveResult {
            id: order.into(),
            status,
            version,
        },
        key: key.into(),
    })
}

/// Resolve the physical order only from its actual scoped immutable event; the
/// subsequent read verifies the matching Workflow origin and full images.
pub async fn read_field_service_visit_by_key(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    key: &str,
) -> Result<CurrentFieldServiceVisitRead, Error> {
    current.require_permission("records.read")?;
    erpx_server_shared::public::CommandKey::new(key.to_owned())?;
    let scope = current.selection().workspace();
    let row:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(o.id)<=2097152 THEN o.id END FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id WHERE e.id=$1 AND e.action IN ('schedule','dispatch','start','complete','exception','resume','cancel') AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4) FOR SHARE OF o,e").bind(format!("service-order-event-{key}")).bind(scope.company_id.as_str()).bind(scope.branch_id.as_ref().map(|v|v.as_str())).bind(scope.department_id.as_ref().map(|v|v.as_str())).fetch_optional(&mut *c).await?;
    let order = row.ok_or_else(denied)?.ok_or_else(invalid)?;
    read_field_service_visit(c, current, &order, key).await
}
