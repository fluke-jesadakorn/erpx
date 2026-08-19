use super::*;

pub const MARKETING_PUBLIC_ABUSE_WINDOW_SECONDS: i64 = 60;
pub const MAX_MARKETING_PUBLIC_ABUSE_LIMIT: u32 = 1_000;
pub const MAX_MARKETING_PROVIDER_ACCOUNTS_PER_HOLDING: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderChannelRouteBinding {
    pub channel: String,
    pub endpoint: String,
    pub reconciliation_endpoint: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MarketingProviderAccountBinding {
    pub provider_account_ref: String,
    pub provider_key: String,
    pub provider_priority: u32,
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub verifier_key_ref: String,
    pub outbound_credential_ref: String,
    pub channels: Vec<String>,
    pub channel_routes: Vec<MarketingProviderChannelRouteBinding>,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingPublicAbuseWindowRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub hostname: String,
    pub resource_kind: String,
    pub resource_id: Uuid,
}

impl fmt::Debug for MarketingProviderAccountBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MarketingProviderAccountBinding")
            .field("provider_account_ref", &self.provider_account_ref)
            .field("provider_key", &self.provider_key)
            .field("provider_priority", &self.provider_priority)
            .field("holding_id", &self.holding_id)
            .field("data_profile_key", &self.data_profile_key)
            .field("holding_id", &self.holding_id)
            .field("verifier_key_ref", &"[redacted]")
            .field("outbound_credential_ref", &"[redacted]")
            .field("channels", &self.channels)
            .field(
                "channel_routes",
                &self
                    .channel_routes
                    .iter()
                    .map(|route| &route.channel)
                    .collect::<Vec<_>>(),
            )
            .field("version", &self.version)
            .finish()
    }
}

fn decode_marketing_provider_channel_routes(
    row: &PgRow,
) -> Result<Vec<MarketingProviderChannelRouteBinding>, PersistenceError> {
    Ok(serde_json::from_value(row.try_get("channel_routes")?)?)
}

