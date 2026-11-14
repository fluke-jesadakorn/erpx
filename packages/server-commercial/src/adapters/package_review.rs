//! Existing package-lifecycle review wire and HMAC domain. A signed review is
//! never operator authority: the consuming unit must retain current IAM/MFA,
//! Company, catalogue, grant and settlement sources before writing.
use crate::{
    application::grant_batches::{GrantBatch, normalize_grant_batch},
    domain::{collation::package_id_collator, packages::CommercialCatalogue},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use erpx_server_shared::public::{BusinessError, FailureKind};
use ring::hmac;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DOMAIN: &str = "erpx-development-commercial-lifecycle-review-v1";
const LIFETIME: i64 = 300_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageReviewActor {
    pub user_id: String,
    pub session_id: String,
    pub assignment_id: String,
    pub role: String,
    pub role_revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageReviewClaims {
    pub version: i64,
    pub holding_id: String,
    pub company_id: String,
    pub actor: PackageReviewActor,
    pub request_hash: String,
    pub authority_revision: String,
    pub catalogue_hash: String,
    pub issued_at: i64,
    pub expires_at: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
pub struct PackageReview {
    pub token: String,
    pub expires_at: i64,
}
pub struct PackageLifecycleReviewSigner {
    key: hmac::Key,
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PACKAGE_REVIEW_INVALID",
        "Preview these Company packages again.",
    )
}
fn digest(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}
fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn safe(value: i64) -> bool {
    (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value)
}
fn json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, BusinessError> {
    serde_json::to_vec(value).map_err(|_| invalid())
}
fn request_hash(
    catalogue: &CommercialCatalogue,
    input: &GrantBatch,
) -> Result<String, BusinessError> {
    let mut normalized = normalize_grant_batch(catalogue, input)?;
    // Review ordering is package-id only. Application execution additionally
    // sorts bases before addons; those are deliberately different contracts.
    let collator = package_id_collator()?;
    normalized
        .changes
        .sort_by(|a, b| collator.compare(&a.grant.package_id, &b.grant.package_id));
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Request<'a> {
        company_id: &'a str,
        expected_revision: &'a str,
        idempotency_key: &'a str,
        changes: &'a [crate::application::grant_batches::BatchChange],
    }
    Ok(digest(&json(&Request {
        company_id: &input.company_id,
        expected_revision: &input.expected_revision,
        idempotency_key: &input.idempotency_key,
        changes: &normalized.changes,
    })?))
}
impl PackageLifecycleReviewSigner {
    pub fn new(secret: &str) -> Result<Self, BusinessError> {
        if secret.chars().map(char::len_utf16).sum::<usize>() < 32 {
            return Err(BusinessError::invalid(
                "PACKAGE_REVIEW_SECRET_REQUIRED",
                "Configure the package review signing key.",
            ));
        }
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes()),
        })
    }
    pub fn issue(
        &self,
        catalogue: &CommercialCatalogue,
        actor: PackageReviewActor,
        holding: &str,
        input: &GrantBatch,
        authority_revision: &str,
        now: i64,
    ) -> Result<PackageReview, BusinessError> {
        let expires_at = now
            .checked_add(LIFETIME)
            .filter(|n| safe(*n))
            .ok_or_else(invalid)?;
        if holding.is_empty()
            || actor.role_revision < 1
            || !safe(actor.role_revision)
            || !is_hash(authority_revision)
            || !safe(now)
        {
            return Err(invalid());
        }
        let claims = PackageReviewClaims {
            version: 1,
            holding_id: holding.into(),
            company_id: input.company_id.clone(),
            actor,
            request_hash: request_hash(catalogue, input)?,
            authority_revision: authority_revision.into(),
            catalogue_hash: digest(&json(catalogue.definitions())?),
            issued_at: now,
            expires_at,
        };
        let body = URL_SAFE_NO_PAD.encode(json(&claims)?);
        let signature = hmac::sign(&self.key, format!("{DOMAIN}:{body}").as_bytes());
        Ok(PackageReview {
            token: format!("{body}.{}", URL_SAFE_NO_PAD.encode(signature.as_ref())),
            expires_at,
        })
    }
    pub fn read(
        &self,
        token: &str,
        catalogue: &CommercialCatalogue,
        actor: &PackageReviewActor,
        holding: &str,
        input: &GrantBatch,
        now: i64,
    ) -> Result<PackageReviewClaims, BusinessError> {
        if token.len() > 8192 || !safe(now) {
            return Err(invalid());
        }
        let (body, mac) = token.split_once('.').ok_or_else(invalid)?;
        let alphabet = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        if !alphabet(body) || mac.len() != 43 || !alphabet(mac) {
            return Err(invalid());
        }
        let signature = URL_SAFE_NO_PAD.decode(mac).map_err(|_| invalid())?;
        hmac::verify(&self.key, format!("{DOMAIN}:{body}").as_bytes(), &signature)
            .map_err(|_| invalid())?;
        let claims: PackageReviewClaims =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
        if claims.version != 1
            || claims.actor != *actor
            || claims.actor.role_revision < 1
            || !safe(claims.actor.role_revision)
            || claims.holding_id != holding
            || holding.is_empty()
            || claims.company_id != input.company_id
            || !is_hash(&claims.authority_revision)
            || claims.request_hash != request_hash(catalogue, input)?
            || claims.catalogue_hash != digest(&json(catalogue.definitions())?)
            || !safe(claims.issued_at)
            || !safe(claims.expires_at)
            || claims.issued_at > now
            || claims.issued_at.checked_add(LIFETIME) != Some(claims.expires_at)
        {
            return Err(invalid());
        }
        // Current authority is mandatory even for receipt recovery. Expiry is
        // checked only inside the new-write callback, as in the original path.
        Ok(claims)
    }
    pub fn assert_fresh(
        &self,
        claims: &PackageReviewClaims,
        now: i64,
    ) -> Result<(), BusinessError> {
        if !safe(now) || claims.expires_at <= now {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PACKAGE_REVIEW_EXPIRED",
                "This preview expired. Preview the current Company again.",
            ));
        }
        Ok(())
    }
    pub fn review_hash(&self, token: &str) -> String {
        digest(token.as_bytes())
    }
}

