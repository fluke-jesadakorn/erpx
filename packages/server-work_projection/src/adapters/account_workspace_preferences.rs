//! Control-owned navigation preferences. A stored Company/path is only an
//! untrusted selector; Identity must still check grants before opening work.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId, UserId};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "WORKSPACE_PREFERENCE_CHANGED",
        "Review your current workspace preference.",
    )
    .into()
}

/// Zod `string.max` counts Unicode scalar values (code points), not UTF-8 bytes.
/// This bound mirrors the gateway's `workspaceKey.max(4096)` check.
fn cp_len(value: &str) -> usize {
    value.chars().count()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("WORKSPACE_PREFERENCE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_account_workspace_preferences(
    c: &mut PgConnection,
    write: bool,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query(if write{"LOCK TABLE account_workspace_preferences,account_company_preferences IN SHARE ROW EXCLUSIVE MODE"}else{"LOCK TABLE account_workspace_preferences,account_company_preferences IN SHARE MODE"}).execute(c).await?;
    Ok(())
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AccountWorkspacePreference {
    pub workspace_path: Option<String>,
    pub platform_assignment_id: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AccountCompanyPreference {
    pub company_id: String,
}
#[derive(PartialEq, Eq)]
struct Image {
    workspace: Option<Value>,
    company: Option<Value>,
}
async fn image(c: &mut PgConnection, user: &UserId, holding: &HoldingId) -> Result<Image, Error> {
    let workspace:Option<Value>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(p)::text)<=16384 THEN to_jsonb(p) ELSE NULL END FROM account_workspace_preferences p WHERE user_id=$1 FOR SHARE").bind(user.as_str()).fetch_optional(&mut *c).await?.map(|v:Option<Value>|v.ok_or_else(changed)).transpose()?;
    let company:Option<Value>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(p)::text)<=16384 THEN to_jsonb(p) ELSE NULL END FROM account_company_preferences p WHERE user_id=$1 AND holding_id=$2 FOR SHARE").bind(user.as_str()).bind(holding.as_str()).fetch_optional(&mut *c).await?.map(|v:Option<Value>|v.ok_or_else(changed)).transpose()?;
    Ok(Image { workspace, company })
}
pub struct CurrentAccountWorkspacePreferences {
    unit: Unit,
    user: UserId,
    holding: HoldingId,
    image: Image,
}
impl CurrentAccountWorkspacePreferences {
    pub fn workspace(&self) -> Result<Option<AccountWorkspacePreference>, Error> {
        self.image
            .workspace
            .as_ref()
            .map(|v| {
                let nullable = |key: &str| -> Result<Option<String>, Error> {
                    if v[key].is_null() {
                        Ok(None)
                    } else {
                        v[key].as_str().map(|s| Some(s.into())).ok_or_else(changed)
                    }
                };
                Ok(AccountWorkspacePreference {
                    workspace_path: nullable("workspace_path")?,
                    platform_assignment_id: nullable("platform_assignment_id")?,
                })
            })
            .transpose()
    }
    pub fn company(&self) -> Result<Option<AccountCompanyPreference>, Error> {
        self.image
            .company
            .as_ref()
            .map(|v| {
                Ok(AccountCompanyPreference {
                    company_id: v["company_id"].as_str().ok_or_else(changed)?.into(),
                })
            })
            .transpose()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.user, &self.holding).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn remember_workspace(
        self,
        c: &mut PgConnection,
        path: &str,
        company: Option<&str>,
        now: i64,
    ) -> Result<Self, Error> {
        self.require_current(c).await?;
        // Path length is a code-point length check like the gateway's zod bound.
        if path.is_empty()
            || cp_len(path) > 4096
            || path.chars().any(char::is_control)
            || !(0..=9_007_199_254_740_991).contains(&now)
        {
            return Err(changed());
        }
        let expected_workspace = json!({"user_id":self.user.as_str(),"workspace_path":path,"platform_assignment_id":self.image.workspace.as_ref().and_then(|v|v["platform_assignment_id"].as_str()),"updated_at":now});
        let expected_company=company.map(|id|json!({"user_id":self.user.as_str(),"holding_id":self.holding.as_str(),"company_id":id,"updated_at":now})).or(self.image.company.clone());
        sqlx::query("INSERT INTO account_workspace_preferences(user_id,workspace_path,updated_at)VALUES($1,$2,$3)ON CONFLICT(user_id)DO UPDATE SET workspace_path=excluded.workspace_path,updated_at=excluded.updated_at").bind(self.user.as_str()).bind(path).bind(now).execute(&mut *c).await?;
        if let Some(company) = company {
            sqlx::query("INSERT INTO account_company_preferences(user_id,holding_id,company_id,updated_at)VALUES($1,$2,$3,$4)ON CONFLICT(user_id,holding_id)DO UPDATE SET company_id=excluded.company_id,updated_at=excluded.updated_at").bind(self.user.as_str()).bind(self.holding.as_str()).bind(company).bind(now).execute(&mut *c).await?;
        }
        let current = Self {
            image: Image {
                workspace: Some(expected_workspace),
                company: expected_company,
            },
            ..self
        };
        current.require_current(c).await?;
        Ok(current)
    }
}
pub async fn read_account_workspace_preferences(
    c: &mut PgConnection,
    user: &UserId,
    holding: &HoldingId,
) -> Result<CurrentAccountWorkspacePreferences, Error> {
    Ok(CurrentAccountWorkspacePreferences {
        unit: unit(c).await?,
        user: user.clone(),
        holding: holding.clone(),
        image: image(c, user, holding).await?,
    })
}
