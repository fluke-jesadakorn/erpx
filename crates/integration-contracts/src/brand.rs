use serde::{Deserialize, Serialize};

use crate::{BoundedContext, DisplayName, ReferenceCode};

owned_uuid_reference!(BrandRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandProfileFact {
    pub brand: BrandRef,
    pub code: ReferenceCode,
    pub display_name: DisplayName,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    BrandProfileChanged(BrandProfileFact),
}

impl_integration_event!(Event, BoundedContext::Brand, {
    Self::BrandProfileChanged(_) => "brand.profile_changed",
});
