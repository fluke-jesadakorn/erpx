//! Tenant-local organization command and read adapters.
//!
//! Organization nodes use the Operations CQRS tables in the holding's
//! physical databases. The write event and snapshot are authoritative; the
//! read row is always projected through the same tenant-scoped store.

use chrono::{DateTime, Utc};
use erpx_domain::{
    access::Scope,
    contracts::ActorAttribution,
    operations::{
        AggregateMetadata, Branch, Company, Department, Headquarters, Holding, OperationsAggregate,
        OrganizationStatus, Team, event_for,
    },
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    PersistenceError,
    operations_persistence::{OperationsJsonReadCursor, SqlxOperationsStore},
    tenant_holding_scope_id,
};

pub const ORGANIZATION_READ_PROJECTION: &str = "operations_read_model";
pub const MAX_ORGANIZATION_NAME_BYTES: usize = 255;
pub const MAX_ORGANIZATION_REFERENCE_BYTES: usize = 255;
pub const MAX_ORGANIZATION_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationNodeKind {
    Holding,
    Company,
    Headquarters,
    Branch,
    Department,
    Team,
}

impl OrganizationNodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Company => "company",
            Self::Headquarters => "headquarters",
            Self::Branch => "branch",
            Self::Department => "department",
            Self::Team => "team",
        }
    }

    pub const fn aggregate_type(self) -> &'static str {
        match self {
            Self::Holding => Holding::AGGREGATE_TYPE,
            Self::Company => Company::AGGREGATE_TYPE,
            Self::Headquarters => Headquarters::AGGREGATE_TYPE,
            Self::Branch => Branch::AGGREGATE_TYPE,
            Self::Department => Department::AGGREGATE_TYPE,
            Self::Team => Team::AGGREGATE_TYPE,
        }
    }

    pub fn parse(value: &str) -> Result<Self, PersistenceError> {
        match value {
            "holding" => Ok(Self::Holding),
            "company" => Ok(Self::Company),
            "headquarters" => Ok(Self::Headquarters),
            "branch" => Ok(Self::Branch),
            "department" => Ok(Self::Department),
            "team" => Ok(Self::Team),
            _ => Err(PersistenceError::InvalidData(
                "organization node kind is unsupported".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationNodeCommand {
    pub kind: OrganizationNodeKind,
    pub node_id: Uuid,
    pub holding_ref: String,
    pub holding_id: Uuid,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub registration_number: String,
    pub status: OrganizationStatus,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationWriteReceipt {
    pub kind: OrganizationNodeKind,
    pub node_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrganizationReadNode {
    pub kind: OrganizationNodeKind,
    pub node_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub registration_number: String,
    pub status: String,
    pub aggregate_version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrganizationReadPage {
    pub nodes: Vec<OrganizationReadNode>,
    pub has_more: bool,
    pub next_cursor: Option<OperationsJsonReadCursor>,
}

#[derive(Debug, Clone)]
pub struct SqlxOrganizationStore {
    operations: SqlxOperationsStore,
}

impl SqlxOrganizationStore {
    pub fn from_pools(
        write_pool: &sqlx::PgPool,
        read_pool: &sqlx::PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Ok(Self {
            operations: SqlxOperationsStore::from_pools(write_pool, read_pool, holding_ref)?,
        })
    }

    pub async fn save(
        &self,
        command: &OrganizationNodeCommand,
    ) -> Result<OrganizationWriteReceipt, PersistenceError> {
        validate_command(command)?;
        validate_routed_holding_ref(&command.holding_ref, self.operations.holding_ref())?;
        validate_routed_holding_id(command.holding_id, self.operations.holding_ref())?;
        validate_relationship_shape(command)?;
        self.validate_parent_references(command).await?;
        let idempotency = self
            .operations
            .idempotency_record(&command.idempotency_key)
            .await?;
        if let Some(record) = &idempotency
            && (record.aggregate_type != command.kind.aggregate_type()
                || record.aggregate_id != command.node_id)
        {
            return Err(PersistenceError::IdempotencyConflict {
                key: command.idempotency_key.clone(),
            });
        }
        if let Some(record) = idempotency {
            return Ok(OrganizationWriteReceipt {
                kind: command.kind,
                node_id: command.node_id,
                aggregate_version: record.aggregate_version,
                event_id: record.event_id,
                applied: false,
            });
        }
        let expected = command.expected_version;
        let receipt = match command.kind {
            OrganizationNodeKind::Holding => {
                let existing = self
                    .operations
                    .get_aggregate::<Holding>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let mut metadata = metadata(
                    command,
                    Scope::Holding {
                        holding_id: command.holding_id,
                    },
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Holding {
                        meta: metadata,
                        code: command.code.clone(),
                        name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
            OrganizationNodeKind::Company => {
                let existing = self
                    .operations
                    .get_aggregate::<Company>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let mut metadata = metadata(
                    command,
                    Scope::Company {
                        holding_id: command.holding_id,
                        company_id: command.node_id,
                    },
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Company {
                        meta: metadata,
                        holding_id: command.holding_id,
                        registration_number: command.registration_number.clone(),
                        legal_name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
            OrganizationNodeKind::Headquarters => {
                let company_id = required_parent(command.company_id, "company_id")?;
                let existing = self
                    .operations
                    .get_aggregate::<Headquarters>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let mut metadata = metadata(
                    command,
                    Scope::Company {
                        holding_id: command.holding_id,
                        company_id,
                    },
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Headquarters {
                        meta: metadata,
                        holding_id: command.holding_id,
                        company_id,
                        name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
            OrganizationNodeKind::Branch => {
                let company_id = required_parent(command.company_id, "company_id")?;
                let existing = self
                    .operations
                    .get_aggregate::<Branch>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let mut metadata = metadata(
                    command,
                    Scope::Branch {
                        holding_id: command.holding_id,
                        company_id,
                        branch_id: command.node_id,
                    },
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Branch {
                        meta: metadata,
                        holding_id: command.holding_id,
                        company_id,
                        code: command.code.clone(),
                        name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
            OrganizationNodeKind::Department => {
                let company_id = required_parent(command.company_id, "company_id")?;
                let existing = self
                    .operations
                    .get_aggregate::<Department>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let scope =
                    scoped_company_or_branch(command.holding_id, company_id, command.branch_id);
                let mut metadata = metadata(
                    command,
                    scope,
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Department {
                        meta: metadata,
                        holding_id: command.holding_id,
                        company_id,
                        branch_id: command.branch_id,
                        name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
            OrganizationNodeKind::Team => {
                let company_id = required_parent(command.company_id, "company_id")?;
                let existing = self
                    .operations
                    .get_aggregate::<Team>(command.node_id)
                    .await?;
                validate_existing_version(
                    existing.as_ref().map(|value| value.meta.version),
                    expected,
                )?;
                let scope =
                    scoped_company_or_branch(command.holding_id, company_id, command.branch_id);
                let mut metadata = metadata(
                    command,
                    scope,
                    existing.as_ref().map(|value| value.meta.created_at),
                )?;
                metadata.version = next_version(expected)?;
                self.persist(
                    command,
                    Team {
                        meta: metadata,
                        holding_id: command.holding_id,
                        company_id,
                        branch_id: command.branch_id,
                        department_id: command.department_id,
                        name: command.name.clone(),
                        status: validated_status(
                            existing.map(|value| value.status),
                            command.status,
                            command.at,
                        )?,
                    },
                    expected,
                )
                .await?
            }
        };
        Ok(receipt)
    }

    /// Validate the authoritative organization hierarchy before constructing
    /// a child aggregate. The API performs the same checks for transport
    /// feedback, but this persistence boundary must not rely on a particular
    /// caller: otherwise a direct worker/recovery caller could create a child
    /// whose parent is missing or belongs to another company.
    async fn validate_parent_references(
        &self,
        command: &OrganizationNodeCommand,
    ) -> Result<(), PersistenceError> {
        let company = match command.kind {
            OrganizationNodeKind::Holding | OrganizationNodeKind::Company => None,
            OrganizationNodeKind::Headquarters
            | OrganizationNodeKind::Branch
            | OrganizationNodeKind::Department
            | OrganizationNodeKind::Team => {
                let company_id = command.company_id.ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "organization child company parent is required".into(),
                    )
                })?;
                Some(self.parent_company(company_id, command.holding_id).await?)
            }
        };

        if let (Some(company), Some(branch_id)) = (company.as_ref(), command.branch_id) {
            self.parent_branch(branch_id, command.holding_id, company.meta.id)
                .await?;
        }

        if command.kind == OrganizationNodeKind::Team
            && let Some(department_id) = command.department_id
        {
            let company_id = command.company_id.ok_or_else(|| {
                PersistenceError::InvalidData("organization team company parent is required".into())
            })?;
            let department = self
                .operations
                .get_aggregate::<Department>(department_id)
                .await?
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "organization team department parent does not exist".into(),
                    )
                })?;
            validate_parent_department(
                &department,
                department_id,
                command.holding_id,
                company_id,
                command.branch_id,
                self.operations.holding_ref(),
            )?;
        }
        Ok(())
    }

    async fn parent_company(
        &self,
        company_id: Uuid,
        holding_id: Uuid,
    ) -> Result<Company, PersistenceError> {
        let company = self
            .operations
            .get_aggregate::<Company>(company_id)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "organization child company parent does not exist".into(),
                )
            })?;
        validate_company_parent(
            &company,
            company_id,
            holding_id,
            self.operations.holding_ref(),
        )?;
        Ok(company)
    }

    async fn read_parent_company(
        &self,
        company_id: Uuid,
        holding_id: Uuid,
    ) -> Result<Company, PersistenceError> {
        let company = self
            .operations
            .get_read_aggregate::<Company>(company_id)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "organization read child company parent does not exist".into(),
                )
            })?
            .value;
        validate_company_parent(
            &company,
            company_id,
            holding_id,
            self.operations.holding_ref(),
        )?;
        Ok(company)
    }

    async fn parent_branch(
        &self,
        branch_id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
    ) -> Result<Branch, PersistenceError> {
        let branch = self
            .operations
            .get_aggregate::<Branch>(branch_id)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData("organization branch parent does not exist".into())
            })?;
        validate_branch_parent(
            &branch,
            branch_id,
            holding_id,
            company_id,
            self.operations.holding_ref(),
        )?;
        Ok(branch)
    }

    async fn read_parent_branch(
        &self,
        branch_id: Uuid,
        holding_id: Uuid,
        company_id: Uuid,
    ) -> Result<Branch, PersistenceError> {
        let branch = self
            .operations
            .get_read_aggregate::<Branch>(branch_id)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "organization read branch parent does not exist".into(),
                )
            })?
            .value;
        validate_branch_parent(
            &branch,
            branch_id,
            holding_id,
            company_id,
            self.operations.holding_ref(),
        )?;
        Ok(branch)
    }

    async fn persist<A: OperationsAggregate>(
        &self,
        command: &OrganizationNodeCommand,
        aggregate: A,
        expected: Option<u64>,
    ) -> Result<OrganizationWriteReceipt, PersistenceError> {
        aggregate_validate(&aggregate)?;
        let mut event = event_for(
            &aggregate,
            format!("organization.{}.saved", command.kind.as_str()),
            1,
            command.actor.clone(),
            Some(command.idempotency_key.clone()),
            command.correlation_id,
            None,
            command.at,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "erpx:organization:v1:{}:{}:{}",
                command.holding_ref,
                command.kind.as_str(),
                command.idempotency_key
            )
            .as_bytes(),
        );
        event.envelope.event_id = event_id;
        event.projection.event_id = event_id;
        let applied = self
            .operations
            .apply_event(&event, &aggregate, expected, &[])
            .await?;
        self.operations.project_event(&event, &aggregate).await?;
        Ok(OrganizationWriteReceipt {
            kind: command.kind,
            node_id: command.node_id,
            aggregate_version: applied.aggregate_version,
            event_id,
            applied: applied.applied,
        })
    }

    pub async fn list(
        &self,
        kind: Option<OrganizationNodeKind>,
        cursor: Option<OperationsJsonReadCursor>,
        page_size: u32,
    ) -> Result<OrganizationReadPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_ORGANIZATION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "organization page size is out of bounds".into(),
            ));
        }
        let prefix = kind.map_or_else(
            || "organization.".to_owned(),
            |value| value.aggregate_type().to_owned(),
        );
        let page = self
            .operations
            .read_json_page(&prefix, cursor, page_size)
            .await?;
        let nodes = page
            .rows
            .into_iter()
            .map(read_node)
            .collect::<Result<Vec<_>, _>>()?;
        for node in &nodes {
            self.validate_read_parent_references(node).await?;
        }
        Ok(OrganizationReadPage {
            nodes,
            has_more: page.has_more,
            next_cursor: page.next_cursor,
        })
    }

    async fn validate_read_parent_references(
        &self,
        node: &OrganizationReadNode,
    ) -> Result<(), PersistenceError> {
        match node.kind {
            OrganizationNodeKind::Holding | OrganizationNodeKind::Company => Ok(()),
            OrganizationNodeKind::Headquarters | OrganizationNodeKind::Branch => {
                let company_id = node.company_id.ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "organization read child is missing its company parent".into(),
                    )
                })?;
                self.read_parent_company(company_id, node.holding_id)
                    .await?;
                Ok(())
            }
            OrganizationNodeKind::Department => {
                let company_id = node.company_id.ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "organization read department is missing its company parent".into(),
                    )
                })?;
                self.read_parent_company(company_id, node.holding_id)
                    .await?;
                if let Some(branch_id) = node.branch_id {
                    self.read_parent_branch(branch_id, node.holding_id, company_id)
                        .await?;
                }
                Ok(())
            }
            OrganizationNodeKind::Team => {
                let company_id = node.company_id.ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "organization read team is missing its company parent".into(),
                    )
                })?;
                self.read_parent_company(company_id, node.holding_id)
                    .await?;
                if let Some(branch_id) = node.branch_id {
                    self.read_parent_branch(branch_id, node.holding_id, company_id)
                        .await?;
                }
                if let Some(department_id) = node.department_id {
                    let department = self
                        .operations
                        .get_read_aggregate::<Department>(department_id)
                        .await?
                        .ok_or_else(|| {
                            PersistenceError::InvalidData(
                                "organization read team department parent does not exist".into(),
                            )
                        })?
                        .value;
                    validate_parent_department(
                        &department,
                        department_id,
                        node.holding_id,
                        company_id,
                        node.branch_id,
                        self.operations.holding_ref(),
                    )?;
                }
                Ok(())
            }
        }
    }
}

