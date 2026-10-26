use erpx_server_identity_access::public::*;
use erpx_server_shared::public::{BusinessError, Clock, TimestampMillis};

pub const NOW: i64 = 1_800_000_000_000;
pub fn policy() -> AccountPolicy {
    AccountPolicy {
        account_origin: "https://account.example.test".into(),
        account_host: "account.example.test".into(),
        environment: "development".into(),
        require_email_verification: true,
        holdings: vec![HoldingOrigin {
            origin: "https://acme.example.test".into(),
            host: "acme.example.test".into(),
            audience: "holding:development:h001:acme.example.test".into(),
        }],
    }
}
pub fn empty() -> PresentedSecrets {
    PresentedSecrets {
        browser: None,
        active: None,
        saved: vec![],
    }
}
pub struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}
pub fn assert_error<T>(value: Result<T, BusinessError>, code: &str) {
    match value {
        Err(error) => assert_eq!(error.code, code),
        Ok(_) => panic!("expected {code}"),
    }
}
