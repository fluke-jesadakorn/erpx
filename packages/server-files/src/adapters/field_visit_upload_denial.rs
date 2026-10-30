//! Best-effort denial evidence in a fresh selected-actor owner unit.
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
/// Trusted Identity owner port; it must verify the actor is current in this
/// exact unit without requiring successful Field source authorization.
pub trait FieldDenialActorCurrent {
    fn require_current<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
    .into()
}
/// No source information leaves this unit. The caller preserves the original
/// denial if this separate best-effort evidence operation fails.
pub async fn record_field_content_denial<A: FieldDenialActorCurrent>(
    c: &mut PgConnection,
    authority: &A,
    actor: &AuthorizedWorkContext,
    link_id: &str,
    variant: &str,
    reason: &str,
    clock: &(dyn Clock + Sync),
) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    if !matches!(variant, "original" | "preview")
        || reason.is_empty()
        || reason.len() > 80
        || !reason
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(changed());
    }
    actor.require_permission("records.read")?;
    let now = clock.now().get();
    if now >= actor.expires_at() {
        return Err(changed());
    }
    authority.require_current(c, actor, now).await?;
    let company = actor.selection().workspace().company_id.as_str();
    let user = actor.user_id().as_str();
    let row:Option<(String,String,String,String,String)>=sqlx::query_as("SELECT a.id,s.target_type,s.target_id,s.component_kind,s.purpose_key FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 FOR SHARE OF l,s,a").bind(link_id).bind(company).fetch_optional(&mut *c).await?;
    let (source, target_type, target, component, purpose) = row.ok_or_else(changed)?;
    if !crate::domain::field_visit_link::valid_field_purpose(&target_type, &component, &purpose) {
        return Err(changed());
    }
    let id = format!("attachment-access-{}", uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,'denied',$7,$8)").bind(&id).bind(company).bind(&source).bind(link_id).bind(user).bind(variant).bind(reason).bind(now).execute(&mut *c).await?;
    // Final evidence and selected-authority checks must observe deferred audit
    // effects in this unit, rather than letting them run only at COMMIT.
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    let end = clock.now().get();
    if end >= actor.expires_at() {
        return Err(changed());
    }
    authority.require_current(c, actor, end).await?;
    let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_access_events e JOIN attachment_links l ON l.id=e.link_id JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE e.id=$1 AND e.company_id=$2 AND e.attachment_id=$3 AND e.link_id=$4 AND e.actor_id=$5 AND e.variant=$6 AND e.outcome='denied' AND e.reason=$7 AND e.created_at=$8 AND s.company_id=$2 AND s.target_type=$9 AND s.component_kind=$10 AND s.purpose_key=$11 AND s.target_id=$12 AND a.id=$3)").bind(&id).bind(company).bind(&source).bind(link_id).bind(user).bind(variant).bind(reason).bind(now).bind(&target_type).bind(&component).bind(&purpose).bind(&target).fetch_one(&mut *c).await?;
    if !exact {
        return Err(changed());
    }
    Ok(())
}