/// The Commercial-owned review evidence writer used by the existing batch
/// command. Outer composition still owns current operator/MFA authority and
/// the final deferred Finance/source checks in the same Holding unit.
pub struct PgPackageLifecycleReview<'a, C> {
    signer: &'a PackageLifecycleReviewSigner,
    clock: &'a C,
    input: &'a GrantBatch,
    normalized: crate::application::grant_batches::NormalizedGrantBatch,
    claims: PackageReviewClaims,
    review_hash: String,
    unit: Option<(String, i32, String, String)>,
}
impl<'a, C: erpx_server_shared::public::Clock + Sync> PgPackageLifecycleReview<'a, C> {
    pub fn new(
        signer: &'a PackageLifecycleReviewSigner,
        clock: &'a C,
        catalogue: &CommercialCatalogue,
        actor: &PackageReviewActor,
        holding: &str,
        input: &'a GrantBatch,
        token: &str,
    ) -> Result<Self, BusinessError> {
        let claims = signer.read(token, catalogue, actor, holding, input, clock.now().get())?;
        Ok(Self {
            signer,
            clock,
            input,
            normalized: normalize_grant_batch(catalogue, input)?,
            claims,
            review_hash: signer.review_hash(token),
            unit: None,
        })
    }
    fn comparison(
        &self,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), BusinessError> {
        if preview.company_id != self.normalized.company_id
            || preview.expected_revision != self.normalized.expected_revision
            || preview.changes != self.normalized.changes
            || preview.after
                != crate::application::grant_batches::prospective_package_grants(
                    &preview.before,
                    &self.normalized,
                )?
            || crate::application::grant_batches::package_grant_revision(
                &preview.company_id,
                &preview.before,
            )? != self.claims.authority_revision
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "PACKAGE_REVIEW_CHANGED",
                "Company packages changed. Preview them again.",
            ));
        }
        self.signer
            .assert_fresh(&self.claims, self.clock.now().get())
    }
}
impl<C: erpx_server_shared::public::Clock + Sync>
    super::postgres_grant_commands::PgPackageBatchReview for PgPackageLifecycleReview<'_, C>
{
    async fn prepare_sources(
        &mut self,
        c: &mut sqlx::PgConnection,
        input: &crate::application::grant_commands::GrantChange,
    ) -> Result<(), erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>> {
        use sqlx::Connection;
        if !c.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(c)
                .await?
                != "serializable"
            || !self.normalized.changes.iter().any(|change| {
                change.expected_version == input.expected_version && change.grant == input.grant
            })
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    async fn verify(
        &mut self,
        c: &mut sqlx::PgConnection,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>> {
        self.comparison(preview)?;
        if self.unit.is_some() {
            return Err(invalid().into());
        }
        self.unit=Some(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(&mut *c).await?);
        let actor = &self.claims.actor;
        let comparison = serde_json::json!({"source":{"kind":"package-lifecycle","revision":self.claims.authority_revision},"before":preview.before,"after":preview.after});
        sqlx::query("INSERT INTO company_package_review_evidence(change_set_id,company_id,actor_id,assignment_id,role_id,role_revision,authority_revision,catalogue_hash,review_hash,reviewed_at,applied_at,comparison_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(&self.input.idempotency_key).bind(&preview.company_id).bind(&actor.user_id).bind(&actor.assignment_id).bind(&actor.role).bind(actor.role_revision).bind(&self.claims.authority_revision).bind(&self.claims.catalogue_hash).bind(&self.review_hash).bind(self.claims.issued_at).bind(self.clock.now().get()).bind(comparison).execute(c).await?;
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut sqlx::PgConnection,
        preview: &crate::application::grant_batches::GrantBatchPreview,
    ) -> Result<(), erpx_server_runtime::public::postgres::PgTransactionError<BusinessError>> {
        let unit:(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
        if self.unit.as_ref() != Some(&unit) {
            return Err(invalid().into());
        }
        self.comparison(preview)?;
        Ok(())
    }
}