fn metadata(
    command: &OrganizationNodeCommand,
    scope: Scope,
    created_at: Option<DateTime<Utc>>,
) -> Result<AggregateMetadata, PersistenceError> {
    let mut metadata = AggregateMetadata::new(
        command.node_id,
        command.holding_ref.clone(),
        scope,
        created_at.unwrap_or(command.at),
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    metadata.updated_at = command.at;
    Ok(metadata)
}

fn scoped_company_or_branch(holding_id: Uuid, company_id: Uuid, branch_id: Option<Uuid>) -> Scope {
    branch_id.map_or(
        Scope::Company {
            holding_id,
            company_id,
        },
        |branch_id| Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
    )
}

fn required_parent(value: Option<Uuid>, field: &'static str) -> Result<Uuid, PersistenceError> {
    value
        .filter(|value| !value.is_nil())
        .ok_or_else(|| PersistenceError::InvalidData(format!("{field} is required")))
}

fn validate_command(command: &OrganizationNodeCommand) -> Result<(), PersistenceError> {
    if command.node_id.is_nil() || command.holding_id.is_nil() || command.correlation_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "organization identifiers are invalid".into(),
        ));
    }
    for (value, field, max) in [
        (
            &command.holding_ref,
            "holding_ref",
            MAX_ORGANIZATION_REFERENCE_BYTES,
        ),
        (
            &command.idempotency_key,
            "idempotency_key",
            MAX_ORGANIZATION_REFERENCE_BYTES,
        ),
        (&command.name, "name", MAX_ORGANIZATION_NAME_BYTES),
    ] {
        if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
        }
    }
    for (value, field) in [
        (&command.code, "code"),
        (&command.registration_number, "registration_number"),
    ] {
        if value.len() > MAX_ORGANIZATION_REFERENCE_BYTES || value.chars().any(char::is_control) {
            return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
        }
    }
    if command.expected_version == Some(0) {
        return Err(PersistenceError::InvalidData(
            "expected organization version must be positive".into(),
        ));
    }
    if command.actor.principal_ref.trim().is_empty() {
        return Err(PersistenceError::InvalidData(
            "organization actor is required".into(),
        ));
    }
    Ok(())
}

