//! Native scoped Service workspace; owner references remain owner supplied.
use crate::field_service_reservation_read::FieldServiceRenderedContext;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceWorkspaceScope {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceWorkspace {
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub agreements: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub orders: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub customers: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub assets: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub tickets: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub people: Vec<serde_json::Value>,
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, string | number | null>>"),schema(value_type=Vec<Object>))]
    pub nodes: Vec<serde_json::Value>,
    pub reference_scope: FieldServiceWorkspaceScope,
    pub workspace_key: String,
    pub can_write: bool,
    pub can_create_agreement: bool,
    pub can_create_order: bool,
    pub can_use_parts: bool,
    pub can_use_asset_references: bool,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FieldServiceWorkspaceResponse {
    pub data: FieldServiceWorkspace,
    pub context: FieldServiceRenderedContext,
}

use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest,
};
use erpx_server_field_service::public::workspace::FieldWorkspaceRow;
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
type Error = PgTransactionError<BusinessError>;
pub(crate) fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_WORKSPACE_LIMIT",
        "The Service workspace is unavailable or exceeds this bounded read.",
    )
}
fn string<'a>(row: &'a FieldWorkspaceRow, key: &str) -> Result<&'a str, BusinessError> {
    row.get(key).and_then(|v| v.as_str()).ok_or_else(invalid)
}
fn optional<'a>(row: &'a FieldWorkspaceRow, key: &str) -> Result<Option<&'a str>, BusinessError> {
    match row.get(key) {
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        _ => Err(invalid()),
    }
}
pub(crate) fn safe_json(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(n) => n
            .as_i64()
            .is_some_and(|x| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&x)),
        serde_json::Value::Array(a) => a.iter().all(safe_json),
        serde_json::Value::Object(o) => o.values().all(safe_json),
        _ => true,
    }
}
fn json_row(row: FieldWorkspaceRow) -> serde_json::Value {
    serde_json::Value::Object(row.into_iter().collect())
}
fn put(row: &mut FieldWorkspaceRow, key: &str, value: impl Into<serde_json::Value>) {
    row.insert(key.into(), value.into());
}
#[derive(Clone)]
pub(crate) struct WorkspaceSources;
impl HoldingWorkSourcePreparation for WorkspaceSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_workspace::lock_service_workspace_customers(
                c,
            )
            .await?;
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            erpx_server_support::public::field_service_workspace::lock_service_workspace_tickets(c)
                .await?;
            erpx_server_assets::public::field_service_workspace::lock_service_workspace_assets(c)
                .await?;
            erpx_server_organization::public::field_service_workspace::lock_service_workspace_locations(c).await?;
            erpx_server_people_payroll::public::field_service_workspace::lock_service_workspace_people(c).await?;
            erpx_server_identity_access::public::field_service_workspace::lock_service_workspace_authority(c).await?;
            erpx_server_field_service::public::workspace::lock_workspace_sources(c).await
        })
    }
}
pub(crate) struct Rights {
    agreement_new: bool,
    agreement_settle: bool,
    order_new: bool,
    order_settle: bool,
    parts_new: bool,
    pub(crate) parts_settle: bool,
    pub(crate) parts_history: bool,
    ticket_new: bool,
    witnesses: Vec<erpx_server_commercial::public::purchases::PurchaseWitness>,
}
async fn rights(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    now: erpx_server_shared::public::TimestampMillis,
) -> Result<Rights, Error> {
    let mut values = Vec::new();
    let mut witnesses = Vec::new();
    let mut reader = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue);
    for (cap, purpose) in [
        ("service.orders", CapabilityPurpose::History),
        ("service.agreements", CapabilityPurpose::NewWork),
        ("service.agreements", CapabilityPurpose::SettleExisting),
        ("service.orders", CapabilityPurpose::NewWork),
        ("service.orders", CapabilityPurpose::SettleExisting),
        ("service.parts-assets", CapabilityPurpose::NewWork),
        ("service.parts-assets", CapabilityPurpose::SettleExisting),
        ("service.parts-assets", CapabilityPurpose::History),
        ("support.source-ticket", CapabilityPurpose::NewWork),
    ] {
        let r = reader
            .inspect(
                &PurchaseRequest {
                    capabilities: vec![cap.into()],
                    purpose,
                    mode: PackageEnforcementMode::Enforced,
                },
                now,
            )
            .await?;
        values.push(r.require_allowed().is_ok());
        witnesses.push(r.witness);
    }
    if !values[0] {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SERVICE_HISTORY_REQUIRED",
            "The Company purchase does not permit Service history.",
        )
        .into());
    }
    Ok(Rights {
        agreement_new: values[1],
        agreement_settle: values[2],
        order_new: values[3],
        order_settle: values[4],
        parts_new: values[5],
        parts_settle: values[6],
        parts_history: values[7],
        ticket_new: values[8],
        witnesses,
    })
}
pub(crate) async fn prepare_workspace<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &Arc<CommercialCatalogue>,
    clock: &C,
) -> Result<PreparedWorkspace, Error> {
    use erpx_server_identity_access::public::field_service_workspace::{
        ServiceWorkspaceAssignmentQuery as Query, ServiceWorkspaceAuthorityRequest,
        read_service_workspace_authority,
    };
    let rights = rights(c, holding, company, catalogue, clock.now()).await?;
    let scope = current.selection().workspace();
    let branch = scope.branch_id.as_ref().map(|v| v.as_str());
    let department = scope.department_id.as_ref().map(|v| v.as_str());
    // Only the retained authorized Field and Support witnesses below supply foreign IDs.
    // Their owner reads are data, never authority; all witnesses are rechecked in this same unit.
    let field = erpx_server_field_service::public::workspace::read_workspace_rows(
        c, company, branch, department,
    )
    .await?;
    let mut ticket_ids = BTreeSet::new();
    let mut customer_ids = BTreeSet::new();
    let mut asset_ids = BTreeSet::new();
    let mut users = BTreeSet::new();
    let mut same_ids = BTreeSet::new();
    users.insert(current.user_id().as_str().to_owned());
    for g in &field.data().agreements {
        customer_ids.insert(string(g, "customer_id")?.to_owned());
        if let Some(id) = optional(g, "asset_id")? {
            asset_ids.insert(id.to_owned());
        }
        same_ids.insert(string(g, "created_by")?.to_owned());
    }
    for o in &field.data().orders {
        ticket_ids.insert(string(o, "ticket_id")?.to_owned());
        if let Some(id) = optional(o, "asset_id")? {
            asset_ids.insert(id.to_owned());
        }
        let tech = string(o, "technician_id")?.to_owned();
        users.insert(tech.clone());
        same_ids.insert(tech);
    }
    for e in &field.data().completions {
        same_ids.insert(e.actor_id.clone());
    }
    let grant = current.selection().assignment();
    let assignment_kind = match grant.scope_kind {
        WorkScopeKind::Holding => "holding",
        WorkScopeKind::Branch => "branch",
        WorkScopeKind::Department => "department",
    };
    let support =
        erpx_server_support::public::field_service_workspace::read_service_workspace_tickets(
            c,
            erpx_server_support::public::field_service_workspace::ServiceWorkspaceTicketScope {
                company_id: company.clone(),
                branch: branch.map(str::to_owned),
                department: department.map(str::to_owned),
                assignment_kind: assignment_kind.into(),
                assignment_node: grant.scope_node_id.as_ref().map(|v| v.as_str().to_owned()),
                include_picker: rights.order_new && rights.ticket_new,
                ids: ticket_ids.into_iter().collect(),
            },
        )
        .await?;
    for t in &support.data().display {
        customer_ids.insert(t.customer_id.clone());
    }
    let customers =
        erpx_server_finance::public::field_service_workspace::read_service_workspace_customers(
            c,
            company,
            rights.agreement_new,
            &customer_ids.into_iter().collect::<Vec<_>>(),
        )
        .await?;
    let assets =
        erpx_server_assets::public::field_service_workspace::read_service_workspace_assets(
            c,
            company,
            branch,
            department,
            rights.parts_new,
            &asset_ids.into_iter().collect::<Vec<_>>(),
        )
        .await?;
    let locations=erpx_server_organization::public::field_service_workspace::read_service_workspace_locations(c,company,branch,department,rights.agreement_new||rights.order_settle).await?;
    let people =
        erpx_server_people_payroll::public::field_service_workspace::read_service_workspace_people(
            c,
            company,
            users.into_iter().collect(),
            rights.order_settle,
        )
        .await?;
    let actor = current.user_id().as_str();
    let mut pairs = BTreeSet::new();
    for g in &field.data().agreements {
        pairs.insert(Query {
            user_id: actor.into(),
            branch_id: string(g, "branch_id")?.into(),
            department_id: optional(g, "department_id")?.map(str::to_owned),
        });
    }
    for o in &field.data().orders {
        let b = string(o, "branch_id")?.to_owned();
        let d = optional(o, "department_id")?.map(str::to_owned);
        for user in [actor, string(o, "technician_id")?] {
            pairs.insert(Query {
                user_id: user.into(),
                branch_id: b.clone(),
                department_id: d.clone(),
            });
        }
    }
    if rights.order_settle {
        for p in people.data().iter().filter(|v| v.eligible) {
            for loc in locations.data() {
                let dept = if loc.kind == "department" {
                    Some(loc.id.clone())
                } else {
                    None
                };
                if department.is_some_and(|v| dept.as_deref() != Some(v)) {
                    continue;
                }
                pairs.insert(Query {
                    user_id: p.user_id.clone(),
                    branch_id: loc.branch_id.clone(),
                    department_id: dept,
                });
                if pairs.len() > 10000 {
                    return Err(invalid().into());
                }
            }
        }
    }
    let authority = read_service_workspace_authority(
        c,
        ServiceWorkspaceAuthorityRequest {
            company: company.clone(),
            actor: actor.into(),
            assignments: pairs.into_iter().collect(),
            same_people: same_ids.into_iter().collect(),
        },
        clock.now(),
    )
    .await?;
    let eligible = |user: &str| {
        people
            .data()
            .iter()
            .any(|p| p.user_id == user && p.eligible)
    };
    let assigned = |user: &str, b: &str, d: Option<&str>, review: bool| {
        authority
            .data()
            .assignments
            .iter()
            .find(|a| a.user_id == user && a.branch_id == b && a.department_id.as_deref() == d)
            .is_some_and(|a| if review { a.review } else { a.write })
            && eligible(user)
    };
    let same_db = |user: &str| {
        authority
            .data()
            .same_people
            .iter()
            .find(|x| x.user_id == user)
            .is_some_and(|x| x.same_person)
    };
    let same_actor = |user: &str| {
        user == actor
            || current
                .command_iam_metadata()
                .person_user_ids
                .iter()
                .any(|v| v.as_str() == user)
    };
    let customer_names: BTreeMap<_, _> = customers
        .data()
        .display
        .iter()
        .map(|x| (x.id.as_str(), x.name.as_str()))
        .collect();
    let asset_codes: BTreeMap<_, _> = assets
        .data()
        .display
        .iter()
        .map(|x| (x.id.as_str(), x.code.as_str()))
        .collect();
    let ticket_map: BTreeMap<_, _> = support
        .data()
        .display
        .iter()
        .map(|x| (x.id.as_str(), x))
        .collect();
    let today: String = sqlx::query_scalar(
        "SELECT to_char(to_timestamp($1::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')",
    )
    .bind(clock.now().get())
    .fetch_one(&mut *c)
    .await?;
    let can_write = current.has_permission("records.write");
    let can_review = current.has_permission("records.review");
    let policy = erpx_server_field_service::public::workspace::WorkspaceRights {
        can_write,
        can_review,
        agreement_new: rights.agreement_new,
        agreement_settle: rights.agreement_settle,
        order_new: rights.order_new,
        order_settle: rights.order_settle,
        parts_new: rights.parts_new,
        parts_settle: rights.parts_settle,
    };
    let mut agreements = Vec::new();
    let mut titles = BTreeMap::new();
    for item in &field.data().linked_agreements {
        titles.insert(item.id.clone(), item.title.clone());
    }
    for row in &field.data().agreements {
        let mut g = row.clone();
        let customer = string(row, "customer_id")?;
        let customer_name = customer_names.get(customer).ok_or_else(invalid)?;
        let b = string(row, "branch_id")?;
        let d = optional(row, "department_id")?;
        let asset = optional(row, "asset_id")?;
        let code = asset
            .map(|id| asset_codes.get(id).copied().ok_or_else(invalid))
            .transpose()?;
        let flags = erpx_server_field_service::public::workspace::agreement_workspace_flags(
            policy,
            erpx_server_field_service::public::workspace::AgreementWorkspaceFacts {
                reviewer_eligible: assigned(actor, b, d, true),
                same_person_db: same_db(string(row, "created_by")?),
                same_person_actor: same_actor(string(row, "created_by")?),
                has_asset: asset.is_some(),
                status: string(row, "status")?,
                start_date: string(row, "start_date")?,
                end_date: string(row, "end_date")?,
                today: &today,
            },
        );
        put(&mut g, "customer_name", *customer_name);
        put(
            &mut g,
            "asset_code",
            if rights.parts_history { code } else { None },
        );
        put(
            &mut g,
            "asset_id",
            if rights.parts_history { asset } else { None },
        );
        put(
            &mut g,
            "requires_asset_capability",
            i64::from(asset.is_some()),
        );
        put(&mut g, "can_review", flags.can_review);
        put(&mut g, "can_activate", flags.can_activate);
        put(&mut g, "can_reject", flags.can_reject);
        put(&mut g, "can_retire", flags.can_retire);
        put(&mut g, "can_request", flags.can_request);
        agreements.push(json_row(g));
    }
    let mut orders = Vec::new();
    for row in &field.data().orders {
        let mut o = row.clone();
        let id = string(row, "id")?;
        let ticket = ticket_map
            .get(string(row, "ticket_id")?)
            .ok_or_else(invalid)?;
        let customer = customer_names
            .get(ticket.customer_id.as_str())
            .ok_or_else(invalid)?;
        let agreement = titles
            .get(string(row, "agreement_id")?)
            .ok_or_else(invalid)?;
        let b = string(row, "branch_id")?;
        let d = optional(row, "department_id")?;
        let asset = optional(row, "asset_id")?;
        let code = asset
            .map(|id| asset_codes.get(id).copied().ok_or_else(invalid))
            .transpose()?;
        let tech = string(row, "technician_id")?;
        let tech_eligible = assigned(tech, b, d, false);
        let reviewer = assigned(actor, b, d, true);
        let independent = !same_db(tech)
            && field
                .data()
                .completions
                .iter()
                .filter(|e| e.order_id == id)
                .all(|e| !same_db(&e.actor_id));
        let latest = field
            .data()
            .completions
            .iter()
            .filter(|e| e.order_id == id)
            .max_by_key(|e| e.version)
            .map(|e| e.actor_id.as_str());
        let name = people
            .data()
            .iter()
            .find(|p| p.user_id == tech)
            .map(|p| p.name.as_str());
        let flags = erpx_server_field_service::public::workspace::order_workspace_flags(
            policy,
            erpx_server_field_service::public::workspace::OrderWorkspaceFacts {
                actor_is_technician: tech == actor,
                technician_eligible: tech_eligible,
                reviewer_eligible: reviewer,
                independent_billing: independent,
                actor_same_person_as_technician: same_actor(tech),
                has_asset: asset.is_some(),
            },
        );
        put(&mut o, "ticket_title", ticket.title.as_str());
        put(&mut o, "customer_name", *customer);
        put(&mut o, "agreement_title", agreement.as_str());
        put(
            &mut o,
            "asset_code",
            if rights.parts_history { code } else { None },
        );
        put(
            &mut o,
            "asset_id",
            if rights.parts_history { asset } else { None },
        );
        put(
            &mut o,
            "requires_asset_capability",
            i64::from(asset.is_some()),
        );
        put(&mut o, "technician_name", name);
        put(&mut o, "technician_eligible", i64::from(tech_eligible));
        put(&mut o, "can_review", i64::from(reviewer));
        put(&mut o, "independent_billing", i64::from(independent));
        put(&mut o, "completed_by", latest);
        put(&mut o, "can_work", flags.can_work);
        put(&mut o, "can_release_billing", flags.can_release_billing);
        orders.push(json_row(o));
    }
    let customer_rows = customers
        .data()
        .picker
        .iter()
        .map(|x| serde_json::json!({"id":x.id,"name":x.name,"currency":x.currency}))
        .collect();
    let asset_rows=assets.data().picker.iter().map(|x|serde_json::json!({"id":x.id,"code":x.code,"name":x.name,"version":x.version,"branch_id":x.branch_id,"department_id":x.department_id})).collect();
    let ticket_rows=support.data().picker.iter().map(|x|serde_json::json!({"id":x.id,"title":x.title,"version":x.version,"customer_id":x.customer_id,"branch_id":x.branch_id,"department_id":x.department_id})).collect();
    let mut person_rows = Vec::new();
    if rights.order_settle {
        for p in people.data().iter().filter(|x| x.eligible) {
            for loc in locations.data() {
                let d = if loc.kind == "department" {
                    Some(loc.id.as_str())
                } else {
                    None
                };
                if department.is_some_and(|v| d != Some(v)) {
                    continue;
                }
                if assigned(&p.user_id, &loc.branch_id, d, false) {
                    person_rows.push(serde_json::json!({"id":p.user_id,"name":p.name,"branch_id":loc.branch_id,"department_id":d}));
                }
            }
        }
        person_rows.sort_by_key(|p| {
            (
                p["name"].as_str().unwrap_or("").to_owned(),
                p["id"].as_str().unwrap_or("").to_owned(),
                p["branch_id"].as_str().unwrap_or("").to_owned(),
                p["department_id"].as_str().map(str::to_owned),
            )
        });
        person_rows.dedup();
    }
    let nodes = if rights.agreement_new {
        locations.data().iter().map(|x|serde_json::json!({"id":x.id,"kind":x.kind,"display_name":x.display_name,"branch_id":x.branch_id})).collect()
    } else {
        Vec::new()
    };
    let key = serde_json::to_string(&(
        holding.as_str(),
        company.as_str(),
        branch,
        department,
        actor,
        grant.id.as_str(),
        grant.role.as_str(),
        current.selection().revision(),
    ))
    .map_err(|_| invalid())?;
    let flags = erpx_server_field_service::public::workspace::workspace_flags(policy);
    let data = FieldServiceWorkspace {
        agreements,
        orders,
        customers: customer_rows,
        assets: asset_rows,
        tickets: ticket_rows,
        people: person_rows,
        nodes,
        reference_scope: FieldServiceWorkspaceScope {
            branch_id: branch.map(str::to_owned),
            department_id: department.map(str::to_owned),
        },
        workspace_key: key,
        can_write: flags.can_write,
        can_create_agreement: flags.can_create_agreement,
        can_create_order: flags.can_create_order,
        can_use_parts: flags.can_use_parts,
        can_use_asset_references: flags.can_use_asset_references,
    };
    let reply = FieldServiceWorkspaceResponse {
        data,
        context: crate::field_service_reservation_read::context(current)?,
    };
    Ok(PreparedWorkspace {
        reply,
        rights,
        field,
        support,
        customers,
        assets,
        locations,
        people,
        authority,
        today,
    })
}
pub(crate) struct PreparedWorkspace {
 pub(crate) reply:FieldServiceWorkspaceResponse,pub(crate) rights:Rights,
 field:erpx_server_field_service::public::workspace::CurrentFieldWorkspaceRows,
 support:erpx_server_support::public::field_service_workspace::CurrentServiceWorkspaceTickets,
 customers:erpx_server_finance::public::field_service_workspace::CurrentServiceWorkspaceCustomers,
 assets:erpx_server_assets::public::field_service_workspace::CurrentServiceWorkspaceAssets,
 locations:erpx_server_organization::public::field_service_workspace::CurrentServiceWorkspaceLocations,
 people:erpx_server_people_payroll::public::field_service_workspace::CurrentServiceWorkspacePeople,
 authority:erpx_server_identity_access::public::field_service_workspace::CurrentServiceWorkspaceAuthority,
 today:String,
}
impl PreparedWorkspace {
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        holding: &HoldingId,
        company: &CompanyId,
        catalogue: &CommercialCatalogue,
        clock: &C,
    ) -> Result<(), Error> {
        self.field.require_current(c).await?;
        self.support.require_current(c).await?;
        self.customers.require_current(c).await?;
        self.assets.require_current(c).await?;
        self.locations.require_current(c).await?;
        self.people.require_current(c).await?;
        self.authority.require_current(c, clock.now()).await?;
        for witness in &self.rights.witnesses {
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue)
                .revalidate(witness, clock.now())
                .await?;
        }
        let now = clock.now();
        let today:String=sqlx::query_scalar("SELECT to_char(to_timestamp($1::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(now.get()).fetch_one(&mut *c).await?;
        if now.get() >= current.expires_at() || today != self.today {
            return Err(invalid().into());
        }
        Ok(())
    }
}
pub(crate) async fn build_workspace<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &Arc<CommercialCatalogue>,
    clock: &C,
) -> Result<FieldServiceWorkspaceResponse, Error> {
    let prepared = prepare_workspace(c, current, holding, company, catalogue, clock).await?;
    let serialized = serde_json::to_value(&prepared.reply).map_err(|_| invalid())?;
    if !safe_json(&serialized) {
        return Err(invalid().into());
    }
    serde_json::to_writer(
        crate::field_service_reservation_read::ReplyBudget(0),
        &serialized,
    )
    .map_err(|_| invalid())?;
    prepared
        .require_current(c, current, holding, company, catalogue, clock)
        .await?;
    Ok(prepared.reply)
}
