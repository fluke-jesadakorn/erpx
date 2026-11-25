//! Query port and handler for lane 10's account invitation reads.
//!
//! No SQL lives here. The handler validates the registered read kind, binds the
//! Company from the transport header and dispatches to the owner reader.

use erpx_server_shared::public::BusinessError;
use serde::Deserialize;
use std::{future::Future, pin::Pin};

use super::domain::{
    AccountAccessReadKind, AccountAccessReadResult, invalid, valid_email, valid_id, valid_now,
    valid_token_hash,
};

pub type AccountAccessReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the Control-backed owner reader behind this port.
pub trait AccountAccessRead: Send + Sync {
    fn email_exists<'a>(&'a self, email: &'a str) -> AccountAccessReadFuture<'a, bool>;
    fn invitation_status<'a>(
        &'a self,
        holding_id: &'a str,
        company_id: &'a str,
        person_id: &'a str,
        invitation_id: &'a str,
    ) -> AccountAccessReadFuture<'a, Option<String>>;
    fn invitation_by_token<'a>(
        &'a self,
        token_hash: &'a str,
        now: i64,
    ) -> AccountAccessReadFuture<'a, Option<super::domain::Invitation>>;
}

/// Closed wire request. Unknown kinds and extra fields are rejected before any
/// reader is called.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountAccessReadRequest {
    EmailExists { email: String },
    InvitationStatus {
        holding_id: String,
        person_id: String,
        invitation_id: String,
    },
    InvitationByToken { token_hash: String, now: i64 },
}

