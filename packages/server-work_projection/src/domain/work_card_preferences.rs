//! Bounded Work Projection card-preference write facts.
//!
//! The caller owns current membership, the command ledger, Company scope and
//! the catalogue availability set. This module only validates the ordered pin
//! list and applies optimistic versioning.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_PINNED: usize = 6;
pub const MAX_CARD_ID: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkCardPreferences {
    pub pinned_ids: Option<Vec<String>>,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveWorkCardPreferences {
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
}

impl SaveWorkCardPreferences {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            pinned_ids: Option<Vec<String>>,
            expected_version: i64,
        }
        let wire: Wire = serde_json::from_str(value).map_err(|_| invalid())?;
        Ok(Self {
            pinned_ids: wire.pinned_ids,
            expected_version: wire.expected_version,
        })
    }

    pub fn validate(
        &self,
        current: &WorkCardPreferences,
        available: &[String],
    ) -> Result<WorkCardPreferences, BusinessError> {
        if self.expected_version < 0 {
            return Err(invalid());
        }
        if let Some(ids) = &self.pinned_ids {
            let mut seen = std::collections::HashSet::with_capacity(ids.len());
            if ids.len() > MAX_PINNED
                || ids.iter().any(|id| {
                    id.is_empty()
                        || id.len() > MAX_CARD_ID
                        || id.chars().any(char::is_control)
                        || !seen.insert(id.as_str())
                })
            {
                return Err(invalid());
            }
        }
        if current.version != self.expected_version {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "VERSION_CONFLICT",
                "Your shortcuts changed on another device. Review the latest arrangement before saving.",
            ));
        }
        let added = self
            .pinned_ids
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter(|id| {
                !current
                    .pinned_ids
                    .as_deref()
                    .is_some_and(|pins| pins.iter().any(|pin| pin == *id))
            })
            .collect::<Vec<_>>();
        if added
            .iter()
            .any(|id| !available.iter().any(|candidate| candidate == *id))
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "SHORTCUT_UNAVAILABLE",
                "A selected shortcut is unavailable in this workspace. Review your available actions.",
            ));
        }
        Ok(WorkCardPreferences {
            pinned_ids: self.pinned_ids.clone(),
            version: current.version + 1,
        })
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "WORK_CARD_PREFERENCES_INVALID",
        "Choose each shortcut at most once.",
    )
}

pub fn pinned_json(value: Option<&Vec<String>>) -> Result<Option<String>, BusinessError> {
    value
        .map(|ids| serde_json::to_string(ids).map_err(|_| invalid()))
        .transpose()
}

pub fn parse_pinned(value: Option<&str>) -> Result<Option<Vec<String>>, BusinessError> {
    value
        .map(|raw| serde_json::from_str::<Vec<String>>(raw).map_err(|_| invalid()))
        .transpose()
}

pub fn result_wire(value: &WorkCardPreferences) -> Result<Value, BusinessError> {
    serde_json::to_value(value).map_err(|_| invalid())
}

pub fn member_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MEMBERSHIP_REQUIRED",
        "Current workspace membership is required.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available() -> Vec<String> {
        ["tasks", "expenses", "leave"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn validates_order_uniqueness_and_version() {
        let current = WorkCardPreferences {
            pinned_ids: Some(vec!["tasks".into()]),
            version: 2,
        };
        let input = SaveWorkCardPreferences {
            pinned_ids: Some(vec!["tasks".into(), "expenses".into()]),
            expected_version: 2,
        };
        assert_eq!(
            input.validate(&current, &available()).unwrap(),
            WorkCardPreferences {
                pinned_ids: Some(vec!["tasks".into(), "expenses".into()]),
                version: 3,
            }
        );
        assert_eq!(
            SaveWorkCardPreferences {
                pinned_ids: Some(vec!["tasks".into(), "tasks".into()]),
                expected_version: 2,
            }
            .validate(&current, &available())
            .unwrap_err()
            .code,
            "WORK_CARD_PREFERENCES_INVALID"
        );
        assert_eq!(
            SaveWorkCardPreferences {
                pinned_ids: None,
                expected_version: 1,
            }
            .validate(&current, &available())
            .unwrap_err()
            .code,
            "VERSION_CONFLICT"
        );
        assert_eq!(
            SaveWorkCardPreferences {
                pinned_ids: Some(vec!["purchases".into()]),
                expected_version: 2,
            }
            .validate(&current, &available())
            .unwrap_err()
            .code,
            "SHORTCUT_UNAVAILABLE"
        );
    }
}
