use crate::domain::packages::CommercialCatalogue;
use erpx_server_shared::public::BusinessError;

pub fn current_package_catalogue() -> Result<CommercialCatalogue, BusinessError> {
    CommercialCatalogue::parse_json(include_str!("../../resources/packages.v1.json"))
}
