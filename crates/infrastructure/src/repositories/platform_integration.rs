use super::*;

pub const MAX_PLATFORM_INTEGRATION_OBSERVATIONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformIntegrationObservation {
    pub integration_key: String,
    pub state: String,
    pub capability: String,
    pub checked_at: DateTime<Utc>,
}

impl PlatformIntegrationObservation {
    pub(super) fn validate(&self) -> Result<(), PersistenceError> {
        let key_valid = (2..=64).contains(&self.integration_key.len())
            && self
                .integration_key
                .bytes()
                .enumerate()
                .all(|(index, byte)| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit() && index > 0
                        || byte == b'_' && index > 0
                });
        let state_valid = matches!(
            self.state.as_str(),
            "ready"
                | "not_ready"
                | "configured"
                | "not_configured"
                | "healthy"
                | "degraded"
                | "unavailable"
                | "not_probed"
        );
        if !key_valid
            || !state_valid
            || self.capability.trim().is_empty()
            || self.capability.len() > 256
            || self.capability.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "platform integration observation is invalid".into(),
            ));
        }
        Ok(())
    }
}

impl SqlxControlPlane {
    /// Replace the bounded integration-health snapshot atomically and return
    /// the persisted allow-listed rows. The write-side observation is the
    /// authority for this operational surface; no provider payload is stored.
    pub async fn record_platform_integration_observations(
        &self,
        observations: &[PlatformIntegrationObservation],
        observed_by: &str,
    ) -> Result<Vec<PlatformIntegrationObservation>, PersistenceError> {
        if observations.is_empty()
            || observations.len() > MAX_PLATFORM_INTEGRATION_OBSERVATIONS
            || observed_by.trim().is_empty()
            || observed_by.len() > 255
            || observed_by.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "platform integration observation batch is invalid".into(),
            ));
        }
        let mut seen = HashSet::with_capacity(observations.len());
        for observation in observations {
            observation.validate()?;
            if !seen.insert(observation.integration_key.as_str()) {
                return Err(PersistenceError::InvalidData(
                    "platform integration observation keys must be unique".into(),
                ));
            }
        }
        let mut transaction = self.pools.write.begin().await?;
        for observation in observations {
            sqlx::query(
                "INSERT INTO erpx_control.platform_integration_observations
                    (integration_key, state, capability, checked_at, observed_by, updated_at)
                 VALUES ($1, $2, $3, $4, $5, now())
                 ON CONFLICT (integration_key) DO UPDATE
                 SET state = EXCLUDED.state,
                     capability = EXCLUDED.capability,
                     checked_at = EXCLUDED.checked_at,
                     observed_by = EXCLUDED.observed_by,
                     updated_at = now()",
            )
            .bind(&observation.integration_key)
            .bind(&observation.state)
            .bind(&observation.capability)
            .bind(observation.checked_at)
            .bind(observed_by)
            .execute(&mut *transaction)
            .await?;
        }
        let rows = sqlx::query(
            "SELECT integration_key, state, capability, checked_at
             FROM erpx_control.platform_integration_observations
             ORDER BY integration_key
             LIMIT $1",
        )
        .bind(i64::try_from(MAX_PLATFORM_INTEGRATION_OBSERVATIONS).unwrap_or(i64::MAX))
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        rows.into_iter()
            .map(|row| {
                Ok(PlatformIntegrationObservation {
                    integration_key: row.try_get("integration_key")?,
                    state: row.try_get("state")?,
                    capability: row.try_get("capability")?,
                    checked_at: row.try_get("checked_at")?,
                })
            })
            .collect()
    }
}
