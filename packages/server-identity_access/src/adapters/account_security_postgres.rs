use super::account_mfa_crypto::AccountMfaKeyring;
use super::{
    account_mfa_postgres::{current, require_fresh_mfa_in_transaction},
    account_postgres::{TxResult, finish, options},
    account_sessions::lock_session_writes,
};
use crate::domain::account_security_snapshot::*;
use crate::{
    application::{
        account_ports::{AccountFuture, DeviceProof},
        account_revocation::AccountWorkContextInvalidation,
        account_security::AccountSecurityRepository,
    },
    domain::{
        account_auth::RequestAudience,
        account_revocation::{AccountRevocationTarget, RetiredAccountChild},
        account_security::*,
    },
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

const MAX_SOURCE_ROWS: usize = 2048;
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
struct SourceBudget {
    rows: usize,
    bytes: usize,
}
impl SourceBudget {
    fn retain(&mut self, bytes: usize) -> Result<(), BusinessError> {
        self.rows += 1;
        self.bytes = self.bytes.saturating_add(bytes);
        if self.rows > MAX_SOURCE_ROWS || self.bytes > MAX_SOURCE_BYTES {
            return Err(source_limit());
        }
        Ok(())
    }
}
fn source_limit() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_SECURITY_SOURCE_LIMIT",
        "The current security source exceeds its safe read budget.",
    )
}
async fn bounded_rows<T: serde::de::DeserializeOwned>(
    connection: &mut PgConnection,
    sql: &'static str,
    user: &str,
    scope: Option<&[String]>,
    budget: &mut SourceBudget,
) -> TxResult<Vec<T>> {
    use futures_util::TryStreamExt;
    // Every caller supplies an audited static SELECT with actor binding, SQL
    // per-row byte guard, deterministic ordering and a hard row limit.
    let query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(user);
    let query = if let Some(scope) = scope {
        query.bind(scope)
    } else {
        query
    };
    let mut rows = query.fetch(connection);
    let mut values = Vec::new();
    while let Some(row) = rows.try_next().await? {
        let payload: Option<String> = row.try_get("payload")?;
        let payload = payload.ok_or_else(source_limit)?;
        budget.retain(payload.len())?;
        values.push(serde_json::from_str(&payload).map_err(|_| source_limit())?);
    }
    Ok(values)
}
async fn source(
    connection: &mut PgConnection,
    actor: &Actor,
    retained_ids: &[String],
) -> TxResult<Image> {
    // Detect an invalid foreign child reference without transferring another
    // user's session row or personal image into this personal snapshot.
    let foreign:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sso_session_bindings b JOIN session p ON p.id=b.parent_session_id JOIN session c ON c.id=b.session_id WHERE p.\"userId\"=$1 AND c.\"userId\"<>$1)").bind(&actor.user_id).fetch_one(&mut *connection).await?;
    if foreign {
        return Err(changed().into());
    }
    let mut budget = SourceBudget { rows: 0, bytes: 0 };
    let sessions:Vec<Session>=bounded_rows(connection,r#"SELECT CASE WHEN octet_length(payload::text)<=65536 THEN payload::text END payload FROM (
      SELECT s.id ordinal,jsonb_build_object('id',s.id,'user_id',s."userId",'created_at',s."createdAt"::text,'expires_at',s."expiresAt"::text,'user_agent',s."userAgent",'image',to_jsonb(s)::text,'expires_ms',(EXTRACT(EPOCH FROM s."expiresAt"::timestamptz)*1000)::bigint) payload
      FROM session s WHERE s."userId"=$1) own ORDER BY ordinal LIMIT 2049"#,&actor.user_id,None,&mut budget).await?;
    let bindings=bounded_rows(connection,r#"SELECT CASE WHEN octet_length(payload::text)<=65536 THEN payload::text END payload FROM (
      SELECT b.session_id ordinal,jsonb_build_object('session_id',b.session_id,'parent_session_id',b.parent_session_id,'audience',b.audience,'environment',b.environment,'authenticated_at',b.authenticated_at,'absolute_expires_at',b.absolute_expires_at,'image',to_jsonb(b)::text) payload
      FROM sso_session_bindings b WHERE b.session_id=ANY($2::text[]) OR b.parent_session_id=ANY($2::text[]) OR EXISTS(SELECT 1 FROM session s WHERE s."userId"=$1 AND (s.id=b.session_id OR s.id=b.parent_session_id))) relevant ORDER BY ordinal LIMIT 2049"#,&actor.user_id,Some(retained_ids),&mut budget).await?;
    let mfa=bounded_rows(connection,r#"SELECT CASE WHEN octet_length(payload::text)<=4096 AND octet_length(secret)<=256 THEN payload::text END payload FROM (
      SELECT m.secret,jsonb_build_object('enabled',m.enabled,'image',to_jsonb(m)::text) payload FROM iam_mfa m WHERE m.user_id=$1) own LIMIT 1"#,&actor.user_id,None,&mut budget).await?.pop();
    let verifications=bounded_rows(connection,r#"SELECT CASE WHEN octet_length(payload::text)<=4096 THEN payload::text END payload FROM (
      SELECT v.session_id ordinal,jsonb_build_object('session_id',v.session_id,'user_id',v.user_id,'verified_at',v.verified_at,'image',to_jsonb(v)::text) payload FROM iam_mfa_sessions v WHERE v.user_id=$1) own ORDER BY ordinal LIMIT 2049"#,&actor.user_id,None,&mut budget).await?;
    let user = bounded_rows(
        connection,
        r#"SELECT CASE WHEN octet_length(payload::text)<=65536 THEN payload::text END payload FROM (
      SELECT to_jsonb(to_jsonb(u)::text) payload FROM "user" u WHERE u.id=$1) own LIMIT 1"#,
        &actor.user_id,
        None,
        &mut budget,
    )
    .await?
    .pop();
    Ok(Image {
        sessions,
        bindings,
        mfa,
        verifications,
        user,
    })
}
pub struct PostgresAccountSecurity {
    unit: PgUnitOfWork,
    environment: String,
    clock: Arc<dyn Clock + Send + Sync>,
    invalidation: Arc<dyn AccountWorkContextInvalidation>,
    keys: Arc<AccountMfaKeyring>,
}
impl PostgresAccountSecurity {
    pub fn new(
        pool: DatabasePool,
        environment: String,
        clock: Arc<dyn Clock + Send + Sync>,
        invalidation: Arc<dyn AccountWorkContextInvalidation>,
        keys: Arc<AccountMfaKeyring>,
    ) -> Result<Self, BusinessError> {
        if environment != "development" {
            return Err(BusinessError::invalid(
                "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
                "The Account environment is not enabled.",
            ));
        }
        Ok(Self {
            unit: PgUnitOfWork::new(pool),
            environment,
            clock,
            invalidation,
            keys,
        })
    }
}
async fn leases(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    lock_session_writes(connection).await?;
    sqlx::query("LOCK TABLE iam_receipts,iam_audit IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *connection)
        .await?;
    sqlx::query("LOCK TABLE iam_mfa,iam_mfa_sessions IN SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}
async fn invalidate(
    connection: &mut PgConnection,
    before: &Image,
    ids: &[String],
    invalidation: &dyn AccountWorkContextInvalidation,
) -> TxResult<()> {
    let mut children = Vec::new();
    for binding in before
        .bindings
        .iter()
        .filter(|b| ids.contains(&b.session_id) && b.parent_session_id.is_some())
    {
        let user = before
            .sessions
            .iter()
            .find(|s| s.id == binding.session_id)
            .ok_or_else(changed)?;
        children.push(RetiredAccountChild::from_binding(
            binding.session_id.clone(),
            user.user_id.clone(),
            &binding.audience,
            &binding.environment,
        )?);
    }
    if children.is_empty() {
        return Ok(());
    }
    let rows=sqlx::query("SELECT CASE WHEN octet_length(holding_id)<=128 THEN holding_id END holding_id,CASE WHEN octet_length(write_binding)<=128 THEN write_binding END write_binding FROM holding_bindings WHERE status='active' ORDER BY holding_id LIMIT 257").fetch_all(connection).await?;
    if rows.len() > 256 {
        return Err(source_limit().into());
    }
    let targets =
        rows.into_iter()
            .map(|r| {
                Ok(AccountRevocationTarget {
                    holding_id: HoldingId::new(r.try_get::<String, _>("holding_id")?)?,
                    write_binding: r.try_get("write_binding")?,
                })
            })
            .collect::<Result<
                Vec<_>,
                erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>,
            >>()?;
    invalidation.invalidate(&targets, &children).await?;
    Ok(())
}
impl AccountSecurityRepository for PostgresAccountSecurity {
    fn read<'a>(&'a self, proof: DeviceProof) -> AccountFuture<'a, SecuritySeed> {
        let environment = self.environment.clone();
        let clock = self.clock.clone();
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), move |connection| {
                        Box::pin(async move {
                            leases(connection).await?;
                            let actor = current(
                                connection,
                                &proof,
                                &RequestAudience::Account,
                                &environment,
                                clock.now().get(),
                            )
                            .await?;
                            let actor = Actor {
                                user_id: actor.user.id,
                                session_id: actor.id,
                            };
                            let image = source(connection, &actor, &[]).await?;
                            Ok(seed(&image, &actor, clock.now().get())?)
                        })
                    })
                    .await,
            )
        })
    }
    fn revoke<'a>(
        &'a self,
        proof: DeviceProof,
        input: RevokeSecurity,
        key: String,
    ) -> AccountFuture<'a, RevokedSecurity> {
        let environment = self.environment.clone();
        let clock = self.clock.clone();
        let invalidation = self.invalidation.clone();
        let keys = self.keys.clone();
        Box::pin(async move {
            finish(self.unit.run_once(options(),move|connection|Box::pin(async move{
        leases(connection).await?;
        let fresh=require_fresh_mfa_in_transaction(connection,&proof,&RequestAudience::Account,&environment,keys,clock.now().get()).await?;
        let actor=Actor{user_id:fresh.user_id().into(),session_id:fresh.session_id().into()};
        let before=source(connection,&actor,&[]).await?;let initial=seed(&before,&actor,clock.now().get())?;
        if input.account_key!=initial.account_key{return Err(changed().into());}
        let hash=digest(&("account.sessions.revoke",&actor,&input))?;
        let receipt=sqlx::query("SELECT CASE WHEN octet_length(company_id)<=128 THEN company_id END company_id,CASE WHEN octet_length(actor_id)<=128 THEN actor_id END actor_id,CASE WHEN octet_length(request_hash)<=128 THEN request_hash END request_hash,CASE WHEN octet_length(result::text)<=4096 THEN result END result FROM iam_receipts WHERE id=$1").bind(&key).fetch_optional(&mut *connection).await?;
        if let Some(receipt)=receipt {
            if receipt.try_get::<String,_>("company_id")?!="account"||receipt.try_get::<String,_>("actor_id")?!=actor.user_id||receipt.try_get::<String,_>("request_hash")?!=hash{return Err(BusinessError::new(FailureKind::Conflict,"IDEMPOTENCY_CONFLICT","This key belongs to another captured security request.").into());}
            let result:RevokedSecurity=serde_json::from_value(receipt.try_get("result")?).map_err(|_|super::account_sessions::persistence())?;if !result.revoked{return Err(super::account_sessions::persistence().into());}
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
            fresh.require_current(connection,clock.now().get()).await?;
            if source(connection,&actor,&[]).await?!=before{return Err(changed().into());}return Ok(result);
        }
        if input.expected_revision!=initial.revision{return Err(changed().into());}
        let own=before.sessions.iter().filter(|s|s.user_id==actor.user_id).collect::<Vec<_>>();
        let ids=if input.scope=="family" {
            let handle=input.family.as_deref().ok_or_else(changed)?;
            let mut parent=None;for s in &own{if family_handle(&actor,&s.id)?==handle&&initial.families.iter().any(|f|f.handle==handle){parent=Some(s.id.clone());break;}}
            let parent=parent.ok_or_else(||BusinessError::new(FailureKind::NotFound,"ACCOUNT_FAMILY_NOT_FOUND","This sign-in is no longer available."))?;
            let ids=before.sessions.iter().filter(|s|s.id==parent||before.bindings.iter().any(|b|b.session_id==s.id&&b.parent_session_id.as_deref()==Some(parent.as_str())&&b.environment=="development")).map(|s|s.id.clone()).collect::<Vec<_>>();
            if ids.iter().any(|id|!own.iter().any(|s|&s.id==id)){return Err(changed().into());}ids
        }else{own.iter().map(|s|s.id.clone()).collect::<Vec<_>>()};
        invalidate(connection,&before,&ids,invalidation.as_ref()).await?;
        // Resample after any Holding wait, before the destructive Control effect.
        fresh.require_current(connection,clock.now().get()).await?;
        sqlx::query("DELETE FROM session WHERE id=ANY($1::text[])").bind(&ids).execute(&mut *connection).await?;
        let mut expected=before.clone();expected.sessions.retain(|s|!ids.contains(&s.id));expected.verifications.retain(|v|!ids.contains(&v.session_id));
        let result=RevokedSecurity{revoked:true,count:ids.len(),reauthenticate:ids.contains(&actor.session_id)};let now=clock.now().get();
        sqlx::query("INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES($1,'account',$2,$3,$4,$5)").bind(&key).bind(&actor.user_id).bind(hash).bind(serde_json::to_value(&result).map_err(|_|super::account_sessions::persistence())?).bind(now).execute(&mut *connection).await?;
        sqlx::query("INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES($1,'account',$2,'self','account.sessions.revoked',$3,$4,$5)").bind(format!("account-security:{key}")).bind(&actor.user_id).bind(&input.scope).bind(serde_json::json!({"reason":input.reason,"scope":input.scope,"count":ids.len(),"currentFamily":result.reauthenticate})).bind(now).execute(&mut *connection).await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
        if source(connection,&actor,&before.sessions.iter().map(|s|s.id.clone()).collect::<Vec<_>>()).await?!=expected||clock.now().get()>=fresh.expires_at(){return Err(changed().into());}
        if !result.reauthenticate{fresh.require_current(connection,clock.now().get()).await?;}
        Ok(result)
    })).await)
        })
    }
}
