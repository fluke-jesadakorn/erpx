//! Read-only new-command preparation. Current personal authorization and exact
//! replay precede this phase in Service/Workflow; mandatory ports retain actual
//! owner/Inventory/Finance proofs and perform no independent commit here.
use crate::domain::parts::{FieldServicePartsActionKind, require_future_reservation};
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::CommandJson;
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceLotSelection {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub balance_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldServicePartsInput {
    Reserve {
        expected_version: i64,
        warehouse_id: String,
        product_id: String,
        quantity_milli: i64,
        expires_at: i64,
        reason: String,
    },
    Issue {
        expected_version: i64,
        reservation_id: String,
        quantity_milli: i64,
        entry_date: Option<String>,
        allocations: Vec<FieldServiceLotSelection>,
        reason: String,
    },
}
impl FieldServicePartsInput {
    pub fn action(&self) -> FieldServicePartsActionKind {
        match self {
            Self::Reserve { .. } => FieldServicePartsActionKind::Reserve,
            Self::Issue { .. } => FieldServicePartsActionKind::Issue,
        }
    }
    pub fn expected_version(&self) -> i64 {
        match self {
            Self::Reserve {
                expected_version, ..
            }
            | Self::Issue {
                expected_version, ..
            } => *expected_version,
        }
    }
    pub fn reason(&self) -> &str {
        match self {
            Self::Reserve { reason, .. } | Self::Issue { reason, .. } => reason,
        }
    }
}
/// Validated request data, no source/current-actor factory. Only this owner's
/// schema adapter constructs the normalized receipt identity.
#[derive(Clone)]
pub struct NormalizedFieldServiceParts {
    pub(crate) input: FieldServicePartsInput,
    pub(crate) receipt_input: CommandJson,
}
impl NormalizedFieldServiceParts {
    pub fn input(&self) -> &FieldServicePartsInput {
        &self.input
    }
    pub fn receipt_input(&self) -> &CommandJson {
        &self.receipt_input
    }
    pub fn into_command(self, key: CommandKey, order_id: RecordId) -> FieldServicePartsCommand {
        FieldServicePartsCommand {
            key,
            order_id,
            normalized: self,
        }
    }
}
#[derive(Clone)]
pub struct FieldServicePartsCommand {
    key: CommandKey,
    order_id: RecordId,
    normalized: NormalizedFieldServiceParts,
}
impl FieldServicePartsCommand {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub fn order_id(&self) -> &RecordId {
        &self.order_id
    }
    pub fn input(&self) -> &FieldServicePartsInput {
        &self.normalized.input
    }
    pub fn receipt_input(&self) -> &CommandJson {
        &self.normalized.receipt_input
    }
    pub fn operation(&self) -> &'static str {
        self.input().action().operation()
    }
}
/// Data obtained from a real retained consumption preparation, not authority.
pub struct FieldServicePreparedConsumptionFacts<'a> {
    pub consumption_id: &'a str,
    pub journal_id: &'a str,
    pub value_minor: i64,
}
pub trait FieldServicePartsPreparationPorts: Send {
    type Source: Sync;
    type Reservation: Send + Sync;
    type Consumption: Send;
    type Error: From<BusinessError> + Send;
    /// Must verify original current owner/action facts, not a cached boolean.
    fn require_action(
        &mut self,
        source: &Self::Source,
        command: &FieldServicePartsCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn application_now(&self) -> TimestampMillis;
    fn reservation(
        &mut self,
        source: &Self::Source,
        id: &str,
    ) -> impl Future<Output = Result<Option<Self::Reservation>, Self::Error>> + Send;
    fn prepare_consumption(
        &mut self,
        source: &Self::Source,
        reservation: &Self::Reservation,
        command: &FieldServicePartsCommand,
        application_at: TimestampMillis,
    ) -> impl Future<Output = Result<Self::Consumption, Self::Error>> + Send;
    fn consumption_facts(value: &Self::Consumption) -> FieldServicePreparedConsumptionFacts<'_>;
}
/// Owns only prepared data/effects. Persistence still requires the actual
/// original owner source, same unit, late version CAS and after-image proof.
pub struct PreparedFieldServiceParts<C> {
    command: FieldServicePartsCommand,
    at: TimestampMillis,
    details: CommandJson,
    consumption: Option<C>,
}
impl<C> PreparedFieldServiceParts<C> {
    pub fn command(&self) -> &FieldServicePartsCommand {
        &self.command
    }
    pub fn application_at(&self) -> TimestampMillis {
        self.at
    }
    pub fn details(&self) -> &CommandJson {
        &self.details
    }
    pub fn consumption(&self) -> Option<&C> {
        self.consumption.as_ref()
    }
    pub fn into_consumption(self) -> Option<C> {
        self.consumption
    }
}
pub async fn prepare_new_field_service_parts<P: FieldServicePartsPreparationPorts>(
    ports: &mut P,
    source: &P::Source,
    command: &FieldServicePartsCommand,
) -> Result<PreparedFieldServiceParts<P::Consumption>, P::Error> {
    ports.require_action(source, command).await?;
    let now = ports.application_now();
    let (details, consumption) = match command.input() {
        FieldServicePartsInput::Reserve { expires_at, .. } => {
            require_future_reservation(*expires_at, now.get())?;
            (
                object(&[
                    ("warehouseId", original(command, "warehouseId")?),
                    ("productId", original(command, "productId")?),
                    ("quantityMilli", original(command, "quantityMilli")?),
                ])?,
                None,
            )
        }
        FieldServicePartsInput::Issue { reservation_id, .. } => {
            let reservation = ports
                .reservation(source, reservation_id)
                .await?
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::NotFound,
                        "FIELD_SERVICE_PART_NOT_FOUND",
                        "Select a part reserved for this service order.",
                    )
                })?;
            let consumption = ports
                .prepare_consumption(source, &reservation, command, now)
                .await?;
            let facts = P::consumption_facts(&consumption);
            let details = object(&[
                ("reservationId", original(command, "reservationId")?),
                ("quantityMilli", original(command, "quantityMilli")?),
                (
                    "consumptionId",
                    CommandJson::from_serializable(&facts.consumption_id)?,
                ),
                (
                    "costMinor",
                    CommandJson::from_serializable(&facts.value_minor)?,
                ),
                (
                    "journalId",
                    CommandJson::from_serializable(&facts.journal_id)?,
                ),
            ])?;
            (details, Some(consumption))
        }
    };
    Ok(PreparedFieldServiceParts {
        command: command.clone(),
        at: now,
        details,
        consumption,
    })
}
fn original(command: &FieldServicePartsCommand, name: &str) -> Result<CommandJson, BusinessError> {
    command.receipt_input().property(name)?.ok_or_else(|| {
        BusinessError::invalid(
            "FIELD_SERVICE_COMMAND_INPUT",
            "Retain the normalized original Service command.",
        )
    })
}
fn object(fields: &[(&str, CommandJson)]) -> Result<CommandJson, BusinessError> {
    let mut text = String::from("{");
    for (index, (name, value)) in fields.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(CommandJson::from_serializable(name)?.as_str());
        text.push(':');
        text.push_str(value.as_str());
    }
    text.push('}');
    CommandJson::parse(&text)
}
