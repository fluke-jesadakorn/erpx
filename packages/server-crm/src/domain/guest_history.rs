//! Pure domain facts for the native CRM contact guest-history read.
//!
//! This is the owner slice behind `GET /api/v1/holdings/{holdingSlug}/crm`
//! with `view=guest-history&id=<contactId>` (`crmContactHistory`). It is a read
//! only: it never writes a guest link, stay or case event and does not touch
//! the already-migrated `crm_case_events` history slice. Composition retains
//! the Account session, selected Company/branch/department scope, `sales.crm`
//! purchase admission, retained `pms.reservations` history and the serializable
//! owner unit; this module enforces the actor read policy and every owner row
//! before presentation.
//!
//! The serialized view is byte-for-byte the retained TypeScript
//! `createCRMIdentityApplication(...).history(id)` payload: `contact`
//! (`id`, `name`, `account_id`), `stays` (the first 200 guest-link
//! reservation/stay rows) and `hasMore` (a camelCase field).
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::Serialize;

const ID_LIMIT: usize = 200;
const TEXT_LIMIT: usize = 2_000;
const STAYS_LIMIT: usize = 200;
const FETCH_LIMIT: usize = STAYS_LIMIT + 1;

/// Current selected work context and composition-supplied purchase facts of
/// the reader. The domain never mints a user, Company, permission, module or
/// purchase entitlement.
#[derive(Clone, Debug)]
pub struct GuestHistoryActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    /// `sales.crm` history admission supplied by composition.
    pub crm_purchased: bool,
    /// Retained `pms.reservations` history admission supplied by composition.
    pub hotel_history: bool,
    pub expires_at: i64,
}

impl GuestHistoryActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self, module: &str) -> bool {
        self.modules.is_empty() || self.modules.iter().any(|value| value == module)
    }

    /// A guest-history read requires `commerce.read` and `pms.read`, the CRM
    /// module, a live work context, purchased CRM history and retained Hotel
    /// reservation history. Fails closed on any missing fact.
    pub fn require_guest_history(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("commerce.read") || !self.has("pms.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_PERMISSION_REQUIRED",
                "Your selected role does not grant CRM guest-history read access.",
            ));
        }
        if !self.has_module("crm") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "CRM_MODULE_FORBIDDEN",
                "Your current role cannot access the CRM pipeline.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if !self.crm_purchased {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "Sales CRM or retained history is required.",
            ));
        }
        if !self.hotel_history {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PACKAGE_REQUIRED",
                "Retained Hotel reservation history is required.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_GUEST_HISTORY_REQUEST_INVALID",
        "Use one bounded CRM contact guest-history request.",
    )
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_GUEST_HISTORY_SOURCE_INVALID",
        "The current CRM guest-history source is invalid.",
    )
}

pub(crate) fn contact_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_CONTACT_NOT_FOUND",
        "Contact not found.",
    )
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.encode_utf16().count() <= ID_LIMIT
        && !value.chars().any(char::is_control)
}

fn valid_text(value: Option<&str>) -> bool {
    value.is_none_or(|text| {
        !text.chars().any(char::is_control) && text.encode_utf16().count() <= TEXT_LIMIT
    })
}

/// One scoped `crm_contacts` row joined to its owning Account. Field names and
/// JSON types match the retired TypeScript `contact(id)` projection exactly.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuestContactRow {
    pub id: String,
    pub name: String,
    pub account_id: String,
}

impl GuestContactRow {
    fn validate(&self, contact_id: &str) -> Result<(), BusinessError> {
        if self.id != contact_id
            || !valid_id(&self.id)
            || !valid_id(&self.account_id)
            || !valid_text(Some(&self.name))
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// One granted guest-link reservation/stay row from the CRM owner's
/// `crm_guest_links` joined to the retained Hotel property and record
/// projections. JSONB `->>'...'` values stay nullable strings, as in the
/// retired TypeScript driver.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuestStayRow {
    pub guest_id: String,
    pub reservation_id: String,
    pub reservation_state: Option<String>,
    pub stay_id: Option<String>,
    pub stay_state: Option<String>,
    pub arrival: Option<String>,
    pub departure: Option<String>,
    pub property_id: String,
}

impl GuestStayRow {
    fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.guest_id)
            || !valid_id(&self.reservation_id)
            || !valid_id(&self.property_id)
            || !valid_text(self.stay_id.as_deref())
            || !valid_text(self.reservation_state.as_deref())
            || !valid_text(self.stay_state.as_deref())
            || !valid_text(self.arrival.as_deref())
            || !valid_text(self.departure.as_deref())
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

/// Raw owner source, read in one caller transaction before validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GuestHistorySource {
    pub contact: Option<GuestContactRow>,
    pub stays: Vec<GuestStayRow>,
}

/// The complete native payload. `hasMore` is intentionally camelCase: the
/// retained TypeScript application returned that exact key.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuestHistoryView {
    pub contact: GuestContactRow,
    pub stays: Vec<GuestStayRow>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