fn validate_relationship_shape(command: &OrganizationNodeCommand) -> Result<(), PersistenceError> {
    let invalid = |message: &'static str| Err(PersistenceError::InvalidData(message.into()));
    match command.kind {
        OrganizationNodeKind::Holding => {
            if command.node_id != command.holding_id
                || command.company_id.is_some()
                || command.branch_id.is_some()
                || command.department_id.is_some()
            {
                return invalid(
                    "holding organization node must use its holding ID and no child parent IDs",
                );
            }
        }
        OrganizationNodeKind::Company => {
            if command.company_id.is_some()
                || command.branch_id.is_some()
                || command.department_id.is_some()
            {
                return invalid("company organization node cannot carry child parent IDs");
            }
        }
        OrganizationNodeKind::Headquarters | OrganizationNodeKind::Branch => {
            if command.company_id.is_none()
                || command.branch_id.is_some()
                || command.department_id.is_some()
            {
                return invalid("headquarters and branch nodes require only a company parent ID");
            }
        }
        OrganizationNodeKind::Department => {
            if command.company_id.is_none() || command.department_id.is_some() {
                return invalid(
                    "department organization node requires a company parent and no department parent",
                );
            }
        }
        OrganizationNodeKind::Team => {
            if command.company_id.is_none() {
                return invalid("team organization node requires a company parent");
            }
        }
    }
    Ok(())
}

