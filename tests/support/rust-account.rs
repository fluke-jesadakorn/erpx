use erpx_server_identity_access::public::*;
use erpx_server_shared::public::{BusinessError, Clock, TimestampMillis};

pub const NOW: i64 = 1_800_000_000_000;
pub fn policy() -> AccountPolicy {
    AccountPolicy {
        account_origin: "https://account.example.test".into(),
        account_host: "account.example.test".into(),
        environment: "development".into(),
        require_email_verification: true,
        holdings: vec![
            HoldingOrigin {
                origin: "https://acme.example.test".into(),
                host: "acme.example.test".into(),
                audience: "holding:development:h001:acme.example.test".into(),
            },
            HoldingOrigin {
                origin: "https://northstar.example.test".into(),
                host: "northstar.example.test".into(),
                audience: "holding:development:h002:northstar.example.test".into(),
            },
        ],
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

/// Explicit recording double for Control-only adapter cases. It is never a
/// production constructor/default and does not prove real Holding persistence;
/// the separate revocation PostgreSQL suite supplies the actual native adapter.
#[allow(dead_code)]
#[derive(Default)]
pub struct RecordedFixtureRevocations(
    std::sync::Mutex<Vec<account_revocation::RetiredAccountChild>>,
);
impl account_revocation::AccountWorkContextInvalidation for RecordedFixtureRevocations {
    fn invalidate<'a>(
        &'a self,
        _targets: &'a [account_revocation::AccountRevocationTarget],
        children: &'a [account_revocation::RetiredAccountChild],
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            assert!(!children.is_empty());
            assert!(
                children
                    .iter()
                    .all(|child| child.environment == "development")
            );
            self.0.lock().unwrap().extend_from_slice(children);
            Ok(())
        })
    }
}
