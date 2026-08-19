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
    pub holding_key: String,
    pub data_profile_key: String,
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
            .field("holding_id", &self.holding_id)
            .field("data_profile_key", &self.data_profile_key)
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
    /// account and its holding must be active before the binding is returned.
    /// The credential value is only an opaque secret-manager key reference.
    pub async fn resolve_finance_bank_provider_account(
        &self,
        provider_account_ref: &str,
        provider_key: &str,
        holding_id: &str,
        data_profile_key: &str,
        credential_key_ref: &str,
    ) -> Result<Option<FinanceBankProviderAccountBinding>, PersistenceError> {
        validate_data_profile_reference(provider_account_ref, "provider_account_ref")?;
        validate_data_profile_reference(provider_key, "provider_key")?;
        validate_data_profile_reference(holding_id, "holding_id")?;
        validate_data_profile_reference(data_profile_key, "data_profile_key")?;
        validate_data_profile_reference(credential_key_ref, "credential_key_ref")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;

        let row = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.holding_id, account.data_profile_key,
                    account.credential_key_ref, account.state, account.version
             FROM erpx_control.finance_bank_provider_accounts account
             INNER JOIN erpx_control.holding_registry holding
                 ON holding.holding_id = account.holding_id::text
                AND holding.data_profile_key = account.data_profile_key
             WHERE account.provider_account_ref = $1
               AND account.provider_key = $2
               AND account.holding_id = $3
               AND account.data_profile_key = $4
               AND account.credential_key_ref = $5
               AND account.state = 'active'
               AND holding.state = 'active'",
        )
        .bind(provider_account_ref)
        .bind(provider_key)
        .bind(holding_id)
        .bind(data_profile_key)
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
                holding_key: holding_id.to_string(),
                data_profile_key: row.try_get("data_profile_key")?,
                holding_id,
                credential_key_ref: row.try_get("credential_key_ref")?,
                state: row.try_get("state")?,
                version,
            })
        })
        .transpose()
    }
}
