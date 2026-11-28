use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalReleaseCriterion {
    pub id: String,
    pub permission: String,
}
pub struct LegalReleaseCatalogue {
    pub(crate) scope_version: String,
    pub(crate) criteria: Vec<LegalReleaseCriterion>,
}
impl LegalReleaseCatalogue {
    pub fn scope_version(&self) -> &str {
        &self.scope_version
    }
    pub fn criteria(&self) -> &[LegalReleaseCriterion] {
        &self.criteria
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalAcceptance {
    pub id: String,
    pub software_version: String,
    pub configuration_revision: i64,
    pub setup_version: i64,
    pub requested_by: String,
    pub revoked_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalGateFact {
    pub criterion_id: String,
    pub state: String,
    pub permission: String,
    pub reviewer_current: bool,
    pub reviewer_revision: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalReleaseFacts {
    pub latest: Option<LegalAcceptance>,
    pub configuration_revision: Option<i64>,
    pub setup_status: Option<String>,
    pub setup_version: Option<i64>,
    pub gates: Vec<LegalGateFact>,
}
/// Legal scope intentionally has no hotel branch-count/currency constraints.
/// Current independent review eligibility is supplied by its owning IAM port.
pub fn accepted_legal_release(
    catalogue: &LegalReleaseCatalogue,
    artifact: &str,
    facts: &LegalReleaseFacts,
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
