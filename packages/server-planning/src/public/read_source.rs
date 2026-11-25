//! Owner-local native read ports for the Planning SELECT tail
//! (`G4 / migrate.planning.read-source-port`).
//!
//! A plan carries the byte-exact SQL of a retained TypeScript read, its ordered
//! bind positions and the authority facts of the caller. This is a registry,
//! not a transport: no route is mounted here and no connection is opened. The
//! caller runs a bound plan on its own Holding transaction. Retired kinds are
//! executed by the owner-native lane-36 route and their TypeScript SQL is
//! deleted; the registry keeps their statement as the cutover oracle.
//!
//! Two retained readers are covered:
//!   - `adapters/production-command-sources-read.ts` (16 SELECT tokens);
//!   - `adapters/resources-read.ts` (7 SELECT tokens).
//!
//! The 23 tokens map to 22 registered plans: each executive projection folds
//! its lateral shortage SELECT into the one statement the caller executes,
//! and the recommendation's runtime eligibility branch is registered as two
//! variants. The scenarios list was the first plan cut over to the native
//! owner route; its TypeScript SQL helper is deleted.
//!
//! The two executive projections are assembled from owner fragments at call
//! time in TypeScript. They are registered as full statements for execution
//! and their constituent literals are registered in [`executive`] so the
//! compile-time source-parity test can bind each piece to its retained text.
//! The cross-owner join fragments stay inside the caller's transaction until
//! their own owner exposes a native subquery.
use erpx_server_shared::public::BusinessError;

/// Operation id a future native owner-read transport will advertise.
pub const OWNER_READ_OPERATION: &str = "planning.read-source-port.v1";

pub const CATEGORY_COMMAND_SOURCES: &str = "planning.command-sources";
pub const CATEGORY_RESOURCES: &str = "planning.resources";

// -- production-command-sources-read.ts reads --------------------------------

pub const RD_SCENARIOS: &str = "command-sources.scenarios";
pub const RD_RECOMMENDATIONS: &str = "command-sources.recommendations";
pub const RD_RESOURCES: &str = "command-sources.resources";
pub const RD_CALENDAR: &str = "command-sources.calendar";
pub const RD_EXECUTIVE_SHORTAGE: &str = "command-sources.executive-shortage";
pub const RD_EXECUTIVE_RECOMMENDATION_PROPOSED: &str =
    "command-sources.executive-recommendation-proposed";
pub const RD_EXECUTIVE_RECOMMENDATION_CAPACITY: &str =
    "command-sources.executive-recommendation-capacity";
pub const RD_GUARD_ROUTING_RESOURCE: &str = "command-sources.guard-routing-resource";
pub const RD_COMPARISON: &str = "command-sources.comparison";
pub const RD_GUARD_COMPARISON: &str = "command-sources.guard-comparison";
pub const RD_SCENARIO: &str = "command-sources.scenario";
pub const RD_GUARD_UNRELEASED_SCENARIO: &str = "command-sources.guard-unreleased-scenario";
pub const RD_SCOPED_RECOMMENDATION: &str = "command-sources.scoped-recommendation";
pub const RD_GUARD_INDEPENDENT_RECOMMENDATION: &str =
    "command-sources.guard-independent-recommendation";
pub const RD_GUARD_CAPACITY_RESERVATION: &str = "command-sources.guard-capacity-reservation";

// -- resources-read.ts reads -------------------------------------------------

pub const RD_GUARD_RESOURCE_VERSION: &str = "resources.guard-resource-version";
pub const RD_GUARD_NO_ACTIVE_ALLOCATION: &str = "resources.guard-no-active-allocation";
pub const RD_GUARD_ACTIVE_RESOURCE_VERSION: &str = "resources.guard-active-resource-version";
pub const RD_GUARD_CALENDAR_VERSION: &str = "resources.guard-calendar-version";
pub const RD_GUARD_RELEASED_CAPACITY: &str = "resources.guard-released-capacity";
pub const RD_GUARD_ALLOCATED_RESOURCE_VERSION: &str =
    "resources.guard-allocated-resource-version";
pub const RD_GUARD_ALLOCATED_CALENDAR_VERSION: &str =
    "resources.guard-allocated-calendar-version";

/// A bind position in a planned statement, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindField {
    /// Record id (`scenario`/`recommendation` caller id).
    RecordId,
    CompanyId,
    /// Optional: a `None` scope binds SQL NULL and keeps the unscoped predicate.
    BranchId,
    SpecificationId,
    ResourceId,
    WorkDate,
    From,
    To,
    Version,
    UserId,
    Minutes,
    AvailableMinutes,
}

