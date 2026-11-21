//! Pure validation for the Work home read-cursor write. No IO, no SQL and no
//! authority resolution happen here: the caller supplies bounded scope facts.
use erpx_server_shared::public::BusinessError;

pub const MAX_ID: usize = 200;
pub const MAX_BASE: usize = 120;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkHomeReadInput {
    pub company_id: String,
    pub user_id: String,
    pub workspace_base: String,
    pub read_through: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedMarkHomeRead {
    pub read_through: i64,
}

fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value.chars().any(char::is_control)
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'*')
        })
}

impl MarkHomeReadInput {
    pub fn validate(&self) -> Result<ValidatedMarkHomeRead, BusinessError> {
        if !bounded(&self.company_id, MAX_ID)
            || !bounded(&self.user_id, MAX_ID)
            || !bounded(&self.workspace_base, MAX_BASE)
            || self.read_through <= 0
        {
            return Err(BusinessError::invalid(
                "INVALID_HOME_READ_CURSOR",
                "Provide a bounded home read cursor.",
            ));
        }
        Ok(ValidatedMarkHomeRead {
            read_through: self.read_through,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> MarkHomeReadInput {
        MarkHomeReadInput {
            company_id: "company-1".into(),
            user_id: "user-1".into(),
            workspace_base: "workplace:work".into(),
            read_through: 1_700_000_000_000,
        }
    }

    #[test]
    fn accepts_bounded_cursor() {
        assert_eq!(
            input().validate().unwrap(),
            ValidatedMarkHomeRead {
                read_through: 1_700_000_000_000
            }
        );
    }

    #[test]
    fn rejects_empty_scope_nonpositive_time_and_control_chars() {
        let mut bad = input();
        bad.user_id.clear();
        assert!(bad.validate().is_err());
        let mut bad = input();
        bad.read_through = 0;
        assert!(bad.validate().is_err());
        let mut bad = input();
        bad.workspace_base = "bad\nbase".into();
        assert!(bad.validate().is_err());
    }
}
