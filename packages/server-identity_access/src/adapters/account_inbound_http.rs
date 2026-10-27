//! HTTP owner adapter; reuse the one canonical Account parser. Parsed cookie
//! evidence is not an authenticated session or an authorized Work context.
use crate::{
    adapters::account_http::{one_header, presented},
    domain::work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
    public::{AccountPolicy, DeviceProof, RequestAudience, SystemAuthenticationSecrets},
};
use axum::http::HeaderMap;
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, FailureKind};

pub struct ParsedAccountRequest {
    host: String,
    audience: RequestAudience,
    origin: String,
    proof: DeviceProof,
}
impl ParsedAccountRequest {
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn audience(&self) -> &RequestAudience {
        &self.audience
    }
    pub fn expected_origin(&self) -> &str {
        &self.origin
    }
    pub fn into_proof(self) -> DeviceProof {
        self.proof
    }
}
pub fn unique_request_header<'a>(
    headers: &'a HeaderMap,
    name: &'static str,
) -> Result<Option<&'a str>, BusinessError> {
    one_header(headers, name)
}
pub fn parse_inbound_account_request(
    policy: &AccountPolicy,
    headers: &HeaderMap,
) -> Result<ParsedAccountRequest, BusinessError> {
    policy.validate().map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_AUTHORITY_UNAVAILABLE",
            "The Account authority is unavailable.",
        )
    })?;
    let host = one_header(headers, "host")?.ok_or_else(not_found)?;
    let audience = policy.audience(host).ok_or_else(not_found)?;
    let origin = match &audience {
        RequestAudience::Account => policy.account_origin.clone(),
        RequestAudience::Holding { audience } => policy
            .holdings
            .iter()
            .find(|h| &h.audience == audience)
            .ok_or_else(not_found)?
            .origin
            .clone(),
    };
    // This function owns the existing 8192-byte aggregate Cookie budget, duplicate
    // rejection across every Cookie header and exact audience cookie selection.
    let proof = presented(headers, &audience)?.proof(&SystemAuthenticationSecrets);
    Ok(ParsedAccountRequest {
        host: host.into(),
        audience,
        origin,
        proof,
    })
}
const CONTEXT: [&str; 4] = [
    "x-erpx-context-company",
    "x-erpx-context-revision",
    "x-erpx-context-assignment",
    "x-erpx-context-assignment-version",
];
/// An allowlisted transport envelope. Resolve only AFTER the actual Control
/// session proof is retained; invalid CAS must not outrank UNAUTHENTICATED.
pub struct CapturedHoldingMutation {
    invalid: bool,
    headers: HeaderMap,
    branch_slug: Option<String>,
    department_slug: Option<String>,
}
impl CapturedHoldingMutation {
    pub fn capture(
        headers: &HeaderMap,
        branch_slug: Option<String>,
        department_slug: Option<String>,
    ) -> Self {
        let mut selected = HeaderMap::new();
        let mut invalid = false;
        for name in CONTEXT {
            let mut values = headers.get_all(name).iter();
            if let Some(value) = values.next() {
                if value.len() > 256 {
                    invalid = true
                } else {
                    selected.append(name, value.clone());
                }
            }
            if values.next().is_some() {
                invalid = true
            }
        }
        Self {
            invalid,
            headers: selected,
            branch_slug,
            department_slug,
        }
    }
    /// Read selectors may be omitted for the first SSR read. If any CAS header
    /// is supplied, require the complete same captured selection; never combine
    /// half of a rendered snapshot with a later freshly selected account.
    pub fn resolve_read(
        self,
        company: CompanyId,
    ) -> Result<HoldingWorkspaceRequest, BusinessError> {
        if self.invalid {
            return Err(changed());
        }
        if self.headers.is_empty() {
            return Ok(HoldingWorkspaceRequest {
                company_id: company,
                branch_slug: self.branch_slug,
                department_slug: self.department_slug,
                expected: None,
                mutation: false,
            });
        }
        let mut request =
            holding_mutation_request(&self.headers, self.branch_slug, self.department_slug)?;
        if request.company_id != company {
            return Err(changed());
        }
        request.mutation = false;
        Ok(request)
    }
    pub fn resolve(self) -> Result<HoldingWorkspaceRequest, BusinessError> {
        if self.invalid {
            return Err(changed());
        }
        holding_mutation_request(&self.headers, self.branch_slug, self.department_slug)
    }
}
pub fn holding_mutation_request(
    headers: &HeaderMap,
    branch_slug: Option<String>,
    department_slug: Option<String>,
) -> Result<HoldingWorkspaceRequest, BusinessError> {
    let get = |name| {
        one_header(headers, name)
            .map_err(|_| changed())?
            .ok_or_else(changed)
    };
    let whole = |name, minimum| {
        let raw = get(name)?;
        let value = raw.parse::<i64>().map_err(|_| changed())?;
        if !(minimum..=9_007_199_254_740_991).contains(&value) || value.to_string() != raw {
            return Err(changed());
        }
        Ok(value)
    };
    let company_id = CompanyId::new(get(CONTEXT[0])?).map_err(|_| changed())?;
    let expected = ExpectedWorkSelection {
        revision: whole(CONTEXT[1], 0)?,
        assignment_id: AssignmentId::new(get(CONTEXT[2])?).map_err(|_| changed())?,
        assignment_version: whole(CONTEXT[3], 1)?,
    };
    Ok(HoldingWorkspaceRequest {
        company_id,
        branch_slug,
        department_slug,
        expected: Some(expected),
        mutation: true,
    })
}
fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your work role changed. Reload and review before submitting.",
    )
}