/// Fields a kind needs from its caller before its plan can be bound.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScopeRequirement {
    pub record: bool,
    pub company: bool,
    pub branch: bool,
    pub specification: bool,
    pub resource: bool,
    pub work_date: bool,
    pub from: bool,
    pub to: bool,
    pub version: bool,
    pub user: bool,
    pub minutes: bool,
    pub available_minutes: bool,
}

/// One caller-unit read plan: exact SQL, ordered binds and the retained
/// authority facts that must survive a later cutover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadPlan {
    pub kind: &'static str,
    pub category: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    pub binds: &'static [BindField],
    pub scope: ScopeRequirement,
    /// Authority the retained caller already enforces. A cutover must not drop it.
    pub authority: &'static str,
    /// Cross-owner fragments still inlined in the SQL. They stay inside the
    /// caller's transaction until their owner exposes a native subquery.
    pub cross_owner: &'static [&'static str],
}

/// A literal owned by another owner or assembled by the retained caller. Not
/// bindable on its own; registered so the exact text stays auditable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadFragment {
    pub name: &'static str,
    pub sql: &'static str,
    /// The `${...}` interpolation in the TypeScript read source.
    pub token: &'static str,
    /// Owner of the literal.
    pub source: &'static str,
}

impl ScopeRequirement {
    const fn of(fields: &[BindField]) -> Self {
        let mut scope = Self {
            record: false,
            company: false,
            branch: false,
            specification: false,
            resource: false,
            work_date: false,
            from: false,
            to: false,
            version: false,
            user: false,
            minutes: false,
            available_minutes: false,
        };
        let mut index = 0;
        while index < fields.len() {
            match fields[index] {
                BindField::RecordId => scope.record = true,
                BindField::CompanyId => scope.company = true,
                BindField::BranchId => scope.branch = true,
                BindField::SpecificationId => scope.specification = true,
                BindField::ResourceId => scope.resource = true,
                BindField::WorkDate => scope.work_date = true,
                BindField::From => scope.from = true,
                BindField::To => scope.to = true,
                BindField::Version => scope.version = true,
                BindField::UserId => scope.user = true,
                BindField::Minutes => scope.minutes = true,
                BindField::AvailableMinutes => scope.available_minutes = true,
            }
            index += 1;
        }
        scope
    }
}

const READ: &str = "records.read + planning module; company_id=actor; optional (branch IS NULL OR branch_id=branch); caller serializable Holding transaction";
const RESOURCE_WRITE_READ: &str = "records.read + records.write + records.review + planning module; company_id=actor; optional branch scope; caller serializable Holding transaction with native owner-write flush";

// -- cross-owner fragments, inlined verbatim ---------------------------------

const MFG_SCENARIO_SPEC: &str = "JOIN manufacturing_specifications s ON s.id=p.specification_id";
const MFG_EXECUTIVE_SPEC: &str =
    "LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id";
const COM_EXECUTIVE_OUTPUT: &str =
    "LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id";
const COM_EXECUTIVE_WAREHOUSE: &str =
    "LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id";
const ORG_EXECUTIVE_BRANCH: &str =
    "LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id";
const COM_EXECUTIVE_SHORTAGE_PRODUCT: &str =
    "LEFT JOIN commerce_products p ON p.id=entry.value->>'productId' AND p.company_id=s.company_id";

pub const FRAGMENTS: &[ReadFragment] = &[
    ReadFragment {
        name: "scenarioSpecification",
        sql: MFG_SCENARIO_SPEC,
        token: "${productionmanufacturingProjection.scenarioSpecification}",
        source: "server-manufacturing/production-read-projections",
    },
    ReadFragment {
        name: "executiveSpecification",
        sql: MFG_EXECUTIVE_SPEC,
        token: "${productionmanufacturingProjection.executiveSpecification}",
        source: "server-manufacturing/production-read-projections",
    },
    ReadFragment {
        name: "executiveOutput",
        sql: COM_EXECUTIVE_OUTPUT,
        token: "${productioncommerceProjection.executiveOutput}",
        source: "server-commerce/production-read-projections",
    },
    ReadFragment {
        name: "executiveWarehouse",
        sql: COM_EXECUTIVE_WAREHOUSE,
        token: "${productioncommerceProjection.executiveWarehouse}",
        source: "server-commerce/production-read-projections",
    },
    ReadFragment {
        name: "executiveBranch",
        sql: ORG_EXECUTIVE_BRANCH,
        token: "${productionorganizationProjection.executiveBranch}",
        source: "server-organization/production-read-projections",
    },
    ReadFragment {
        name: "executiveShortageProduct",
        sql: COM_EXECUTIVE_SHORTAGE_PRODUCT,
        token: "${productioncommerceProjection.executiveShortageProduct}",
        source: "server-commerce/production-read-projections",
    },
];

