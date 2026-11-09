//! Staged observation of the legacy native engineering database predicate.
//! A roster passed here is parsed data, not proof of a sealed attestation.
//! This result grants no authority and is not a release/rollout decision.
use erpx_server_runtime::public::{
    native_engineering_roster::NativeEngineeringRoster,
    postgres::{PersistenceError, PgTransactionError},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String);
fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("NATIVE_ENGINEERING_IDENTITY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text").fetch_one(c).await?)
}
async fn matches(c: &mut PgConnection, roster: &NativeEngineeringRoster) -> Result<bool, Error> {
    let entries: Vec<_> = roster
        .entries()
        .iter()
        .filter(|entry| matches!(entry.binding(), "H001_DB" | "H002_DB" | "H003_DB"))
        .collect();
    let names: Vec<_> = entries.iter().map(|entry| entry.token()).collect();
    let oids: Vec<i64> = entries
        .iter()
        .map(|entry| i64::from(entry.oid()))
        .collect();
    Ok(sqlx::query_scalar("SELECT current_user='erpx_test_admin' AND session_user='erpx_test_admin' AND floor(extract(epoch FROM pg_postmaster_start_time())*1000)::bigint=$1 AND EXISTS(SELECT 1 FROM pg_database d JOIN pg_roles role ON role.rolname=current_user WHERE d.datname=current_database() AND pg_get_userbyid(d.datdba)='erpx_test_admin' AND NOT role.rolsuper AND NOT role.rolcreaterole AND NOT role.rolreplication AND NOT role.rolbypassrls AND role.rolcanlogin AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','UPDATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','UPDATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','DELETE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','DELETE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','INSERT') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','INSERT') AND NOT has_table_privilege(current_user,'pg_catalog.pg_database','TRUNCATE') AND NOT has_table_privilege(current_user,'pg_catalog.pg_shdescription','TRUNCATE') AND NOT EXISTS(SELECT 1 FROM aclexplode(COALESCE(d.datacl,acldefault('d',d.datdba))) acl WHERE acl.grantee=0 AND acl.privilege_type='CONNECT') AND EXISTS(SELECT 1 FROM unnest($2::text[],$3::bigint[]) allowed(token,oid) WHERE d.datname='erpx_dev_rust_'||allowed.token AND d.oid::bigint=allowed.oid AND shobj_description(d.oid,'pg_database')='erpx-rust-test:'||allowed.token))").bind(roster.postmaster_started_ms()).bind(names).bind(oids).fetch_one(c).await?)
}
/// The retained TS predicate's non-roster branch accepts only the exact three
/// browser database names for this run. It is not a role or ACL grant.
async fn legacy_matches(c: &mut PgConnection, run: &str) -> Result<bool, Error> {
    if !hexadecimal(run, 12) {
        return Ok(false);
    }
    Ok(sqlx::query_scalar("SELECT current_database() IN ($1,$2,$3)")
        .bind(format!("erpx_browser_{run}_h001_db"))
        .bind(format!("erpx_browser_{run}_h002_db"))
        .bind(format!("erpx_browser_{run}_h003_db"))
        .fetch_one(c)
        .await?)
}
async fn identity(
    c: &mut PgConnection,
    run: &str,
    roster: Option<&NativeEngineeringRoster>,
) -> Result<bool, Error> {
    match roster {
        Some(roster) => matches(c, roster).await,
        None => legacy_matches(c, run).await,
    }
}
pub struct CurrentNativeEngineeringDatabase {
    unit: Unit,
    roster: NativeEngineeringRoster,
    matches: bool,
}
impl CurrentNativeEngineeringDatabase {
    pub fn matches(&self) -> bool {
        self.matches
    }
    /// Re-evaluates identity facts. Separately retain and recheck sealed file,
    /// process, context and release evidence; no catalog-lock guarantee is made.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit || matches(c, &self.roster).await? != self.matches {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "NATIVE_ENGINEERING_DATABASE_CHANGED",
                "The native engineering database identity changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_native_engineering_database(
    c: &mut PgConnection,
    roster: &NativeEngineeringRoster,
) -> Result<CurrentNativeEngineeringDatabase, Error> {
    let unit = unit(&mut *c).await?;
    let matches = matches(c, roster).await?;
    Ok(CurrentNativeEngineeringDatabase {
        unit,
        roster: roster.clone(),
        matches,
    })
}
/// Current identity for either branch of the retained TS predicate: a sealed
/// native roster, or its exact browser-name fallback. This is a source
/// observation only; attestation, enrollment, authority and release remain
/// independent gates.
pub struct CurrentNativeEngineeringIdentity {
    unit: Unit,
    run: String,
    roster: Option<NativeEngineeringRoster>,
    matches: bool,
}
impl CurrentNativeEngineeringIdentity {
    pub fn matches(&self) -> bool {
        self.matches
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || identity(c, &self.run, self.roster.as_ref()).await? != self.matches
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "NATIVE_ENGINEERING_IDENTITY_CHANGED",
                "The native engineering identity changed.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn inspect_native_engineering_identity(
    c: &mut PgConnection,
    run: &str,
    roster: Option<&NativeEngineeringRoster>,
) -> Result<CurrentNativeEngineeringIdentity, Error> {
    let unit = unit(&mut *c).await?;
    let matches = identity(c, run, roster).await?;
    Ok(CurrentNativeEngineeringIdentity {
        unit,
        run: run.to_owned(),
        roster: roster.cloned(),
        matches,
    })
}