fn validate_company_parent(
    company: &Company,
    company_id: Uuid,
    holding_id: Uuid,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    if company.meta.id != company_id
        || company.holding_id != holding_id
        || company.meta.holding_ref != holding_ref
    {
        return Err(PersistenceError::InvalidData(
            "organization child company parent is outside the routed holding".into(),
        ));
    }
    company
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
}

fn validate_branch_parent(
    branch: &Branch,
    branch_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    if branch.meta.id != branch_id
        || branch.holding_id != holding_id
        || branch.company_id != company_id
        || branch.meta.holding_ref != holding_ref
    {
        return Err(PersistenceError::InvalidData(
            "organization branch parent is outside the requested company".into(),
        ));
    }
    branch
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
}

fn validate_parent_department(
    department: &Department,
    department_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    if department.meta.id != department_id
        || department.holding_id != holding_id
        || department.company_id != company_id
        || department.meta.holding_ref != holding_ref
        || department.branch_id != branch_id
    {
        return Err(PersistenceError::InvalidData(
            "organization team department parent is outside the requested hierarchy".into(),
        ));
    }
    department
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
}

fn validate_existing_version(
    actual: Option<u64>,
    expected: Option<u64>,
) -> Result<(), PersistenceError> {
    match (actual, expected) {
        (None, None) | (Some(_), Some(_)) => Ok(()),
        (None, Some(expected)) => Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "organization".into(),
            aggregate_id: Uuid::nil(),
            expected: Some(expected),
            actual: None,
        }),
        (Some(actual), None) => Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "organization".into(),
            aggregate_id: Uuid::nil(),
            expected: None,
            actual: Some(actual),
        }),
    }
}