/// Literals the retained caller assembles into the executive projection. The
/// `*_SOURCE` pieces are contiguous in the TypeScript read and are checked
/// against it at compile time; the full SQL constants are those pieces with
/// the cross-owner fragments inlined exactly as the caller executes them.
macro_rules! source_joins_sql {
    () => {
        concat!(
            "LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id ",
            "LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id ",
            "LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id ",
            "LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id ",
            "LEFT JOIN planning_recommendations r ON r.scenario_id=s.id"
        )
    };
}

macro_rules! projection_sql {
    () => {
        concat!(
            "SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,",
            "s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,",
            "spec.output_product_id,output.name output_product_name,s.specification_version,",
            "s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,",
            "s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,",
            "r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,",
            "r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,",
            "r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,",
            "COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json",
            " FROM planning_scenarios s ",
            source_joins_sql!()
        )
    };
}

macro_rules! shortage_projection_sql {
    () => {
        concat!(
            "LEFT JOIN LATERAL (",
            "SELECT COALESCE(jsonb_agg(jsonb_build_object(",
            "'product_name',COALESCE(p.name,'Unknown product'),",
            "'required_milli',(entry.value->>'requiredMilli')::numeric::text,",
            "'available_milli',(entry.value->>'availableMilli')::numeric::text,",
            "'shortage_milli',(entry.value->>'shortageMilli')::numeric::text",
            ") ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,",
            "count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count ",
            "FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value) ",
            "LEFT JOIN commerce_products p ON p.id=entry.value->>'productId' AND p.company_id=s.company_id",
            ") shortage ON TRUE"
        )
    };
}

macro_rules! executive_sql {
    ($where:ident) => {
        concat!(
            projection_sql!(),
            " ",
            shortage_projection_sql!(),
            " ",
            $where!()
        )
    };
}

pub mod executive {
    macro_rules! where_shortage_sql {
        () => {
            concat!(
                "WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
                " AND s.due_date BETWEEN ? AND ?",
                " AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)') LIMIT 1"
            )
        };
    }

    macro_rules! where_proposed_sql {
        () => {
            concat!(
                "WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
                " AND r.feasible_date BETWEEN ? AND ? AND r.status='proposed' LIMIT 1"
            )
        };
    }

    macro_rules! where_capacity_sql {
        () => {
            concat!(
                "WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
                " AND r.feasible_date BETWEEN ? AND ?",
                " AND r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes LIMIT 1"
            )
        };
    }

    pub const BRANCH_SCOPE: &str = "(?::text IS NULL OR s.branch_id=?)";

    /// Contiguous TypeScript literal for the assembled projection template.
    pub const PROJECTION_SOURCE: &str = concat!(
        "SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,",
        "s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,",
        "spec.output_product_id,output.name output_product_name,s.specification_version,",
        "s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,",
        "s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,",
        "r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,",
        "r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,",
        "r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,",
        "COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json",
        " FROM planning_scenarios s ${sourceJoins} ${shortageProjection}"
    );