/// Validate the requested contact and the complete owner row source, then
/// apply the TypeScript `rows.slice(0,200)` / `rows.length>200` presentation.
pub fn validate_guest_history(
    contact_id: &str,
    source: GuestHistorySource,
) -> Result<GuestHistoryView, BusinessError> {
    if !valid_id(contact_id) {
        return Err(request_invalid());
    }
    if source.stays.len() > FETCH_LIMIT {
        return Err(source_invalid());
    }
    let GuestHistorySource { contact, stays } = source;
    let contact = contact.ok_or_else(contact_not_found)?;
    contact.validate(contact_id)?;
    for stay in &stays {
        stay.validate()?;
    }
    let has_more = stays.len() > STAYS_LIMIT;
    let stays = stays.into_iter().take(STAYS_LIMIT).collect();
    Ok(GuestHistoryView {
        contact,
        stays,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> GuestHistoryActor {
        GuestHistoryActor {
            user_id: "user-1".to_owned(),
            company_id: CompanyId::new("company").expect("company"),
            branch_id: Some("branch-1".to_owned()),
            department_id: Some("dept-1".to_owned()),
            permissions: vec![
                "commerce.read".to_owned(),
                "pms.read".to_owned(),
            ],
            modules: vec!["crm".to_owned()],
            crm_purchased: true,
            hotel_history: true,
            expires_at: 500,
        }
    }

    fn contact() -> GuestContactRow {
        GuestContactRow {
            id: "contact-1".to_owned(),
            name: "Guest Name".to_owned(),
            account_id: "account-1".to_owned(),
        }
    }

    fn stay(n: usize) -> GuestStayRow {
        GuestStayRow {
            guest_id: format!("guest-{n}"),
            reservation_id: format!("reservation-{n}"),
            reservation_state: Some("confirmed".to_owned()),
            stay_id: Some(format!("stay-{n}")),
            stay_state: Some("checked_out".to_owned()),
            arrival: Some("2026-11-01".to_owned()),
            departure: Some("2026-11-03".to_owned()),
            property_id: "property-1".to_owned(),
        }
    }

    #[test]
    fn history_read_requires_permissions_module_context_and_both_entitlements() {
        let now = TimestampMillis::new(100);
        assert!(actor().require_guest_history(now).is_ok());

        let mut missing = actor();
        missing.permissions = vec!["commerce.read".to_owned()];
        assert_eq!(
            missing.require_guest_history(now).unwrap_err().code,
            "CRM_PERMISSION_REQUIRED"
        );

        let mut module = actor();
        module.modules = vec!["marketing".to_owned()];
        assert_eq!(
            module.require_guest_history(now).unwrap_err().code,
            "CRM_MODULE_FORBIDDEN"
        );

        assert_eq!(
            actor().require_guest_history(TimestampMillis::new(501)).unwrap_err().code,
            "WORK_CONTEXT_CHANGED"
        );

        let mut crm = actor();
        crm.crm_purchased = false;
        assert_eq!(
            crm.require_guest_history(now).unwrap_err().code,
            "PACKAGE_REQUIRED"
        );

        let mut hotel = actor();
        hotel.hotel_history = false;
        assert_eq!(
            hotel.require_guest_history(now).unwrap_err().code,
            "PACKAGE_REQUIRED"
        );
    }

    #[test]
    fn validation_preserves_the_retired_contact_stays_has_more_payload() {
        let mut source = GuestHistorySource {
            contact: Some(contact()),
            stays: (0..201).map(stay).collect(),
        };
        let view = validate_guest_history("contact-1", source.clone()).expect("view");
        assert_eq!(view.contact, contact());
        assert_eq!(view.stays.len(), 200);
        assert!(view.has_more);

        source.stays.pop();
        let view = validate_guest_history("contact-1", source).expect("view");
        assert_eq!(view.stays.len(), 200);
        assert!(!view.has_more);
    }

    #[test]
    fn validation_rejects_missing_contact_out_of_scope_and_malformed_rows() {
        assert_eq!(
            validate_guest_history("contact-1", GuestHistorySource::default())
                .unwrap_err()
                .code,
            "CRM_CONTACT_NOT_FOUND"
        );
        assert_eq!(
            validate_guest_history("", GuestHistorySource::default())
                .unwrap_err()
                .code,
            "CRM_GUEST_HISTORY_REQUEST_INVALID"
        );
        let mut wrong = contact();
        wrong.id = "other".to_owned();
        assert_eq!(
            validate_guest_history(
                "contact-1",
                GuestHistorySource {
                    contact: Some(wrong),
                    stays: Vec::new(),
                }
            )
            .unwrap_err()
            .code,
            "CRM_GUEST_HISTORY_SOURCE_INVALID"
        );
        let mut too_many = GuestHistorySource {
            contact: Some(contact()),
            stays: (0..202).map(stay).collect(),
        };
        assert_eq!(
            validate_guest_history("contact-1", too_many.clone())
                .unwrap_err()
                .code,
            "CRM_GUEST_HISTORY_SOURCE_INVALID"
        );
        too_many.stays.pop();
        let mut bad = stay(0);
        bad.guest_id = String::new();
        too_many.stays[0] = bad;
        assert_eq!(
            validate_guest_history("contact-1", too_many)
                .unwrap_err()
                .code,
            "CRM_GUEST_HISTORY_SOURCE_INVALID"
        );
    }

    #[test]
    fn serialized_view_uses_the_exact_typescript_field_names() {
        let view = validate_guest_history(
            "contact-1",
            GuestHistorySource {
                contact: Some(contact()),
                stays: vec![stay(1)],
            },
        )
        .expect("view");
        let value = serde_json::to_value(view).expect("serialize");
        let object = value.as_object().expect("object");
        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(fields, vec!["contact", "hasMore", "stays"]);
        assert_eq!(value["contact"]["account_id"], "account-1");
        assert_eq!(value["stays"][0]["reservation_state"], "confirmed");
        assert_eq!(value["hasMore"], false);
    }
}
