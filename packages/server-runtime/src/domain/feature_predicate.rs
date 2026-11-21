//! Pure renderer for the cross-owner scheduled-feature predicate.
//!
//! The Executive generation lease claim is gated by the same release/rollout
//! and engineering-verification facts as the deployment. Those facts are
//! supplied through [`FeaturePredicatePort`] by composition; this module is the
//! owner-local, IO-free renderer that turns them into SQL for the typed command
//! kinds. It never hard-codes a dev-only `TRUE`: production always renders the
//! real rollout/engineering predicate, and an unconfigured production policy
//! fails closed.
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Deployment-supplied cross-owner predicate facts. The SQL fragments are the
/// authoritative rollout/acceptance expressions from the release policy; the
/// owner crate only composes them into the lease-claim statement.
#[derive(Clone, Debug)]
pub struct FeaturePredicateConfig {
    pub production: bool,
    /// Active rollout expression over alias `f` (company_feature_rollouts).
    pub rollout_active_sql: String,
    /// Engineering verification expression over `engineering_subject.company_id`,
    /// present only for modules admitted to the engineering cohort.
    pub engineering_sql: Option<String>,
    pub engineering_modules: Vec<String>,
    pub release_modules: Vec<String>,
}

/// Cross-owner seam implemented by composition. The owner crate never reads
/// another owner's tables or env directly.
pub trait FeaturePredicatePort: Send + Sync {
    fn policy(&self) -> &FeaturePredicateConfig;
    fn production(&self) -> bool {
        self.policy().production
    }
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "RUNTIME_FEATURE_PREDICATE_INVALID",
        "The scheduled feature predicate is not configured.",
    )
}

fn rollout_exists(
    config: &FeaturePredicateConfig,
    company_expression: &str,
    module: &str,
) -> String {
    format!(
        "EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id={company_expression} \
         AND f.module_id='{module}' AND {})",
        config.rollout_active_sql
    )
}

/// Render the predicate used by the Executive generation lease claim for one
/// company expression and module. Mirrors `scheduledFeaturePredicate` exactly:
/// dev admits all; production requires a real rollout acceptance or a sealed
/// engineering verification context, and an unconfigured rollout expression is
/// rejected instead of silently admitting.
pub fn scheduled_predicate(
    config: &FeaturePredicateConfig,
    company_expression: &str,
    module: &str,
) -> Result<String, BusinessError> {
    if !config.production {
        return Ok("TRUE".to_owned());
    }
    if config.rollout_active_sql.trim().is_empty() {
        return Err(invalid());
    }
    let engineering = if config.engineering_modules.iter().any(|m| m == module) {
        config
            .engineering_sql
            .clone()
            .unwrap_or_else(|| "FALSE".to_owned())
    } else {
        "FALSE".to_owned()
    };
    if engineering == "FALSE" {
        return Ok(if config.release_modules.iter().any(|m| m == module) {
            rollout_exists(config, company_expression, module)
        } else {
            "FALSE".to_owned()
        });
    }
    let accepted = if config.release_modules.iter().any(|m| m == module) {
        rollout_exists(config, "engineering_subject.company_id", module)
    } else {
        "FALSE".to_owned()
    };
    Ok(format!(
        "(SELECT ({accepted} OR {engineering}) FROM (SELECT {company_expression}::text company_id) engineering_subject)"
    ))
}

/// Render the full `UPDATE` SQL for the interrupted-job sweep (3 binds).
pub fn interruption_sql(config: &FeaturePredicateConfig) -> Result<String, BusinessError> {
    let predicate = format!(
        "({} AND {})",
        scheduled_predicate(config, "j.company_id", "operations")?,
        scheduled_predicate(config, "j.company_id", "agent")?
    );
    Ok(format!(
        "UPDATE executive_generation_jobs j SET state='failed',error='Generation was interrupted. Your saved report is unchanged; retry to start a new attempt.',updated_at=$1 \
         WHERE state='running' AND lease_until<$2 AND {predicate}"
    ))
}

/// Render the full `UPDATE ... SKIP LOCKED RETURNING` SQL for the lease claim (3 binds).
pub fn lease_claim_sql(config: &FeaturePredicateConfig) -> Result<String, BusinessError> {
    let predicate = format!(
        "({} AND {})",
        scheduled_predicate(config, "queued.company_id", "operations")?,
        scheduled_predicate(config, "queued.company_id", "agent")?
    );
    Ok(format!(
        "UPDATE executive_generation_jobs SET state='running',lease_token=$1,lease_until=$2,attempts=attempts+1,updated_at=$3 \
         WHERE id=(SELECT id FROM executive_generation_jobs queued WHERE state='queued' AND {predicate} \
         ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(production: bool) -> FeaturePredicateConfig {
        FeaturePredicateConfig {
            production,
            rollout_active_sql: "(f.rollout_state='pilot' OR f.rollout_state='available')".to_owned(),
            engineering_sql: Some("EXISTS(SELECT 1 FROM engineering_verification_contexts engineering WHERE engineering.company_id=engineering_subject.company_id AND engineering.enabled)".to_owned()),
            engineering_modules: vec!["operations".to_owned()],
            release_modules: vec!["operations".to_owned(), "agent".to_owned()],
        }
    }

    #[test]
    fn dev_admits_but_production_uses_real_predicate() {
        assert_eq!(
            scheduled_predicate(&config(false), "j.company_id", "agent").unwrap(),
            "TRUE"
        );
        let prod = scheduled_predicate(&config(true), "j.company_id", "agent").unwrap();
        assert!(prod.contains("company_feature_rollouts"));
        assert!(!prod.contains("TRUE"));
    }

    #[test]
    fn unconfigured_production_fails_closed() {
        let mut c = config(true);
        c.rollout_active_sql.clear();
        assert!(scheduled_predicate(&c, "j.company_id", "agent").is_err());
        assert!(interruption_sql(&c).is_err());
    }

    #[test]
    fn lease_claim_renders_skip_locked() {
        let sql = lease_claim_sql(&config(true)).unwrap();
        assert!(sql.contains("FOR UPDATE SKIP LOCKED LIMIT 1"));
        assert!(sql.contains("RETURNING *"));
    }
}