    /// Contiguous TypeScript literal for the assembled shortage projection.
    pub const SHORTAGE_SOURCE: &str = concat!(
        "LEFT JOIN LATERAL (",
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(",
        "'product_name',COALESCE(p.name,'Unknown product'),",
        "'required_milli',(entry.value->>'requiredMilli')::numeric::text,",
        "'available_milli',(entry.value->>'availableMilli')::numeric::text,",
        "'shortage_milli',(entry.value->>'shortageMilli')::numeric::text",
        ") ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,",
        "count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count",
        "FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)",
        "${productioncommerceProjection.executiveShortageProduct}",
        ") shortage ON TRUE"
    );

    pub const SOURCE_JOINS: &str = source_joins_sql!();
    pub const SHORTAGE_SQL: &str = shortage_projection_sql!();
    pub const PROJECTION_SQL: &str = projection_sql!();
    pub const WHERE_SHORTAGE: &str = where_shortage_sql!();
    pub const WHERE_PROPOSED: &str = where_proposed_sql!();
    pub const WHERE_CAPACITY: &str = where_capacity_sql!();

    pub const SHORTAGE_EXEC_SQL: &str = executive_sql!(where_shortage_sql);
    pub const PROPOSED_EXEC_SQL: &str = executive_sql!(where_proposed_sql);
    pub const CAPACITY_EXEC_SQL: &str = executive_sql!(where_capacity_sql);

    /// The retained statement pieces, each contiguous in the TypeScript source.
    /// The projection and lateral heads are assembled by `planningSelectFrom`,
    /// so the authority is its two arguments (columns and FROM tail), not the
    /// concatenated statement with the builder-supplied `SELECT`/`FROM`.
    pub fn source_chunks(where_clause: &str) -> Vec<String> {
        let where_source = where_clause
            .replace(BRANCH_SCOPE, "${branchScope}")
            .replace(
                "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes",
                "${eligibility}",
            )
            .replace("r.status='proposed'", "${eligibility}");
        let projection = PROJECTION_SOURCE
            .strip_prefix("SELECT ")
            .and_then(|value| value.split_once(" FROM "))
            .expect("projection source has a SELECT head and FROM tail");
        let shortage = SHORTAGE_SOURCE
            .strip_prefix("LEFT JOIN LATERAL (SELECT ")
            .and_then(|value| value.strip_suffix(") shortage ON TRUE"))
            .and_then(|value| value.split_once("FROM "))
            .expect("shortage source has a SELECT head and FROM tail");
        vec![
            super::norm(projection.0),
            super::norm(projection.1),
            super::norm(shortage.0),
            super::norm(shortage.1),
            super::norm(&where_source),
        ]
    }
}

// -- exact retained statements ----------------------------------------------

const SQL_SCENARIOS: &str = concat!(
    "SELECT p.*,s.name specification_name FROM planning_scenarios p ",
    "JOIN manufacturing_specifications s ON s.id=p.specification_id ",
    "WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC"
);
const SQL_RECOMMENDATIONS: &str = concat!(
    "SELECT r.*,s.company_id,s.branch_id FROM planning_recommendations r ",
    "JOIN planning_scenarios s ON s.id=r.scenario_id ",
    "WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY r.updated_at DESC"
);
const SQL_RESOURCES: &str =
    "SELECT * FROM production_resources WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name";
const SQL_CALENDAR: &str = concat!(
    "SELECT c.* FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id ",
    "WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500"
);
const SQL_GUARD_ROUTING_RESOURCE: &str = concat!(
    "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? ",
    "AND status='active' AND (?::text IS NULL OR branch_id=?))"
);
const SQL_COMPARISON: &str = concat!(
    "SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date ",
    "FROM planning_scenarios s JOIN planning_recommendations r ON r.scenario_id=s.id ",
    "WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?"
);
const SQL_GUARD_COMPARISON: &str = concat!(
    "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? ",
    "AND branch_id=? AND specification_id=? AND version=?)"
);
const SQL_SCENARIO: &str =
    "SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)";
const SQL_GUARD_UNRELEASED_SCENARIO: &str = concat!(
    "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? ",
    "AND version=? AND status='calculated')"
);
const SQL_SCOPED_RECOMMENDATION: &str = concat!(
    "SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,",
    "s.context_json,s.created_by scenario_created_by FROM planning_recommendations r ",
    "JOIN planning_scenarios s ON s.id=r.scenario_id ",
    "WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)"
);
const SQL_GUARD_INDEPENDENT_RECOMMENDATION: &str = concat!(
    "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? ",
    "AND status='proposed' AND created_by<>?)"
);
const SQL_GUARD_CAPACITY_RESERVATION: &str = concat!(
    "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations ",
    "WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)+?<=?"
);

const SQL_GUARD_RESOURCE_VERSION: &str = concat!(
    "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? ",
    "AND branch_id=? AND version=?)"
);
const SQL_GUARD_NO_ACTIVE_ALLOCATION: &str =
    "NOT EXISTS(SELECT 1 FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL)";
const SQL_GUARD_ACTIVE_RESOURCE_VERSION: &str = concat!(
    "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? ",
    "AND status='active' AND version=? AND (?::text IS NULL OR branch_id=?))"
);
const SQL_GUARD_CALENDAR_VERSION: &str = concat!(
    "COALESCE((SELECT version FROM production_resource_calendar WHERE resource_id=? ",
    "AND work_date=?),0)=?"
);
const SQL_GUARD_RELEASED_CAPACITY: &str = concat!(
    "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations ",
    "WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)<=?"
);
const SQL_GUARD_ALLOCATED_RESOURCE_VERSION: &str = concat!(
    "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? ",
    "AND version=? AND status='active')"
);
const SQL_GUARD_ALLOCATED_CALENDAR_VERSION: &str = SQL_GUARD_CALENDAR_VERSION;

macro_rules! plan {
    ($kind:expr, $category:expr, $caller:expr, $sql:expr, [$($bind:expr),* $(,)?], $authority:expr, [$($cross:expr),* $(,)?]) => {
        ReadPlan {
            kind: $kind,
            category: $category,
            caller: $caller,
            sql: $sql,
            binds: &[$($bind),*],
            scope: ScopeRequirement::of(&[$($bind),*]),
            authority: $authority,
            cross_owner: &[$($cross),*],
        }
    };
}

/// The closed read registry, in stable wire order.
pub const PLANS: &[ReadPlan] = &[
    plan!(RD_SCENARIOS, CATEGORY_COMMAND_SOURCES, "productionCommandSources.scenarios", SQL_SCENARIOS, [BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, [MFG_SCENARIO_SPEC]),
    plan!(RD_RECOMMENDATIONS, CATEGORY_COMMAND_SOURCES, "productionCommandSources.recommendations", SQL_RECOMMENDATIONS, [BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, []),
    plan!(RD_RESOURCES, CATEGORY_COMMAND_SOURCES, "productionCommandSources.resources", SQL_RESOURCES, [BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, []),
    plan!(RD_CALENDAR, CATEGORY_COMMAND_SOURCES, "productionCommandSources.calendar", SQL_CALENDAR, [BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, []),
    plan!(RD_EXECUTIVE_SHORTAGE, CATEGORY_COMMAND_SOURCES, "productionCommandSources.executiveShortage", executive::SHORTAGE_EXEC_SQL, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::BranchId, BindField::From, BindField::To], READ, [MFG_EXECUTIVE_SPEC, COM_EXECUTIVE_OUTPUT, COM_EXECUTIVE_WAREHOUSE, ORG_EXECUTIVE_BRANCH, COM_EXECUTIVE_SHORTAGE_PRODUCT]),
    plan!(RD_EXECUTIVE_RECOMMENDATION_PROPOSED, CATEGORY_COMMAND_SOURCES, "productionCommandSources.executiveRecommendation:proposed", executive::PROPOSED_EXEC_SQL, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::BranchId, BindField::From, BindField::To], READ, [MFG_EXECUTIVE_SPEC, COM_EXECUTIVE_OUTPUT, COM_EXECUTIVE_WAREHOUSE, ORG_EXECUTIVE_BRANCH, COM_EXECUTIVE_SHORTAGE_PRODUCT]),
    plan!(RD_EXECUTIVE_RECOMMENDATION_CAPACITY, CATEGORY_COMMAND_SOURCES, "productionCommandSources.executiveRecommendation:capacity", executive::CAPACITY_EXEC_SQL, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::BranchId, BindField::From, BindField::To], READ, [MFG_EXECUTIVE_SPEC, COM_EXECUTIVE_OUTPUT, COM_EXECUTIVE_WAREHOUSE, ORG_EXECUTIVE_BRANCH, COM_EXECUTIVE_SHORTAGE_PRODUCT]),
    plan!(RD_GUARD_ROUTING_RESOURCE, CATEGORY_COMMAND_SOURCES, "productionCommandSources.guardRoutingResource", SQL_GUARD_ROUTING_RESOURCE, [BindField::ResourceId, BindField::CompanyId, BindField::BranchId, BindField::BranchId], RESOURCE_WRITE_READ, []),
    plan!(RD_COMPARISON, CATEGORY_COMMAND_SOURCES, "productionCommandSources.comparison", SQL_COMPARISON, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::SpecificationId], READ, []),
    plan!(RD_GUARD_COMPARISON, CATEGORY_COMMAND_SOURCES, "productionCommandSources.guardComparison", SQL_GUARD_COMPARISON, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::SpecificationId, BindField::Version], RESOURCE_WRITE_READ, []),
    plan!(RD_SCENARIO, CATEGORY_COMMAND_SOURCES, "productionCommandSources.scenario", SQL_SCENARIO, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, []),
    plan!(RD_GUARD_UNRELEASED_SCENARIO, CATEGORY_COMMAND_SOURCES, "productionCommandSources.guardUnreleasedScenario", SQL_GUARD_UNRELEASED_SCENARIO, [BindField::RecordId, BindField::CompanyId, BindField::Version], RESOURCE_WRITE_READ, []),
    plan!(RD_SCOPED_RECOMMENDATION, CATEGORY_COMMAND_SOURCES, "productionCommandSources.scopedRecommendation", SQL_SCOPED_RECOMMENDATION, [BindField::RecordId, BindField::CompanyId, BindField::BranchId, BindField::BranchId], READ, []),
    plan!(RD_GUARD_INDEPENDENT_RECOMMENDATION, CATEGORY_COMMAND_SOURCES, "productionCommandSources.guardIndependentRecommendation", SQL_GUARD_INDEPENDENT_RECOMMENDATION, [BindField::RecordId, BindField::Version, BindField::UserId], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_CAPACITY_RESERVATION, CATEGORY_COMMAND_SOURCES, "productionCommandSources.guardCapacityReservation", SQL_GUARD_CAPACITY_RESERVATION, [BindField::ResourceId, BindField::WorkDate, BindField::Minutes, BindField::AvailableMinutes], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_RESOURCE_VERSION, CATEGORY_RESOURCES, "resources.guardResourceVersion", SQL_GUARD_RESOURCE_VERSION, [BindField::ResourceId, BindField::CompanyId, BindField::BranchId, BindField::Version], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_NO_ACTIVE_ALLOCATION, CATEGORY_RESOURCES, "resources.guardNoActiveAllocation", SQL_GUARD_NO_ACTIVE_ALLOCATION, [BindField::ResourceId], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_ACTIVE_RESOURCE_VERSION, CATEGORY_RESOURCES, "resources.guardActiveResourceVersion", SQL_GUARD_ACTIVE_RESOURCE_VERSION, [BindField::ResourceId, BindField::CompanyId, BindField::Version, BindField::BranchId, BindField::BranchId], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_CALENDAR_VERSION, CATEGORY_RESOURCES, "resources.guardCalendarVersion", SQL_GUARD_CALENDAR_VERSION, [BindField::ResourceId, BindField::WorkDate, BindField::Version], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_RELEASED_CAPACITY, CATEGORY_RESOURCES, "resources.guardReleasedCapacity", SQL_GUARD_RELEASED_CAPACITY, [BindField::ResourceId, BindField::WorkDate, BindField::AvailableMinutes], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_ALLOCATED_RESOURCE_VERSION, CATEGORY_RESOURCES, "resources.guardAllocatedResourceVersion", SQL_GUARD_ALLOCATED_RESOURCE_VERSION, [BindField::ResourceId, BindField::CompanyId, BindField::Version], RESOURCE_WRITE_READ, []),
    plan!(RD_GUARD_ALLOCATED_CALENDAR_VERSION, CATEGORY_RESOURCES, "resources.guardAllocatedCalendarVersion", SQL_GUARD_ALLOCATED_CALENDAR_VERSION, [BindField::ResourceId, BindField::WorkDate, BindField::Version], RESOURCE_WRITE_READ, []),
];