impl AccountAccessReadRequest {
    pub fn kind(&self) -> AccountAccessReadKind {
        match self {
            Self::EmailExists { .. } => AccountAccessReadKind::EmailExists,
            Self::InvitationStatus { .. } => AccountAccessReadKind::InvitationStatus,
            Self::InvitationByToken { .. } => AccountAccessReadKind::InvitationByToken,
        }
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        match self {
            Self::EmailExists { email } if valid_email(email) => Ok(()),
            Self::InvitationStatus { holding_id, person_id, invitation_id }
                if valid_id(holding_id) && valid_id(person_id) && valid_id(invitation_id) =>
            {
                Ok(())
            }
            Self::InvitationByToken { token_hash, now }
                if valid_token_hash(token_hash) && valid_now(*now) =>
            {
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
}

/// Run one validated read. `company_id` is always the transport header; no
/// caller-supplied Company value can widen the invitation status scope.
pub async fn execute(
    reader: &dyn AccountAccessRead,
    company_id: &str,
    request: AccountAccessReadRequest,
) -> Result<AccountAccessReadResult, BusinessError> {
    if !valid_id(company_id) {
        return Err(invalid());
    }
    request.validate()?;
    match request {
        AccountAccessReadRequest::EmailExists { email } => reader
            .email_exists(&email)
            .await
            .map(AccountAccessReadResult::Boolean),
        AccountAccessReadRequest::InvitationStatus {
            holding_id,
            person_id,
            invitation_id,
        } => reader
            .invitation_status(&holding_id, company_id, &person_id, &invitation_id)
            .await
            .map(AccountAccessReadResult::Status),
        AccountAccessReadRequest::InvitationByToken { token_hash, now } => reader
            .invitation_by_token(&token_hash, now)
            .await
            .map(AccountAccessReadResult::Invitation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_10::domain::{KIND_EMAIL_EXISTS, KIND_INVITATION_STATUS, KIND_INVITATION_BY_TOKEN};
    use std::sync::Mutex;

    struct Probe {
        seen: Mutex<Vec<String>>,
    }

    impl AccountAccessRead for Probe {
        fn email_exists<'a>(&'a self, email: &'a str) -> AccountAccessReadFuture<'a, bool> {
            self.seen.lock().unwrap().push(format!("email:{email}"));
            Box::pin(async { Ok(true) })
        }
        fn invitation_status<'a>(
            &'a self,
            holding_id: &'a str,
            company_id: &'a str,
            person_id: &'a str,
            invitation_id: &'a str,
        ) -> AccountAccessReadFuture<'a, Option<String>> {
            self.seen.lock().unwrap().push(format!("status:{holding_id}:{company_id}:{person_id}:{invitation_id}"));
            Box::pin(async { Ok(Some("pending".into())) })
        }
        fn invitation_by_token<'a>(
            &'a self,
            token_hash: &'a str,
            now: i64,
        ) -> AccountAccessReadFuture<'a, Option<super::super::domain::Invitation>> {
            self.seen.lock().unwrap().push(format!("token:{token_hash}:{now}"));
            Box::pin(async { Ok(None) })
        }
    }

    #[tokio::test]
    async fn dispatch_uses_transport_company_and_closed_kinds() {
        let probe = Probe { seen: Mutex::new(Vec::new()) };
        let email = execute(&probe, "c1", AccountAccessReadRequest::EmailExists { email: "a@example.test".into() })
            .await
            .expect("email read");
        assert_eq!(email, AccountAccessReadResult::Boolean(true));
        let status = execute(
            &probe,
            "c1",
            AccountAccessReadRequest::InvitationStatus {
                holding_id: "h1".into(),
                person_id: "p1".into(),
                invitation_id: "i1".into(),
            },
        )
        .await
        .expect("status read");
        assert_eq!(status, AccountAccessReadResult::Status(Some("pending".into())));
        assert!(probe.seen.lock().unwrap().contains(&"status:h1:c1:p1:i1".to_owned()));
        let token = execute(
            &probe,
            "c1",
            AccountAccessReadRequest::InvitationByToken {
                token_hash: "a".repeat(64),
                now: 1_700_000_000_000,
            },
        )
        .await
        .expect("token read");
        assert_eq!(token, AccountAccessReadResult::Invitation(None));
    }

    #[tokio::test]
    async fn invalid_requests_never_reach_the_reader() {
        let probe = Probe { seen: Mutex::new(Vec::new()) };
        let bad = execute(&probe, "c1", AccountAccessReadRequest::EmailExists { email: "nope".into() }).await;
        assert_eq!(bad, Err(invalid()));
        let bad_company = execute(
            &probe,
            "not a company",
            AccountAccessReadRequest::InvitationStatus {
                holding_id: "h1".into(),
                person_id: "p1".into(),
                invitation_id: "i1".into(),
            },
        )
        .await;
        assert_eq!(bad_company, Err(invalid()));
        assert!(probe.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn wire_tags_are_the_registered_kinds() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"kind":"email_exists","email":"a@example.test"}"#,
        )
        .expect("json");
        let request: AccountAccessReadRequest = serde_json::from_value(value).expect("request");
        assert_eq!(request.kind().as_str(), KIND_EMAIL_EXISTS);
        let value: serde_json::Value =
            serde_json::from_str(r#"{"kind":"invitation_status","holding_id":"h1","person_id":"p1","invitation_id":"i1"}"#)
                .expect("json");
        let request: AccountAccessReadRequest = serde_json::from_value(value).expect("request");
        assert_eq!(request.kind().as_str(), KIND_INVITATION_STATUS);
        let value: serde_json::Value =
            serde_json::from_str(r#"{"kind":"invitation_by_token","token_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","now":1700000000000}"#)
                .expect("json");
        let request: AccountAccessReadRequest = serde_json::from_value(value).expect("request");
        assert_eq!(request.kind().as_str(), KIND_INVITATION_BY_TOKEN);
        assert!(serde_json::from_str::<AccountAccessReadRequest>(
            r#"{"kind":"drop_table","email":"a@example.test"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<AccountAccessReadRequest>(
            r#"{"kind":"email_exists","email":"a@example.test","sql":"SELECT 1"}"#
        )
        .is_err());
    }
}
