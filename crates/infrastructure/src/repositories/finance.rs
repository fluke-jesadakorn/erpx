use super::*;

/// Control-plane routing metadata for a Finance bank-provider account.
///
/// `credential_key_ref` is only an opaque secret-manager lookup key. The
/// binding contains no credential bytes, provider payload, access token, or
/// payment instrument. Its `Debug` implementation deliberately redacts the
/// key reference so accidental diagnostics cannot disclose it.
#[derive(Clone, PartialEq, Eq)]
pub struct FinanceBankProviderAccountBinding {
    pub provider_account_ref: String,
    pub provider_key: String,
    pub tenant_id: String,
    pub holding_ref: String,
    pub holding_id: Uuid,
    pub credential_key_ref: String,
    pub state: String,
    pub version: u64,
}

impl fmt::Debug for FinanceBankProviderAccountBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FinanceBankProviderAccountBinding")
            .field("provider_account_ref", &self.provider_account_ref)
            .field("provider_key", &self.provider_key)
            .field("tenant_id", &self.tenant_id)
            .field("holding_ref", &self.holding_ref)
            .field("holding_id", &self.holding_id)
            .field("credential_key_ref", &"[redacted]")
            .field("state", &self.state)
            .field("version", &self.version)
            .finish()
    }
}

impl FinanceBankProviderAccountBinding {
    pub fn credential_key_ref(&self) -> &str {
        &self.credential_key_ref
    }
}

impl SqlxControlPlane {
    /// Resolve an exact Finance bank-provider route from authoritative
    /// control-plane state. Every route component is matched, and both the
    /// account and its tenant must be active before the binding is returned.
    /// The credential value is only an opaque secret-manager key reference.
    pub async fn resolve_finance_bank_provider_account(
        &self,
        provider_account_ref: &str,
        provider_key: &str,
        tenant_id: &str,
        holding_ref: &str,
        credential_key_ref: &str,
    ) -> Result<Option<FinanceBankProviderAccountBinding>, PersistenceError> {
        validate_tenant_reference(provider_account_ref, "provider_account_ref")?;
        validate_tenant_reference(provider_key, "provider_key")?;
        validate_tenant_reference(tenant_id, "tenant_id")?;
        validate_tenant_reference(holding_ref, "holding_ref")?;
        validate_tenant_reference(credential_key_ref, "credential_key_ref")?;

        let row = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.tenant_id, account.holding_ref, account.holding_id,
                    account.credential_key_ref, account.state, account.version
             FROM erpx_control.finance_bank_provider_accounts account
             INNER JOIN erpx_control.tenant_registry tenant
                 ON tenant.tenant_id = account.tenant_id
                AND tenant.holding_ref = account.holding_ref
             WHERE account.provider_account_ref = $1
               AND account.provider_key = $2
               AND account.tenant_id = $3
               AND account.holding_ref = $4
               AND account.credential_key_ref = $5
               AND account.state = 'active'
               AND tenant.state = 'active'",
        )
        .bind(provider_account_ref)
        .bind(provider_key)
        .bind(tenant_id)
        .bind(holding_ref)
        .bind(credential_key_ref)
        .fetch_optional(&self.pools.write)
        .await?;

        row.map(|row| {
            let holding_id: Uuid = row.try_get("holding_id")?;
            if holding_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "finance provider account holding is invalid".into(),
                ));
            }
            let version =
                positive_u64(row.try_get("version")?, "finance provider account version")?;
            Ok(FinanceBankProviderAccountBinding {
                provider_account_ref: row.try_get("provider_account_ref")?,
                provider_key: row.try_get("provider_key")?,
                tenant_id: row.try_get("tenant_id")?,
                holding_ref: row.try_get("holding_ref")?,
                holding_id,
                credential_key_ref: row.try_get("credential_key_ref")?,
                state: row.try_get("state")?,
                version,
            })
        })
        .transpose()
    }
}
