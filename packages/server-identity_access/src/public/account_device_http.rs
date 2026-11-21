use super::account_contracts::DeviceAccountsDto;
use serde::Serialize;

/// The existing Account device facade's exact display-only envelope.
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct DeviceAccountsResponse {
    pub data: DeviceAccountsDto,
}