impl SqlxControlPlane {
    /// Atomically consume one anonymous Marketing request in a durable
    /// control-plane window. The bucket deliberately excludes client IP and
    /// contact data, so it is a coarse public-resource fence rather than a
    /// user identity store.
    pub async fn consume_marketing_public_abuse_window(
        &self,
        request: &MarketingPublicAbuseWindowRequest,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<bool, PersistenceError> {
        validate_data_profile_reference(&request.holding_id, "holding_id")?;
        validate_data_profile_reference(&request.data_profile_key, "data_profile_key")?;
        validate_public_hostname(&request.hostname)?;
        if !matches!(request.resource_kind.as_str(), "form" | "preview")
            || request.resource_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "Marketing public abuse resource is invalid".into(),
            ));
        }
        if limit == 0 || limit > MAX_MARKETING_PUBLIC_ABUSE_LIMIT {
            return Err(PersistenceError::InvalidData(
                "Marketing public abuse limit is outside its bound".into(),
            ));
        }
        let window_seconds = MARKETING_PUBLIC_ABUSE_WINDOW_SECONDS;
        let window_started_at = DateTime::<Utc>::from_timestamp(
            now.timestamp().div_euclid(window_seconds) * window_seconds,
            0,
        )
        .ok_or_else(|| {
            PersistenceError::InvalidData("Marketing public abuse timestamp is invalid".into())
        })?;
        let mut transaction = self.pools.write.begin().await?;
        let active = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM erpx_control.holding_registry
                 WHERE holding_id=$1 AND data_profile_key=$2 AND state='active'
            )",
        )
        .bind(&request.holding_id)
        .bind(&request.data_profile_key)
        .fetch_one(&mut *transaction)
        .await?;
        if !active {
            transaction.commit().await?;
            return Ok(false);
        }
        let count: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_control.marketing_public_abuse_windows
                (holding_id, data_profile_key, hostname, resource_kind, resource_id,
                 window_started_at, request_count, last_seen_at)
             VALUES ($1,$2,$3,$4,$5,$6,1,$7)
             ON CONFLICT (holding_id, data_profile_key, hostname, resource_kind,
                          resource_id, window_started_at)
             DO UPDATE SET request_count =
                               erpx_control.marketing_public_abuse_windows.request_count + 1,
                           last_seen_at = EXCLUDED.last_seen_at
             RETURNING request_count",
        )
        .bind(&request.holding_id)
        .bind(&request.data_profile_key)
        .bind(&request.hostname)
        .bind(&request.resource_kind)
        .bind(request.resource_id)
        .bind(window_started_at)
        .bind(now)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(count <= i64::from(limit))
    }

    /// Bound cleanup for expired public abuse windows. It is safe to run from
    /// multiple workers because deletion is idempotent and resource-scoped.
    pub async fn purge_marketing_public_abuse_windows(
        &self,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        if limit == 0 || limit > MAX_MARKETING_PUBLIC_ABUSE_LIMIT {
            return Err(PersistenceError::InvalidData(
                "Marketing public abuse cleanup limit is outside its bound".into(),
            ));
        }
        let result = sqlx::query(
            "DELETE FROM erpx_control.marketing_public_abuse_windows
              WHERE ctid IN (
                SELECT ctid FROM erpx_control.marketing_public_abuse_windows
                 WHERE last_seen_at < $1
                 ORDER BY last_seen_at
                 LIMIT $2
              )",
        )
        .bind(before)
        .bind(i64::from(limit))
        .execute(&self.pools.write)
        .await?;
        Ok(result.rows_affected())
    }

    /// Resolve callback routing from the control plane. The caller must still
    /// verify the raw provider signature with the deployment secret before
    /// using this result to touch a holding database.
    pub async fn resolve_marketing_provider_account(
        &self,
        provider_account_ref: &str,
        holding_id: &str,
        data_profile_key: &str,
        verifier_key_ref: &str,
    ) -> Result<Option<MarketingProviderAccountBinding>, PersistenceError> {
        validate_data_profile_reference(provider_account_ref, "provider_account_ref")?;
        validate_data_profile_reference(holding_id, "holding_id")?;
        validate_data_profile_reference(data_profile_key, "data_profile_key")?;
        validate_data_profile_reference(verifier_key_ref, "verifier_key_ref")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;
        let row = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.provider_priority,
                    account.holding_id,
                    account.data_profile_key,
                    account.verifier_key_ref, account.outbound_credential_ref,
                    account.channels, account.channel_routes, account.version
             FROM erpx_control.marketing_provider_accounts account
             INNER JOIN erpx_control.holding_registry holding
                 ON holding.holding_id = account.holding_id::text
                AND holding.data_profile_key = account.data_profile_key
             WHERE account.provider_account_ref = $1
               AND account.holding_id = $2
               AND account.data_profile_key = $3
               AND account.verifier_key_ref = $4
               AND account.state = 'active'
               AND account.onboarding_state = 'verified'
               AND holding.state = 'active'",
        )
        .bind(provider_account_ref)
        .bind(holding_id)
        .bind(data_profile_key)
        .bind(verifier_key_ref)
        .fetch_optional(&self.pools.write)
        .await?;
        row.map(|row| {
            let holding_id: Uuid = row.try_get("holding_id")?;
            if holding_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "marketing provider account holding is invalid".into(),
                ));
            }
            let version = positive_u64(
                row.try_get("version")?,
                "marketing provider account version",
            )?;
            let channel_routes = decode_marketing_provider_channel_routes(&row)?;
            Ok(MarketingProviderAccountBinding {
                provider_account_ref: row.try_get("provider_account_ref")?,
                provider_key: row.try_get("provider_key")?,
                provider_priority: positive_provider_priority(row.try_get("provider_priority")?)?,
                holding_key: holding_id.to_string(),
                data_profile_key: row.try_get("data_profile_key")?,
                holding_id,
                verifier_key_ref: row.try_get("verifier_key_ref")?,
                outbound_credential_ref: row.try_get("outbound_credential_ref")?,
                channels: row.try_get("channels")?,
                channel_routes,
                version,
            })
        })
        .transpose()
    }

    /// Resolve the exact active outbound account route for a holding-routed
    /// delivery worker. Callback verifier material is intentionally not part
    /// of this lookup; outbound workers receive only the opaque credential
    /// reference and still re-resolve this binding before each provider call.
    pub async fn resolve_marketing_provider_account_for_outbound(
        &self,
        provider_account_ref: &str,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<Option<MarketingProviderAccountBinding>, PersistenceError> {
        validate_data_profile_reference(provider_account_ref, "provider_account_ref")?;
        validate_data_profile_reference(holding_id, "holding_id")?;
        validate_data_profile_reference(data_profile_key, "data_profile_key")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;
        let row = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.provider_priority,
                    account.holding_id, account.data_profile_key,
                    account.verifier_key_ref, account.outbound_credential_ref,
                    account.channels, account.channel_routes, account.version
             FROM erpx_control.marketing_provider_accounts account
             INNER JOIN erpx_control.holding_registry holding
                 ON holding.holding_id = account.holding_id::text
                AND holding.data_profile_key = account.data_profile_key
             WHERE account.provider_account_ref = $1
               AND account.holding_id = $2
               AND account.data_profile_key = $3
               AND account.state = 'active'
               AND account.onboarding_state = 'verified'
               AND holding.state = 'active'",
        )
        .bind(provider_account_ref)
        .bind(holding_id)
        .bind(data_profile_key)
        .fetch_optional(&self.pools.write)
        .await?;
        row.map(|row| {
            let holding_id: Uuid = row.try_get("holding_id")?;
            if holding_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "marketing provider account holding is invalid".into(),
                ));
            }
            let channel_routes = decode_marketing_provider_channel_routes(&row)?;
            Ok(MarketingProviderAccountBinding {
                provider_account_ref: row.try_get("provider_account_ref")?,
                provider_key: row.try_get("provider_key")?,
                provider_priority: positive_provider_priority(row.try_get("provider_priority")?)?,
                holding_key: holding_id.to_string(),
                data_profile_key: row.try_get("data_profile_key")?,
                holding_id,
                verifier_key_ref: row.try_get("verifier_key_ref")?,
                outbound_credential_ref: row.try_get("outbound_credential_ref")?,
                channels: row.try_get("channels")?,
                channel_routes,
                version: positive_u64(
                    row.try_get("version")?,
                    "marketing provider account version",
                )?,
            })
        })
        .transpose()
    }

    /// List active outbound provider accounts for one active holding holding.
    /// Provider account discovery remains control-plane authoritative; no
    /// holding database or deployment environment value supplies this set.
    pub async fn list_marketing_provider_accounts_for_outbound(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<Vec<MarketingProviderAccountBinding>, PersistenceError> {
        validate_data_profile_reference(holding_id, "holding_id")?;
        validate_data_profile_reference(data_profile_key, "data_profile_key")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;
        let rows = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.provider_priority,
                    account.holding_id, account.data_profile_key,
                    account.verifier_key_ref, account.outbound_credential_ref,
                    account.channels, account.channel_routes, account.version
             FROM erpx_control.marketing_provider_accounts account
             INNER JOIN erpx_control.holding_registry holding
                 ON holding.holding_id = account.holding_id::text
                AND holding.data_profile_key = account.data_profile_key
             WHERE account.holding_id = $1
               AND account.data_profile_key = $2
               AND account.state = 'active'
               AND account.onboarding_state = 'verified'
               AND holding.state = 'active'
             ORDER BY account.provider_priority, account.provider_account_ref
             LIMIT $3",
        )
        .bind(holding_id)
        .bind(data_profile_key)
        .bind((MAX_MARKETING_PROVIDER_ACCOUNTS_PER_HOLDING + 1) as i64)
        .fetch_all(&self.pools.write)
        .await?;
        if rows.len() > MAX_MARKETING_PROVIDER_ACCOUNTS_PER_HOLDING {
            return Err(PersistenceError::InvalidData(
                "marketing provider account count is unbounded".into(),
            ));
        }
        rows.into_iter()
            .map(|row| {
                let holding_id: Uuid = row.try_get("holding_id")?;
                if holding_id.is_nil() {
                    return Err(PersistenceError::InvalidData(
                        "marketing provider account holding is invalid".into(),
                    ));
                }
                let channel_routes = decode_marketing_provider_channel_routes(&row)?;
                Ok(MarketingProviderAccountBinding {
                    provider_account_ref: row.try_get("provider_account_ref")?,
                    provider_key: row.try_get("provider_key")?,
                    provider_priority: positive_provider_priority(
                        row.try_get("provider_priority")?,
                    )?,
                    holding_key: holding_id.to_string(),
                    data_profile_key: row.try_get("data_profile_key")?,
                    holding_id,
                    verifier_key_ref: row.try_get("verifier_key_ref")?,
                    outbound_credential_ref: row.try_get("outbound_credential_ref")?,
                    channels: row.try_get("channels")?,
                    channel_routes,
                    version: positive_u64(
                        row.try_get("version")?,
                        "marketing provider account version",
                    )?,
                })
            })
            .collect()
    }

    /// Resolve callback routing by the provider account identifier alone.
    /// Holding and holding values are returned from the control plane rather
    /// than accepted from callback headers.
    pub async fn resolve_marketing_provider_account_for_callback(
        &self,
        provider_account_ref: &str,
    ) -> Result<Option<MarketingProviderAccountBinding>, PersistenceError> {
        validate_data_profile_reference(provider_account_ref, "provider_account_ref")?;
        let row = sqlx::query(
            "SELECT account.provider_account_ref, account.provider_key,
                    account.provider_priority,
                    account.holding_id, account.data_profile_key,
                    account.verifier_key_ref, account.outbound_credential_ref,
                    account.channels, account.channel_routes, account.version
             FROM erpx_control.marketing_provider_accounts account
             INNER JOIN erpx_control.holding_registry holding
                 ON holding.holding_id = account.holding_id::text
                AND holding.data_profile_key = account.data_profile_key
             WHERE account.provider_account_ref = $1
               AND account.state = 'active'
               AND account.onboarding_state = 'verified'
               AND holding.state = 'active'",
        )
        .bind(provider_account_ref)
        .fetch_optional(&self.pools.write)
        .await?;
        row.map(|row| {
            let holding_id: Uuid = row.try_get("holding_id")?;
            if holding_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "marketing provider account holding is invalid".into(),
                ));
            }
            let channel_routes = decode_marketing_provider_channel_routes(&row)?;
            Ok(MarketingProviderAccountBinding {
                provider_account_ref: row.try_get("provider_account_ref")?,
                provider_key: row.try_get("provider_key")?,
                provider_priority: positive_provider_priority(row.try_get("provider_priority")?)?,
                holding_key: holding_id.to_string(),
                data_profile_key: row.try_get("data_profile_key")?,
                holding_id,
                verifier_key_ref: row.try_get("verifier_key_ref")?,
                outbound_credential_ref: row.try_get("outbound_credential_ref")?,
                channels: row.try_get("channels")?,
                channel_routes,
                version: positive_u64(
                    row.try_get("version")?,
                    "marketing provider account version",
                )?,
            })
        })
        .transpose()
    }
}