fn next_version(expected: Option<u64>) -> Result<u64, PersistenceError> {
    expected
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("organization version overflow".into()))
}

fn validated_status(
    existing: Option<OrganizationStatus>,
    requested: OrganizationStatus,
    at: DateTime<Utc>,
) -> Result<OrganizationStatus, PersistenceError> {
    if let Some(current) = existing {
        let mut probe = current;
        if probe != requested {
            probe = requested;
            let mut metadata = AggregateMetadata::new(
                Uuid::new_v4(),
                "organization-status-validation",
                Scope::Holding {
                    holding_id: Uuid::new_v4(),
                },
                at,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            metadata.updated_at = at;
            let mut value = Holding {
                meta: metadata,
                code: "status".into(),
                name: "status".into(),
                status: current,
            };
            value
                .transition_to(requested, at)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        }
        Ok(probe)
    } else if requested == OrganizationStatus::Active {
        Ok(requested)
    } else {
        Err(PersistenceError::InvalidData(
            "organization nodes must be created active".into(),
        ))
    }
}

fn aggregate_validate<A: OperationsAggregate>(aggregate: &A) -> Result<(), PersistenceError> {
    aggregate
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
}

fn read_node(
    row: crate::operations_persistence::OperationsJsonReadRow,
) -> Result<OrganizationReadNode, PersistenceError> {
    let kind = match row.aggregate_type.as_str() {
        Holding::AGGREGATE_TYPE => OrganizationNodeKind::Holding,
        Company::AGGREGATE_TYPE => OrganizationNodeKind::Company,
        Headquarters::AGGREGATE_TYPE => OrganizationNodeKind::Headquarters,
        Branch::AGGREGATE_TYPE => OrganizationNodeKind::Branch,
        Department::AGGREGATE_TYPE => OrganizationNodeKind::Department,
        Team::AGGREGATE_TYPE => OrganizationNodeKind::Team,
        _ => {
            return Err(PersistenceError::InvalidData(
                "unknown organization read aggregate".into(),
            ));
        }
    };
    let id = row
        .state
        .get("meta")
        .and_then(|meta| meta.get("id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| PersistenceError::InvalidData("organization read ID is invalid".into()))?;
    if id != row.aggregate_id {
        return Err(PersistenceError::InvalidData(
            "organization read ID does not match its key".into(),
        ));
    }
    validate_read_aggregate(
        kind,
        &row.state,
        row.aggregate_id,
        row.aggregate_version,
        &row.holding_ref,
        &row.lifecycle_status,
    )?;
    let uuid_field = |field: &str| -> Result<Option<Uuid>, PersistenceError> {
        row.state
            .get(field)
            .and_then(Value::as_str)
            .map(|value| {
                Uuid::parse_str(value).map_err(|_| {
                    PersistenceError::InvalidData(format!("organization {field} is invalid"))
                })
            })
            .transpose()
    };
    let holding_id = uuid_field("holding_id")?
        .or_else(|| {
            row.state
                .get("meta")
                .and_then(|meta| meta.get("scope"))
                .and_then(|scope| scope.get("Holding"))
                .and_then(|value| value.get("holding_id"))
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
        })
        .ok_or_else(|| {
            PersistenceError::InvalidData("organization holding ID is invalid".into())
        })?;
    validate_routed_holding_id(holding_id, &row.holding_ref)?;
    if kind == OrganizationNodeKind::Holding && id != holding_id {
        return Err(PersistenceError::InvalidData(
            "organization holding node ID does not match its holding ID".into(),
        ));
    }
    Ok(OrganizationReadNode {
        kind,
        node_id: id,
        holding_id,
        company_id: uuid_field("company_id")?,
        branch_id: uuid_field("branch_id")?,
        department_id: uuid_field("department_id")?,
        code: row
            .state
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        name: row
            .state
            .get("name")
            .or_else(|| row.state.get("legal_name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        registration_number: row
            .state
            .get("registration_number")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        status: row.lifecycle_status,
        aggregate_version: row.aggregate_version,
        updated_at: row.updated_at,
    })
}

fn validate_routed_holding_ref(
    command_holding_ref: &str,
    routed_holding_ref: &str,
) -> Result<(), PersistenceError> {
    if command_holding_ref != routed_holding_ref {
        return Err(PersistenceError::InvalidData(
            "organization command holding reference does not match the routed tenant".into(),
        ));
    }
    Ok(())
}

fn validate_routed_holding_id(
    command_holding_id: Uuid,
    routed_holding_ref: &str,
) -> Result<(), PersistenceError> {
    if command_holding_id != tenant_holding_scope_id(routed_holding_ref) {
        return Err(PersistenceError::InvalidData(
            "organization holding ID does not match the routed holding reference".into(),
        ));
    }
    Ok(())
}

fn validate_read_aggregate(
    kind: OrganizationNodeKind,
    state: &Value,
    aggregate_id: Uuid,
    aggregate_version: u64,
    holding_ref: &str,
    lifecycle_status: &str,
) -> Result<(), PersistenceError> {
    macro_rules! validate {
        ($type:ty) => {{
            let aggregate: $type = serde_json::from_value(state.clone())?;
            let metadata = aggregate.metadata();
            if metadata.id != aggregate_id
                || metadata.version != aggregate_version
                || metadata.holding_ref != holding_ref
                || aggregate.lifecycle_status() != lifecycle_status
            {
                return Err(PersistenceError::InvalidData(
                    "organization read aggregate metadata does not match its projection key".into(),
                ));
            }
            aggregate_validate(&aggregate)
        }};
    }

    match kind {
        OrganizationNodeKind::Holding => validate!(Holding),
        OrganizationNodeKind::Company => validate!(Company),
        OrganizationNodeKind::Headquarters => validate!(Headquarters),
        OrganizationNodeKind::Branch => validate!(Branch),
        OrganizationNodeKind::Department => validate!(Department),
        OrganizationNodeKind::Team => validate!(Team),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use erpx_domain::contracts::ActorKind;

    fn shape_command(
        kind: OrganizationNodeKind,
        node_id: Uuid,
        holding_id: Uuid,
        company_id: Option<Uuid>,
        branch_id: Option<Uuid>,
        department_id: Option<Uuid>,
    ) -> OrganizationNodeCommand {
        OrganizationNodeCommand {
            kind,
            node_id,
            holding_ref: "holding-organization-test".into(),
            holding_id,
            company_id,
            branch_id,
            department_id,
            code: "NODE".into(),
            name: "Organization node".into(),
            registration_number: "REG-NODE".into(),
            status: OrganizationStatus::Active,
            expected_version: None,
            idempotency_key: "organization-shape-test".into(),
            actor: ActorAttribution {
                principal_ref: "organization-test-owner".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::from_u128(0x88ff),
            at: Utc::now(),
        }
    }

    fn company_read_row(state: Value) -> crate::operations_persistence::OperationsJsonReadRow {
        let at = Utc::now();
        crate::operations_persistence::OperationsJsonReadRow {
            aggregate_type: Company::AGGREGATE_TYPE.into(),
            aggregate_id: Uuid::from_u128(0x8801),
            holding_ref: "holding-organization-test".into(),
            aggregate_version: 1,
            lifecycle_status: "active".into(),
            state,
            updated_at: at,
        }
    }

    #[test]
    fn organization_write_shape_requires_canonical_hierarchy_ids() {
        let non_canonical_holding = shape_command(
            OrganizationNodeKind::Holding,
            Uuid::from_u128(0x8801),
            Uuid::from_u128(0x8802),
            None,
            None,
            None,
        );
        assert!(matches!(
            validate_relationship_shape(&non_canonical_holding),
            Err(PersistenceError::InvalidData(message))
                if message.contains("holding ID")
        ));

        let company_with_child_parent = shape_command(
            OrganizationNodeKind::Company,
            Uuid::from_u128(0x8801),
            Uuid::from_u128(0x8800),
            Some(Uuid::from_u128(0x8802)),
            None,
            None,
        );
        assert!(matches!(
            validate_relationship_shape(&company_with_child_parent),
            Err(PersistenceError::InvalidData(message))
                if message.contains("cannot carry child parent IDs")
        ));
    }

    #[test]
    fn organization_write_shape_rejects_ambiguous_parent_fields() {
        let headquarters_with_branch_parent = shape_command(
            OrganizationNodeKind::Headquarters,
            Uuid::from_u128(0x8803),
            Uuid::from_u128(0x8800),
            Some(Uuid::from_u128(0x8801)),
            Some(Uuid::from_u128(0x8802)),
            None,
        );
        assert!(matches!(
            validate_relationship_shape(&headquarters_with_branch_parent),
            Err(PersistenceError::InvalidData(message))
                if message.contains("only a company parent ID")
        ));

        let department_with_department_parent = shape_command(
            OrganizationNodeKind::Department,
            Uuid::from_u128(0x8804),
            Uuid::from_u128(0x8800),
            Some(Uuid::from_u128(0x8801)),
            None,
            Some(Uuid::from_u128(0x8805)),
        );
        assert!(matches!(
            validate_relationship_shape(&department_with_department_parent),
            Err(PersistenceError::InvalidData(message))
                if message.contains("no department parent")
        ));
    }

    fn valid_company_state() -> Value {
        let at = Utc::now();
        let holding_id = tenant_holding_scope_id("holding-organization-test");
        let company_id = Uuid::from_u128(0x8801);
        let mut metadata = AggregateMetadata::new(
            company_id,
            "holding-organization-test",
            Scope::Company {
                holding_id,
                company_id,
            },
            at,
        )
        .expect("valid organization metadata");
        metadata.version = 1;
        serde_json::to_value(Company {
            meta: metadata,
            holding_id,
            registration_number: "REG-8801".into(),
            legal_name: "Organization Test Company".into(),
            status: OrganizationStatus::Active,
        })
        .expect("serializable company")
    }

    #[test]
    fn read_projection_rejects_mismatched_typed_scope_and_version() {
        let mut state = valid_company_state();
        state["holding_id"] = serde_json::json!(Uuid::from_u128(0x8810).to_string());
        let row = company_read_row(state);
        assert!(matches!(
            read_node(row),
            Err(PersistenceError::InvalidData(_))
        ));

        let mut stale_state = valid_company_state();
        stale_state["meta"]["version"] = serde_json::json!(2);
        let row = company_read_row(stale_state);
        assert!(matches!(
            read_node(row),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match")
        ));
    }

    #[test]
    fn command_holding_reference_must_match_the_routed_tenant() {
        assert!(validate_routed_holding_ref("holding-a", "holding-a").is_ok());
        assert!(matches!(
            validate_routed_holding_ref("holding-b", "holding-a"),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match the routed tenant")
        ));
    }

    #[test]
    fn command_holding_id_must_match_the_routed_holding_reference() {
        let holding_ref = "holding-organization-test";
        assert!(
            validate_routed_holding_id(tenant_holding_scope_id(holding_ref), holding_ref,).is_ok()
        );
        assert!(matches!(
            validate_routed_holding_id(Uuid::from_u128(0x8800), holding_ref),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match")
        ));
    }

    #[test]
    fn valid_company_projection_keeps_the_read_contract() {
        let row = company_read_row(valid_company_state());
        let node = read_node(row).expect("valid company projection");
        assert_eq!(node.kind, OrganizationNodeKind::Company);
        assert_eq!(node.node_id, Uuid::from_u128(0x8801));
        assert_eq!(node.aggregate_version, 1);
        assert!(node.updated_at <= Utc::now() + Duration::seconds(1));
    }
}
