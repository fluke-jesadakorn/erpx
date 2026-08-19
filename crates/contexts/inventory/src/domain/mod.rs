mod records;

pub use records::{
    InventoryLifecycleRecord, InventoryLifecycleRecordError, InventoryLifecycleRecordKind,
    InventoryLifecycleRecordStatus,
};

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CODE_BYTES: usize = 128;

macro_rules! domain_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, InventoryDomainError> {
                if value.is_nil() {
                    return Err(InventoryDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
        impl TryFrom<Uuid> for $name {
            type Error = InventoryDomainError;
            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }
    };
}

domain_id!(StockItemId, "stock_item_id");
domain_id!(WarehouseId, "warehouse_id");
domain_id!(StockMovementId, "stock_movement_id");
domain_id!(ReservationId, "reservation_id");
domain_id!(ProcurementLineReference, "procurement_line_reference");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InventoryDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Inventory records require an explicit company or branch scope")]
    InvalidScope,
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("stock quantity must be positive")]
    NonPositiveQuantity,
    #[error("stock arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("requested quantity exceeds available stock")]
    InsufficientAvailable,
    #[error("reservation identifier already exists")]
    DuplicateReservation,
    #[error("reservation is not active")]
    ReservationNotActive,
    #[error("reservation belongs to a different demand")]
    ReservationDemandMismatch,
    #[error("aggregate timestamp cannot move backwards")]
    InvalidTime,
    #[error("aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationStatus {
    Active,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    pub reservation_id: ReservationId,
    pub quantity: i64,
    pub demand_reference: String,
    pub status: ReservationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    Created,
    StockMoved {
        movement_id: StockMovementId,
        quantity_delta: i64,
        procurement_line: Option<ProcurementLineReference>,
        at: DateTime<Utc>,
    },
    Reserved {
        reservation: Reservation,
        at: DateTime<Utc>,
    },
    ReservationReleased {
        reservation_id: ReservationId,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItem {
    id: StockItemId,
    version: u64,
    scope: AuthorityScope,
    warehouse_id: WarehouseId,
    item_code: String,
    on_hand: i64,
    reserved: i64,
    reservations: BTreeMap<ReservationId, Reservation>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockItemState {
    pub id: StockItemId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub warehouse_id: WarehouseId,
    pub item_code: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub reservations: BTreeMap<ReservationId, Reservation>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StockItem {
    pub fn try_rehydrate(state: StockItemState) -> Result<Self, InventoryDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            warehouse_id: state.warehouse_id,
            item_code: state.item_code,
            on_hand: state.on_hand,
            reserved: state.reserved,
            reservations: state.reservations,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn register(
        id: StockItemId,
        scope: AuthorityScope,
        warehouse_id: WarehouseId,
        item_code: String,
        at: DateTime<Utc>,
    ) -> Result<Self, InventoryDomainError> {
        validate_record_scope(&scope)?;
        validate_code(&item_code, "item_code")?;
        let value = Self {
            id,
            version: 1,
            scope,
            warehouse_id,
            item_code,
            on_hand: 0,
            reserved: 0,
            reservations: BTreeMap::new(),
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::Created],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), InventoryDomainError> {
        validate_record_scope(&self.scope)?;
        validate_code(&self.item_code, "item_code")?;
        if self.version == 0 {
            return Err(InventoryDomainError::VersionOverflow);
        }
        if self.on_hand < 0 || self.reserved < 0 || self.reserved > self.on_hand {
            return Err(InventoryDomainError::InsufficientAvailable);
        }
        let active_reserved = self
            .reservations
            .values()
            .filter(|reservation| reservation.status == ReservationStatus::Active)
            .try_fold(0_i64, |total, reservation| {
                total
                    .checked_add(reservation.quantity)
                    .ok_or(InventoryDomainError::ArithmeticOverflow)
            })?;
        if active_reserved != self.reserved {
            return Err(InventoryDomainError::ReservationDemandMismatch);
        }
        if self.updated_at < self.created_at {
            return Err(InventoryDomainError::InvalidTime);
        }
        Ok(())
    }

    pub fn receive(
        &mut self,
        movement_id: StockMovementId,
        quantity: i64,
        procurement_line: Option<ProcurementLineReference>,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryDomainError> {
        positive(quantity)?;
        let next = self
            .on_hand
            .checked_add(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        self.advance(at)?;
        self.on_hand = next;
        self.uncommitted_events.push(DomainEvent::StockMoved {
            movement_id,
            quantity_delta: quantity,
            procurement_line,
            at,
        });
        Ok(())
    }

    pub fn issue(
        &mut self,
        movement_id: StockMovementId,
        quantity: i64,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryDomainError> {
        positive(quantity)?;
        if quantity > self.available() {
            return Err(InventoryDomainError::InsufficientAvailable);
        }
        let next = self
            .on_hand
            .checked_sub(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        self.advance(at)?;
        self.on_hand = next;
        self.uncommitted_events.push(DomainEvent::StockMoved {
            movement_id,
            quantity_delta: -quantity,
            procurement_line: None,
            at,
        });
        Ok(())
    }

    pub fn reserve(
        &mut self,
        reservation_id: ReservationId,
        quantity: i64,
        demand_reference: String,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryDomainError> {
        positive(quantity)?;
        validate_code(&demand_reference, "demand_reference")?;
        if self.reservations.contains_key(&reservation_id) {
            return Err(InventoryDomainError::DuplicateReservation);
        }
        if quantity > self.available() {
            return Err(InventoryDomainError::InsufficientAvailable);
        }
        let next = self
            .reserved
            .checked_add(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        self.advance(at)?;
        self.reserved = next;
        let reservation = Reservation {
            reservation_id,
            quantity,
            demand_reference,
            status: ReservationStatus::Active,
        };
        self.reservations
            .insert(reservation_id, reservation.clone());
        self.uncommitted_events
            .push(DomainEvent::Reserved { reservation, at });
        Ok(())
    }

    pub fn release_reservation(
        &mut self,
        reservation_id: ReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryDomainError> {
        let quantity = self
            .reservations
            .get(&reservation_id)
            .filter(|reservation| reservation.status == ReservationStatus::Active)
            .map(|reservation| reservation.quantity)
            .ok_or(InventoryDomainError::ReservationNotActive)?;
        let next = self
            .reserved
            .checked_sub(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        self.advance(at)?;
        self.reserved = next;
        self.reservations
            .get_mut(&reservation_id)
            .expect("active reservation was checked")
            .status = ReservationStatus::Released;
        self.uncommitted_events
            .push(DomainEvent::ReservationReleased { reservation_id, at });
        Ok(())
    }

    /// Consume an active reservation and its stock in one aggregate version.
    /// This prevents the release-then-issue race that can expose reserved
    /// quantity to another demand between two commands.
    pub fn fulfill_reservation(
        &mut self,
        reservation_id: ReservationId,
        movement_id: StockMovementId,
        at: DateTime<Utc>,
    ) -> Result<(), InventoryDomainError> {
        let quantity = self
            .reservations
            .get(&reservation_id)
            .filter(|reservation| reservation.status == ReservationStatus::Active)
            .map(|reservation| reservation.quantity)
            .ok_or(InventoryDomainError::ReservationNotActive)?;
        let next_reserved = self
            .reserved
            .checked_sub(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        let next_on_hand = self
            .on_hand
            .checked_sub(quantity)
            .ok_or(InventoryDomainError::ArithmeticOverflow)?;
        self.advance(at)?;
        self.reserved = next_reserved;
        self.on_hand = next_on_hand;
        self.reservations
            .get_mut(&reservation_id)
            .expect("active reservation was checked")
            .status = ReservationStatus::Released;
        self.uncommitted_events
            .push(DomainEvent::ReservationReleased { reservation_id, at });
        self.uncommitted_events.push(DomainEvent::StockMoved {
            movement_id,
            quantity_delta: -quantity,
            procurement_line: None,
            at,
        });
        self.validate()
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), InventoryDomainError> {
        if at < self.updated_at {
            return Err(InventoryDomainError::InvalidTime);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(InventoryDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> StockItemId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn warehouse_id(&self) -> WarehouseId {
        self.warehouse_id
    }
    pub fn item_code(&self) -> &str {
        &self.item_code
    }
    pub const fn on_hand(&self) -> i64 {
        self.on_hand
    }
    pub const fn reserved(&self) -> i64 {
        self.reserved
    }
    pub const fn available(&self) -> i64 {
        self.on_hand - self.reserved
    }
    pub fn reservations(&self) -> &BTreeMap<ReservationId, Reservation> {
        &self.reservations
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn validate_record_scope(scope: &AuthorityScope) -> Result<(), InventoryDomainError> {
    scope
        .validate()
        .map_err(|_| InventoryDomainError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(InventoryDomainError::InvalidScope)
    }
}

fn validate_code(value: &str, field: &'static str) -> Result<(), InventoryDomainError> {
    if value.trim().is_empty()
        || value.len() > MAX_CODE_BYTES
        || value.chars().any(char::is_control)
    {
        Err(InventoryDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn positive(quantity: i64) -> Result<(), InventoryDomainError> {
    if quantity > 0 {
        Ok(())
    } else {
        Err(InventoryDomainError::NonPositiveQuantity)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }
    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }
    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }
    fn item() -> StockItem {
        StockItem::register(
            StockItemId::try_from_uuid(id(10)).unwrap(),
            scope(),
            WarehouseId::try_from_uuid(id(11)).unwrap(),
            "ITEM-1".into(),
            at(9),
        )
        .unwrap()
    }

    #[test]
    fn inventory_requires_exact_scope_and_safe_code() {
        assert!(matches!(
            StockItem::register(
                StockItemId::try_from_uuid(id(10)).unwrap(),
                AuthorityScope::Holding { holding_id: id(1) },
                WarehouseId::try_from_uuid(id(11)).unwrap(),
                "ITEM-1".into(),
                at(9),
            ),
            Err(InventoryDomainError::InvalidScope)
        ));
    }

    #[test]
    fn issue_and_reservation_cannot_consume_unavailable_stock() {
        let mut item = item();
        item.receive(
            StockMovementId::try_from_uuid(id(20)).unwrap(),
            10,
            None,
            at(10),
        )
        .unwrap();
        item.reserve(
            ReservationId::try_from_uuid(id(21)).unwrap(),
            8,
            "SO-1".into(),
            at(11),
        )
        .unwrap();
        assert_eq!(item.available(), 2);
        assert_eq!(
            item.issue(StockMovementId::try_from_uuid(id(22)).unwrap(), 3, at(12),),
            Err(InventoryDomainError::InsufficientAvailable)
        );
    }

    #[test]
    fn reservation_release_restores_available_stock_once() {
        let mut item = item();
        item.receive(
            StockMovementId::try_from_uuid(id(20)).unwrap(),
            10,
            None,
            at(10),
        )
        .unwrap();
        let reservation_id = ReservationId::try_from_uuid(id(21)).unwrap();
        item.reserve(reservation_id, 4, "SO-1".into(), at(11))
            .unwrap();
        item.release_reservation(reservation_id, at(12)).unwrap();
        assert_eq!(item.available(), 10);
        assert_eq!(
            item.release_reservation(reservation_id, at(13)),
            Err(InventoryDomainError::ReservationNotActive)
        );
    }

    #[test]
    fn procurement_reference_is_safe_non_aggregate_correlation() {
        let mut item = item();
        item.receive(
            StockMovementId::try_from_uuid(id(20)).unwrap(),
            2,
            Some(ProcurementLineReference::try_from_uuid(id(99)).unwrap()),
            at(10),
        )
        .unwrap();
        assert_eq!(item.on_hand(), 2);
    }

    #[test]
    fn reservation_fulfillment_consumes_reserved_stock_atomically() {
        let mut item = item();
        item.receive(
            StockMovementId::try_from_uuid(id(30)).unwrap(),
            10,
            None,
            at(10),
        )
        .unwrap();
        let reservation_id = ReservationId::try_from_uuid(id(31)).unwrap();
        item.reserve(reservation_id, 4, "SO-31".into(), at(11))
            .unwrap();
        let version = item.version();
        item.take_uncommitted_events();
        item.fulfill_reservation(
            reservation_id,
            StockMovementId::try_from_uuid(id(32)).unwrap(),
            at(12),
        )
        .unwrap();
        assert_eq!(item.version(), version + 1);
        assert_eq!(
            (item.on_hand(), item.reserved(), item.available()),
            (6, 0, 6)
        );
        assert_eq!(
            item.reservations()[&reservation_id].status,
            ReservationStatus::Released
        );
        assert!(matches!(
            item.take_uncommitted_events().as_slice(),
            [
                DomainEvent::ReservationReleased { .. },
                DomainEvent::StockMoved {
                    quantity_delta: -4,
                    ..
                }
            ]
        ));
        assert_eq!(
            item.fulfill_reservation(
                reservation_id,
                StockMovementId::try_from_uuid(id(33)).unwrap(),
                at(13),
            ),
            Err(InventoryDomainError::ReservationNotActive)
        );
    }
}
