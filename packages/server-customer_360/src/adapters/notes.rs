//! Customer contact-note reads. Authority and purchase facts are supplied by
//! composition; this adapter only executes the closed, scoped projection.
use crate::domain::notes::{
    CustomerNoteHistoryRow, CustomerNoteOrder, CustomerNoteRow, CustomerNoteSource,
    CustomerNotesView, customer_unavailable,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug)]
pub struct CustomerNotesReadActor {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub user_id: String,
    pub can_create: bool,
}

pub async fn lock_customer_note_sources(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("LOCK TABLE customer_contact_notes, customer_contact_note_history IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn customer_source(
    c: &mut PgConnection,
    actor: &CustomerNotesReadActor,
    customer_id: &str,
) -> Result<CustomerNoteSource, Error> {
    let row = sqlx::query(
        "SELECT id,name,code,version FROM finance_parties \
         WHERE id=$1 AND company_id=$2 AND kind='customer'",
    )
    .bind(customer_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(customer_unavailable().into());
    };
    Ok(CustomerNoteSource {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        code: row.try_get("code")?,
        version: row.try_get("version")?,
    })
}

async fn order_source(
    c: &mut PgConnection,
    actor: &CustomerNotesReadActor,
    order_id: &str,
) -> Result<CustomerNoteOrder, Error> {
    let row = sqlx::query(
        "SELECT o.id,o.party_id,o.number,o.version,p.name party_name \
         FROM commerce_orders o JOIN finance_parties p \
           ON p.id=o.party_id AND p.company_id=o.company_id \
         WHERE o.id=$1 AND o.company_id=$2 AND o.kind IN ('sales','quote') \
           AND ($3::text IS NULL OR o.branch_id=$3) \
           AND ($4::text IS NULL OR o.department_id=$4)",
    )
    .bind(order_id)
    .bind(&actor.company_id)
    .bind(&actor.branch_id)
    .bind(&actor.department_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(customer_unavailable().into());
    };
    Ok(CustomerNoteOrder {
        id: row.try_get("id")?,
        party_id: row.try_get("party_id")?,
        party_name: row.try_get("party_name")?,
        number: row.try_get("number")?,
        version: row.try_get("version")?,
    })
}

fn note_row(row: &sqlx::postgres::PgRow) -> Result<CustomerNoteRow, Error> {
    Ok(CustomerNoteRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        customer_id: row.try_get("customer_id")?,
        order_id: row.try_get("order_id")?,
        channel: row.try_get("channel")?,
        occurred_at: row.try_get("occurred_at")?,
        body: row.try_get("body")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        can_change: false,
    })
}

fn history_row(row: &sqlx::postgres::PgRow) -> Result<CustomerNoteHistoryRow, Error> {
    Ok(CustomerNoteHistoryRow {
        id: row.try_get("id")?,
        note_id: row.try_get("note_id")?,
        version: row.try_get("version")?,
        action: row.try_get("action")?,
        body: row.try_get("body")?,
        reason: row.try_get("reason")?,
        actor_id: row.try_get("actor_id")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Read the bounded customer or order note workspace. The caller must supply
/// the already-authorized Company/branch/department and current user.
pub async fn read_customer_notes(
    c: &mut PgConnection,
    actor: &CustomerNotesReadActor,
    customer_id: Option<&str>,
    order_id: Option<&str>,
) -> Result<CustomerNotesView, Error> {
    let order = match order_id {
        Some(id) => Some(order_source(c, actor, id).await?),
        None => None,
    };
    let customer_id = match (customer_id, &order) {
        (Some(id), _) => id.to_owned(),
        (None, Some(order)) => order.party_id.clone(),
        (None, None) => return Err(customer_unavailable().into()),
    };
    let customer = customer_source(c, actor, &customer_id).await?;

    let (notes, history) = match (&order, order_id) {
        (Some(_), Some(order_id)) => {
            let notes = sqlx::query(
                "SELECT n.* FROM customer_contact_notes n \
                 JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                 WHERE n.company_id=$1 AND n.order_id=$2 \
                   AND ($3::text IS NULL OR o.branch_id=$3) \
                   AND ($4::text IS NULL OR o.department_id=$4) \
                 ORDER BY n.created_at DESC,n.id DESC",
            )
            .bind(&actor.company_id)
            .bind(order_id)
            .bind(&actor.branch_id)
            .bind(&actor.department_id)
            .fetch_all(&mut *c)
            .await?;
            let history = sqlx::query(
                "SELECT h.* FROM customer_contact_note_history h \
                 JOIN customer_contact_notes n ON n.id=h.note_id \
                 JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                 WHERE n.company_id=$1 AND n.order_id=$2 \
                   AND ($3::text IS NULL OR o.branch_id=$3) \
                   AND ($4::text IS NULL OR o.department_id=$4) \
                 ORDER BY h.created_at DESC,h.version DESC",
            )
            .bind(&actor.company_id)
            .bind(order_id)
            .bind(&actor.branch_id)
            .bind(&actor.department_id)
            .fetch_all(&mut *c)
            .await?;
            (notes, history)
        }
        _ => {
            let notes = sqlx::query(
                "SELECT n.* FROM customer_contact_notes n \
                 LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                 WHERE n.company_id=$1 AND n.customer_id=$2 \
                   AND (n.order_id IS NULL AND $3::text IS NULL AND $4::text IS NULL \
                        OR n.order_id IS NOT NULL \
                           AND ($3::text IS NULL OR o.branch_id=$3) \
                           AND ($4::text IS NULL OR o.department_id=$4)) \
                 ORDER BY n.created_at DESC,n.id DESC",
            )
            .bind(&actor.company_id)
            .bind(&customer_id)
            .bind(&actor.branch_id)
            .bind(&actor.department_id)
            .fetch_all(&mut *c)
            .await?;
            let history = sqlx::query(
                "SELECT h.* FROM customer_contact_note_history h \
                 JOIN customer_contact_notes n ON n.id=h.note_id \
                 LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
                 WHERE n.company_id=$1 AND n.customer_id=$2 \
                   AND (n.order_id IS NULL AND $3::text IS NULL AND $4::text IS NULL \
                        OR n.order_id IS NOT NULL \
                           AND ($3::text IS NULL OR o.branch_id=$3) \
                           AND ($4::text IS NULL OR o.department_id=$4)) \
                 ORDER BY h.created_at DESC,h.version DESC",
            )
            .bind(&actor.company_id)
            .bind(&customer_id)
            .bind(&actor.branch_id)
            .bind(&actor.department_id)
            .fetch_all(&mut *c)
            .await?;
            (notes, history)
        }
    };

    Ok(CustomerNotesView {
        customer,
        order,
        notes: notes.iter().map(note_row).collect::<Result<_, _>>()?,
        history: history.iter().map(history_row).collect::<Result<_, _>>()?,
        can_create: actor.can_create,
    })
}
