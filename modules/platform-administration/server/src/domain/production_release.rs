use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReleaseCriterion {
    pub id: String,
    pub permission: String,
}
pub struct ProductionReleaseCatalogue {
    pub(crate) scope_version: String,
    pub(crate) criteria: Vec<ProductionReleaseCriterion>,
}
impl ProductionReleaseCatalogue {
    pub fn scope_version(&self) -> &str {
        &self.scope_version
    }
    pub fn criteria(&self) -> &[ProductionReleaseCriterion] {
        &self.criteria
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionAcceptance {
    pub id: String,
    pub software_version: String,
    pub configuration_revision: i64,
    pub setup_version: i64,
    pub requested_by: String,
    pub revoked_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionGateFact {
    pub criterion_id: String,
    pub state: String,
    pub permission: String,
    pub reviewer_current: bool,
    pub reviewer_revision: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReleaseFacts {
    pub latest: Option<ProductionAcceptance>,
    pub configuration_revision: Option<i64>,
    pub setup_status: Option<String>,
    pub setup_version: Option<i64>,
    pub gates: Vec<ProductionGateFact>,
}
/// Runtime acceptance for the fixed production scope. The material Company
/// configuration (branch count, currency, setup, module settings) is captured
/// by `company_release_revisions`; equality with the accepted revision proves
/// no triggering configuration row changed since submission, so the predicate
/// does not re-read those owner tables. Setup acceptance and every independent
/// gate reviewer are still rechecked in the same serializable snapshot.
pub fn accepted_production_release(
    catalogue: &ProductionReleaseCatalogue,
    artifact: &str,
    facts: &ProductionReleaseFacts,
) -> bool {
    let Some(release) = &facts.latest else {
        return false;
    };
    artifact != "unbuilt"
        && !artifact.is_empty()
        && release.revoked_at.is_none()
        && release.software_version == artifact
        && facts.configuration_revision == Some(release.configuration_revision)
        && facts.setup_status.as_deref() == Some("accepted")
        && facts.setup_version == Some(release.setup_version)
        && catalogue.criteria.iter().all(|criterion| {
            facts.gates.iter().any(|gate| {
                gate.criterion_id == criterion.id
                    && gate.permission == criterion.permission
                    && gate.state == "passed"
                    && gate.reviewer_current
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> ProductionReleaseCatalogue {
        ProductionReleaseCatalogue {
            scope_version: "scope".to_owned(),
            criteria: vec![
                ProductionReleaseCriterion {
                    id: "criterion-a".to_owned(),
                    permission: "finance.approve".to_owned(),
                },
                ProductionReleaseCriterion {
                    id: "criterion-b".to_owned(),
                    permission: "organization.manage".to_owned(),
                },
            ],
        }
    }

    fn passing_facts() -> ProductionReleaseFacts {
        ProductionReleaseFacts {
            latest: Some(ProductionAcceptance {
                id: "rel-1".to_owned(),
                software_version: "v1.0.0".to_owned(),
                configuration_revision: 7,
                setup_version: 3,
                requested_by: "alice".to_owned(),
                revoked_at: None,
            }),
            configuration_revision: Some(7),
            setup_status: Some("accepted".to_owned()),
            setup_version: Some(3),
            gates: vec![
                ProductionGateFact {
                    criterion_id: "criterion-a".to_owned(),
                    state: "passed".to_owned(),
                    permission: "finance.approve".to_owned(),
                    reviewer_current: true,
                    reviewer_revision: "rev-1".to_owned(),
                },
                ProductionGateFact {
                    criterion_id: "criterion-b".to_owned(),
                    state: "passed".to_owned(),
                    permission: "organization.manage".to_owned(),
                    reviewer_current: true,
                    reviewer_revision: "rev-2".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn unbuilt_artifact_never_passes() {
        let catalogue = catalogue();
        let facts = passing_facts();
        assert!(!accepted_production_release(&catalogue, "unbuilt", &facts));
    }

    #[test]
    fn complete_acceptance_passes() {
        let catalogue = catalogue();
        let facts = passing_facts();
        assert!(accepted_production_release(&catalogue, "v1.0.0", &facts));
    }

    #[test]
    fn revoked_release_never_passes() {
        let catalogue = catalogue();
        let mut facts = passing_facts();
        facts.latest.as_mut().unwrap().revoked_at = Some(42);
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
    }

    #[test]
    fn missing_gate_never_passes() {
        let catalogue = catalogue();
        let mut facts = passing_facts();
        facts.gates.pop();
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
        facts.gates.clear();
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
    }

    #[test]
    fn stale_reviewer_never_passes() {
        let catalogue = catalogue();
        let mut facts = passing_facts();
        facts.gates[0].reviewer_current = false;
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
    }

    #[test]
    fn changed_configuration_revision_never_passes() {
        let catalogue = catalogue();
        let mut facts = passing_facts();
        facts.configuration_revision = Some(8);
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
        facts = passing_facts();
        facts.setup_status = Some("pending".to_owned());
        assert!(!accepted_production_release(&catalogue, "v1.0.0", &facts));
    }
}