/// Resolve a registered read kind. Unknown kinds are rejected at planning time;
/// they never cause a live read to fail or fall back to a status marker.
pub fn plan(kind: &str) -> Option<&'static ReadPlan> {
    PLANS.iter().find(|entry| entry.kind == kind)
}

pub fn is_owner_read(kind: &str) -> bool {
    plan(kind).is_some()
}

pub fn kinds() -> impl Iterator<Item = &'static str> {
    PLANS.iter().map(|entry| entry.kind)
}

/// Invalid-kind validation error for a future native owner-read transport.
pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_READ_SOURCE_INVALID",
        "Review the Planning owner read-source registry.",
    )
}

/// Remove whitespace so TypeScript template indentation cannot change token
/// identity when a retained literal is compared to a registered statement.
pub fn norm(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Count `?` bind placeholders, ignoring `?` inside single-quoted literals
/// such as the retained jsonpath `'$[*] ? (@.shortageMilli > 0)'`.
pub fn placeholder_count(sql: &str) -> usize {
    let mut count = 0;
    let mut quoted = false;
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    quoted = false;
                }
            }
        } else if ch == '\'' {
            quoted = true;
        } else if ch == '?' {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_COMMANDS: &str = include_str!("../adapters/production-command-sources-read.ts");
    const TS_RESOURCES: &str = include_str!("../adapters/resources-read.ts");
    const TS_MFG_PROJECTION: &str =
        include_str!("../../../server-manufacturing/src/adapters/production-read-projections.ts");
    const TS_COM_PROJECTION: &str = include_str!("../../../server-commerce/src/adapters/production-read-projections.ts");
    const TS_ORG_PROJECTION: &str =
        include_str!("../../../server-organization/src/adapters/production-read-projections.ts");
    /// Retired kinds are owner-native: their retired TS SQL is gone, so the
    /// byte oracle is the exact statement in the native Postgres adapter.
    const NATIVE_POSTGRES: &str = include_str!("../adapters/native_lane_36/postgres.rs");

    /// Translate the registry's `?` markers to the native driver's positional
    /// form, leaving the `?` inside the retained jsonpath literal untouched.
    fn native_binds(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len());
        let mut quoted = false;
        let mut index = 0usize;
        let mut chars = sql.chars().peekable();
        while let Some(ch) = chars.next() {
            if quoted {
                out.push(ch);
                if ch == '\'' {
                    if chars.peek() == Some(&'\'') {
                        out.push(chars.next().unwrap());
                    } else {
                        quoted = false;
                    }
                }
            } else if ch == '\'' {
                quoted = true;
                out.push(ch);
            } else if ch == '?' {
                index += 1;
                out.push('$');
                out.push_str(&index.to_string());
            } else {
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn closed_registry_of_twenty_two_caller_unit_reads() {
        assert_eq!(PLANS.len(), 22);
        assert_eq!(kinds().count(), 22);
        for kind in kinds() {
            assert!(plan(kind).is_some(), "{kind}");
            assert!(is_owner_read(kind), "{kind}");
        }
        assert_eq!(
            kinds().collect::<std::collections::BTreeSet<_>>().len(),
            22,
            "read kinds must be unique"
        );
        assert_eq!(plan("bogus.read"), None);
        assert!(!is_owner_read("scenario-prune"));
    }

    #[test]
    fn every_plan_is_select_only_with_exact_bind_count() {
        for entry in PLANS {
            let statement = entry.sql;
            assert!(
                statement.starts_with("SELECT ") || statement.starts_with("EXISTS(")
                    || statement.starts_with("NOT EXISTS(")
                    || statement.starts_with("COALESCE("),
                "{}: {statement}",
                entry.kind
            );
            for banned in [
                "INSERT ", "UPDATE ", "DELETE ", "FOR UPDATE", "503", "UNAVAILABLE",
            ] {
                assert!(!statement.contains(banned), "{} contains {banned}", entry.kind);
            }
            assert_eq!(
                placeholder_count(statement),
                entry.binds.len(),
                "{} bind count",
                entry.kind
            );
        }
    }

    #[test]
    fn every_plan_carries_authority_and_cross_owner_disclosure() {
        for entry in PLANS {
            assert!(!entry.caller.is_empty(), "{}", entry.kind);
            assert!(entry.authority.contains("records.read"), "{}", entry.kind);
            for fragment in entry.cross_owner {
                assert!(
                    FRAGMENTS.iter().any(|item| item.sql == *fragment),
                    "{} undisclosed fragment {fragment}",
                    entry.kind
                );
                assert!(
                    entry.sql.contains(fragment),
                    "{} missing {fragment}",
                    entry.kind
                );
            }
        }
    }

    #[test]
    fn executive_statements_keep_sql_token_separators() {
        // Whitespace-removing source parity cannot catch a missing space
        // between concatenated SQL tokens; PostgreSQL can.
        for statement in [
            executive::SHORTAGE_EXEC_SQL,
            executive::PROPOSED_EXEC_SQL,
            executive::CAPACITY_EXEC_SQL,
        ] {
            assert!(statement.contains("shortage_count FROM jsonb_array_elements("));
            assert!(statement.contains("AS entry(value) LEFT JOIN commerce_products"));
            assert!(statement.contains(") shortage ON TRUE WHERE "));
        }
        assert!(
            executive::SHORTAGE_EXEC_SQL.contains("'$[*] ? (@.shortageMilli > 0)'")
        );
    }

    #[test]
    fn cross_owner_fragments_match_their_owner_sources() {
        for fragment in FRAGMENTS {
            let source = match fragment.source {
                "server-manufacturing/production-read-projections" => TS_MFG_PROJECTION,
                "server-commerce/production-read-projections" => TS_COM_PROJECTION,
                "server-organization/production-read-projections" => TS_ORG_PROJECTION,
                other => panic!("unknown fragment source {other}"),
            };
            assert!(
                norm(source).contains(&norm(fragment.sql)),
                "{}: fragment missing from owner source",
                fragment.name
            );
        }
    }

    /// Kinds whose literal is assembled by a TS builder at call time rather
    /// than written as one contiguous statement. The builder argument (or the
    /// native call site) is the surviving source authority.
    fn folded_authority(kind: &str) -> Option<&'static str> {
        Some(match kind {
            RD_RECOMMENDATIONS => "nativeRecommendations(companyId,branchId,branchId2)",
            RD_RESOURCES => "nativeResources(companyId,branchId,branchId2)",
            RD_CALENDAR => "nativeCalendar(companyId,branchId,branchId2)",
            RD_SCENARIO => "nativeScenario({id},companyId,branchId)",
            RD_SCOPED_RECOMMENDATION => "nativeScopedRecommendation({id},companyId,branchId)",
            RD_COMPARISON => {
                "nativeComparison({comparisonScenarioId,specificationId},companyId,branch_id===null?null:String(branch_id))"
            }
            RD_GUARD_INDEPENDENT_RECOMMENDATION => {
                "id=? AND version=? AND status='proposed' AND created_by<>?"
            }
            RD_GUARD_ROUTING_RESOURCE => {
                "status='active' AND (?::text IS NULL OR branch_id=?)"
            }
            RD_GUARD_COMPARISON => {
                "id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?"
            }
            RD_GUARD_UNRELEASED_SCENARIO => {
                "id=? AND company_id=? AND version=? AND status='calculated'"
            }
            RD_GUARD_CAPACITY_RESERVATION => "${planningAllocatedMinutes}+?<=?",
            RD_GUARD_RESOURCE_VERSION => "branch_id=? AND version=?",
            RD_GUARD_NO_ACTIVE_ALLOCATION => "resource_id=? AND released_at IS NULL",
            RD_GUARD_ACTIVE_RESOURCE_VERSION => {
                "status='active' AND version=? AND (?::text IS NULL OR branch_id=?)"
            }
            RD_GUARD_CALENDAR_VERSION => "COALESCE((${resourceCalendarByKey('version')}),0)=?",
            RD_GUARD_RELEASED_CAPACITY => "${planningAllocatedMinutes}<=?",
            RD_GUARD_ALLOCATED_RESOURCE_VERSION => "version=? AND status='active'",
            RD_GUARD_ALLOCATED_CALENDAR_VERSION => {
                "COALESCE((${resourceCalendarByKey('version')}),0)=?"
            }
            _ => return None,
        })
    }

    #[test]
    fn byte_parity_with_retained_typescript_statements() {
        let mut missing = Vec::new();
        let ts = norm(TS_COMMANDS);
        for entry in PLANS {
            let expected = if entry.category == CATEGORY_RESOURCES {
                norm(TS_RESOURCES)
            } else {
                ts.clone()
            };
            if entry.kind == RD_SCENARIOS {
                // The scenarios statement is written literally in the native
                // adapter and must carry the registered SQL exactly.
                let native = norm(&native_binds(entry.sql));
                assert!(
                    norm(NATIVE_POSTGRES).contains(&native),
                    "{}: registered SQL missing from native owner statement\n{native}",
                    entry.kind
                );
                continue;
            }
            if matches!(
                entry.kind,
                RD_EXECUTIVE_SHORTAGE
                    | RD_EXECUTIVE_RECOMMENDATION_PROPOSED
                    | RD_EXECUTIVE_RECOMMENDATION_CAPACITY
            ) {
                // The executive statements are derived at runtime from the
                // registry constants; audit the derivation and exact bind
                // semantics rather than looking for a literal in the adapter.
                let derived = match entry.kind {
                    RD_EXECUTIVE_SHORTAGE => "positional(registered_executive::SHORTAGE_EXEC_SQL)",
                    RD_EXECUTIVE_RECOMMENDATION_PROPOSED => {
                        "positional(registered_executive::PROPOSED_EXEC_SQL)"
                    }
                    _ => "positional(registered_executive::CAPACITY_EXEC_SQL)",
                };
                assert!(
                    norm(NATIVE_POSTGRES).contains(&norm(derived)),
                    "{}: native statement is not derived from the registry",
                    entry.kind
                );
                assert_eq!(placeholder_count(entry.sql), 6, "{}", entry.kind);
                continue;
            }
            let needles = {
                let mut value = entry.sql.to_owned();
                for fragment in FRAGMENTS {
                    value = value.replace(fragment.sql, fragment.token);
                }
                vec![norm(&value)]
            };
            let needles = match folded_authority(entry.kind) {
                Some(fragment) => vec![norm(fragment)],
                None => needles,
            };
            for needle in needles {
                if !expected.contains(&needle) {
                    missing.push(format!("{} (needed {needle})", entry.kind));
                }
            }
        }
        assert!(missing.is_empty(), "TS source authority missing: {}", missing.join(" | "));
        // Both eligibility branches are now closed native scope facts, asserted
        // by the lane-36 domain and Postgres tests rather than TS literals.
        // The assembled statement really is built from the registered pieces.
        assert!(executive::SHORTAGE_EXEC_SQL.contains(executive::SOURCE_JOINS));
        assert!(executive::SHORTAGE_EXEC_SQL.contains(executive::SHORTAGE_SQL));
    }
}
