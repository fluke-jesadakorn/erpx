use crate::{
    adapters::{
        account_postgres::{credential, finish},
        postgres_effective_authority::lock_effective_authority_sources,
        postgres_invitation_authority::inspect_current,
        work_admission::work_module_catalog,
    },
    application::{
        account_credentials::InvitationCredentialQuery,
        account_invitations::{AccountInvitationRepository, InvitationAccountState},
        account_ports::AccountFuture,
    },
    domain::{
        account_auth::{EmailAddress, PasswordHash, SecretDigest},
        account_credentials::signup_unavailable,
        invitation_authority::StaffInvitationGrant,
    },
};
use erpx_server_people_payroll::public::{
    identity_facts::PgPeopleAuthorityReader,
    invitation_person_link::{
        InvitationPersonLinkRequest, PgInvitationPersonLinker, link_invited_person,
        lock_invitation_person_link_sources,
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgRequestLease, PgTransactionError, PgUnitOfWork, SessionLock, TransactionOptions,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::{collections::BTreeMap, sync::Arc};

pub struct PostgresAccountInvitations {
    control: DatabasePool,
    holdings: BTreeMap<HoldingId, DatabasePool>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountInvitations {
    pub fn new(
        control: DatabasePool,
        holdings: BTreeMap<HoldingId, DatabasePool>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            control,
            holdings,
            clock,
        }
    }
}
type Tx<T> = Result<T, PgTransactionError<BusinessError>>;
async fn locks(connection: &mut PgConnection) -> Tx<()> {
    sqlx::query("LOCK TABLE \"user\",account,principals,account_invitations,account_invitation_claims,holding_bindings IN SHARE MODE").execute(connection).await?;
    Ok(())
}
async fn query(
    connection: &mut PgConnection,
    digest: &SecretDigest,
    now: i64,
) -> Tx<(InvitationCredentialQuery, InvitationAccountState)> {
    let row = sqlx::query("SELECT * FROM account_invitations WHERE token_hash=$1 AND (status='pending' OR status='accepted' AND accepted_at IS NULL) AND created_at<=$2 AND expires_at>$2")
        .bind(digest.as_str()).bind(now).fetch_optional(&mut *connection).await?.ok_or_else(signup_unavailable)?;
    let email = EmailAddress::new(&row.try_get::<String, _>("email")?)?;
    StaffInvitationGrant::parse(&row.try_get::<String, _>("role")?)?;
    let invitation_id: String = row.try_get("id")?;
    let target = format!("usr_invite_{invitation_id}");
    let current = credential(connection, &email).await?;
    let status: String = row.try_get("status")?;
    if status == "pending" && current.is_some()
        || status == "accepted"
            && (row
                .try_get::<Option<String>, _>("accepted_user_id")?
                .as_deref()
                != Some(&target)
                || !current
                    .as_ref()
                    .is_some_and(|c| c.user_id == target && c.can_authenticate(false)))
    {
        return Err(signup_unavailable().into());
    }
    let created_by: String = row.try_get("created_by")?;
    let holding_id: String = row.try_get("holding_id")?;
    let authorizer_control_active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u LEFT JOIN principals p ON p.user_id=u.id WHERE u.id=$1 AND COALESCE(p.status,'active')='active')").bind(&created_by).fetch_one(&mut *connection).await?;
    let holding_binding_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND status='active')",
    )
    .bind(&holding_id)
    .fetch_one(&mut *connection)
    .await?;
    if !authorizer_control_active || !holding_binding_active {
        return Err(signup_unavailable().into());
    }
    if status == "accepted" {
        let claim: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM account_invitation_claims WHERE invitation_id=$1 AND eligible=1)").bind(&invitation_id).fetch_one(&mut *connection).await?;
        if !claim {
            return Err(signup_unavailable().into());
        }
    }
    let name: String = row.try_get("name")?;
    Ok((
        InvitationCredentialQuery {
            invitation_id,
            holding_id,
            company_id: row.try_get("company_id")?,
            person_id: row.try_get("person_id")?,
            name: name.clone(),
            role_payload: row.try_get("role")?,
            scope_kind: row.try_get("scope_kind")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            created_by,
            assignment_id: row.try_get("assignment_id")?,
            expires_at: row.try_get("expires_at")?,
            created_at: row.try_get("created_at")?,
            authorizer_control_active,
            holding_binding_active,
            email: email.clone(),
            now,
        },
        InvitationAccountState {
            name,
            email: email.as_str().to_owned(),
            credential: current,
        },
    ))
}
async fn activate_holding(
    connection: &mut PgConnection,
    request: &InvitationCredentialQuery,
    clock: &(dyn Clock + Send + Sync),
) -> Tx<()> {
    // Strong write-source locks precede the first snapshot; no source lease is
    // upgraded after concurrent writers have taken their own policy snapshots.
    sqlx::query("LOCK TABLE assignments,iam_audit,command_receipts IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *connection)
        .await?;
    lock_invitation_person_link_sources(&mut *connection).await?;
    lock_effective_authority_sources(&mut *connection).await?;
    sqlx::query("LOCK TABLE iam_reviews IN SHARE MODE")
        .execute(&mut *connection)
        .await?;
    let now = clock.now().get();
    inspect_current(connection, request, now, true).await?;
    let grant = StaffInvitationGrant::parse(&request.role_payload)?;
    let user = format!("usr_invite_{}", request.invitation_id);
    let assignment = format!("assignment-{}", request.invitation_id);
    let command = format!("activate-{}", request.invitation_id);
    let payload = serde_json::json!({"invitation":request.invitation_id,"holding":request.holding_id,"company":request.company_id,"person":request.person_id,"user":user,"role":grant.role,"scope":request.scope_kind,"branch":request.branch_id,"department":request.department_id,"parent":request.assignment_id,"until":grant.expires});
    let modules = &work_module_catalog()?
        .definitions()
        .iter()
        .find(|r| r.id == grant.role)
        .ok_or_else(signup_unavailable)?
        .modules;
    let scope_node = if request.scope_kind == "department" {
        request.department_id.as_deref()
    } else {
        request.branch_id.as_deref()
    };
    let hash = hex::encode(Sha256::digest(payload.to_string().as_bytes()));
    if let Some(receipt) = sqlx::query("SELECT operation,actor_id,request_hash,record_id,result_json,created_at FROM command_receipts WHERE command_id=$1").bind(&command).fetch_optional(&mut *connection).await? {
        if receipt.try_get::<String,_>("operation")? != "identity.account.activate" || receipt.try_get::<String,_>("actor_id")? != request.created_by || receipt.try_get::<String,_>("request_hash")? != hash || receipt.try_get::<String,_>("record_id")? != request.person_id || receipt.try_get::<Option<String>,_>("result_json")?.is_none() { return Err(signup_unavailable().into()); }
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND parent_id=$4 AND role=$5 AND iam_assignment_active(id,$6) AND scope_kind=$7 AND scope_node_id IS NOT DISTINCT FROM $8 AND branch_id IS NOT DISTINCT FROM $9 AND valid_until=$10 AND valid_from=$11 AND modules_json::jsonb=$12::jsonb)")
            .bind(&assignment).bind(&user).bind(&request.company_id).bind(&request.assignment_id).bind(&grant.role).bind(now).bind(&request.scope_kind).bind(scope_node).bind(&request.branch_id).bind(grant.expires).bind(receipt.try_get::<i64,_>("created_at")?).bind(serde_json::to_string(modules).map_err(|_| signup_unavailable())?).fetch_one(&mut *connection).await?;
        if !active { return Err(signup_unavailable().into()); }
        let linked = PgPeopleAuthorityReader::new(&mut *connection).person_subject(&CompanyId::new(request.company_id.clone())?, &RecordId::new(request.person_id.clone())?).await?.is_some_and(|person| person.user_id.as_ref().is_some_and(|id| id.as_str() == user));
        if !linked { return Err(signup_unavailable().into()); }
        inspect_current(connection, request, clock.now().get(), true).await?;
        return Ok(());
    }
    let company = CompanyId::new(request.company_id.clone())?;
    let person_id = RecordId::new(request.person_id.clone())?;
    let person = PgPeopleAuthorityReader::new(&mut *connection)
        .person_subject(&company, &person_id)
        .await?
        .ok_or_else(signup_unavailable)?;
    let engagement = person
        .active_engagements
        .iter()
        .find(|e| {
            request
                .branch_id
                .as_deref()
                .is_none_or(|id| e.branch_id.as_ref().map(|v| v.as_str()) == Some(id))
                && request
                    .department_id
                    .as_deref()
                    .is_none_or(|id| e.department_id.as_ref().map(|v| v.as_str()) == Some(id))
        })
        .ok_or_else(signup_unavailable)?;
    link_invited_person(
        &mut PgInvitationPersonLinker::new(&mut *connection),
        &InvitationPersonLinkRequest {
            company_id: company,
            person_id,
            user_id: UserId::new(user.clone())?,
            expected_person_version: person.version,
            engagement_id: engagement.id.clone(),
            expected_engagement_version: engagement.version,
            branch_id: request.branch_id.clone().map(RecordId::new).transpose()?,
            department_id: request
                .department_id
                .clone()
                .map(RecordId::new)
                .transpose()?,
            now: TimestampMillis::new(now),
        },
    )
    .await?;
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'manual')")
        .bind(&assignment).bind(&user).bind(&grant.role).bind(serde_json::to_string(modules).map_err(|_| signup_unavailable())?).bind(&request.company_id).bind(&request.branch_id).bind(now).bind(grant.expires).bind(&request.scope_kind).bind(scope_node).bind(&request.assignment_id).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES($1,$2,$3,$4,'identity.account.activate',$5,$6,$7)")
        .bind(format!("native-invitation-{}",request.invitation_id)).bind(&request.company_id).bind(&request.created_by).bind(&request.assignment_id).bind(&request.person_id).bind(payload).bind(now).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,'identity.account.activate',$2,$3,$4,$5,$6)")
        .bind(command).bind(&request.created_by).bind(hash).bind(&request.person_id).bind(serde_json::json!({"activated":true,"assignmentId":assignment}).to_string()).bind(now).execute(&mut *connection).await?;
    // The final time/current-role/Person check shares the retained source locks.
    inspect_current(connection, request, clock.now().get(), true).await?;
    Ok(())
}
impl AccountInvitationRepository for PostgresAccountInvitations {
    fn current(&self, digest: SecretDigest, now: i64) -> AccountFuture<'_, InvitationAccountState> {
        Box::pin(async move {
            finish(
                PgRequestLease::new(self.control.clone())
                    .run(|connection| {
                        Box::pin(async move {
                            locks(connection).await?;
                            query(connection, &digest, now)
                                .await
                                .map(|(_, state)| state)
                        })
                    })
                    .await,
            )
        })
    }
    fn activate(&self, digest: SecretDigest, verified_hash: PasswordHash) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            let holdings = self.holdings.clone();
            let clock = self.clock.clone();
            let retained_digest = digest.clone();
            let retained_hash = verified_hash.clone();
            let invitation = finish(
                PgRequestLease::new(self.control.clone())
                    .run(move |connection| {
                        Box::pin(async move {
                            locks(connection).await?;
                            let (request, state) =
                                query(connection, &retained_digest, clock.now().get()).await?;
                            if !state
                                .credential
                                .is_some_and(|c| c.password_hash == retained_hash)
                            {
                                return Err(signup_unavailable().into());
                            }
                            let pool = holdings
                                .get(&HoldingId::new(request.holding_id.clone())?)
                                .ok_or_else(signup_unavailable)?
                                .clone();
                            let id = request.invitation_id.clone();
                            PgUnitOfWork::new(pool)
                                .run(
                                    TransactionOptions {
                                        session_lock: Some(SessionLock {
                                            namespace: "invitation-activation".into(),
                                            key: id.clone(),
                                        }),
                                        prefer_indexed_reads: false,
                                    },
                                    move |connection| {
                                        let request = request.clone();
                                        let clock = clock.clone();
                                        Box::pin(async move {
                                            activate_holding(connection, &request, clock.as_ref())
                                                .await
                                        })
                                    },
                                )
                                .await?;
                            Ok(id)
                        })
                    })
                    .await,
            )?;
            // A separate retryable Control-only finalization follows the durable
            // Holding commit. It can never replay that commit or grant effects.
            let clock = self.clock.clone();
            finish(PgUnitOfWork::new(self.control.clone()).run(TransactionOptions::default(),move |connection| {let id=invitation.clone();let digest=digest.clone();let hash=verified_hash.clone();let clock=clock.clone();Box::pin(async move {
                sqlx::query("LOCK TABLE \"user\",account,principals,account_invitations,account_invitation_claims IN SHARE ROW EXCLUSIVE MODE").execute(&mut *connection).await?;
                let row=sqlx::query("SELECT email,accepted_user_id FROM account_invitations WHERE id=$1 AND token_hash=$2 AND status='accepted'").bind(&id).bind(digest.as_str()).fetch_optional(&mut *connection).await?.ok_or_else(signup_unavailable)?;
                let current=credential(connection,&EmailAddress::new(&row.try_get::<String,_>("email")?)?).await?.ok_or_else(signup_unavailable)?;
                if current.user_id!=format!("usr_invite_{id}") || row.try_get::<Option<String>,_>("accepted_user_id")?.as_deref()!=Some(&current.user_id) || current.password_hash!=hash || !current.can_authenticate(false) { return Err(signup_unavailable().into()); }
                sqlx::query("UPDATE account_invitations SET accepted_at=COALESCE(accepted_at,$2) WHERE id=$1 AND status='accepted'").bind(id).bind(clock.now().get()).execute(connection).await?;
                Ok(())
            })}).await)
        })
    }
}
