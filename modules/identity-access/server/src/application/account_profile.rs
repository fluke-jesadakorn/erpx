//! Application slice for the native Account profile read.
//!
//! One bounded read query, one owner port and one validation step. No SQL, no
//! HTTP, no transaction opening: the adapter owns persistence and the caller
//! owns the request boundary.

use crate::domain::account_profile::{
    AccountProfile, ProfileRecord, project_profile, valid_user_id,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type AccountProfileFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port. The Pg adapter executes the exact Control read; tests inject a
/// closed probe. The returned record is still raw so the pure projection stays
/// in the application layer.
pub trait AccountProfileReader: Send + Sync {
    fn read<'a>(&'a self, user_id: &'a str) -> AccountProfileFuture<'a, Option<ProfileRecord>>;
}

pub struct ProfileQuery {
    pub user_id: String,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_PROFILE_REQUEST_INVALID",
        "Use one bounded account profile read.",
    )
}

/// Read-only handler. Rejects unbounded identity headers before any port call,
/// then projects the raw row into the byte-compatible owner payload.
pub async fn read_profile(
    reader: &dyn AccountProfileReader,
    query: ProfileQuery,
) -> Result<Option<AccountProfile>, BusinessError> {
    if !valid_user_id(&query.user_id) {
        return Err(invalid());
    }
    let record = reader.read(&query.user_id).await?;
    Ok(record.map(project_profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Probe {
        record: Option<ProfileRecord>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl AccountProfileReader for Probe {
        fn read<'a>(&'a self, user_id: &'a str) -> AccountProfileFuture<'a, Option<ProfileRecord>> {
            let seen = self.seen.clone();
            let record = self.record.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(user_id.to_owned());
                Ok(record)
            })
        }
    }

    #[tokio::test]
    async fn applies_defaults_and_forwards_bounded_identity() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe {
            record: Some(ProfileRecord {
                name: "Ada".into(),
                email: "ada@example.test".into(),
                language: None,
                appearance: None,
                version: None,
            }),
            seen: seen.clone(),
        };
        let profile = read_profile(
            &probe,
            ProfileQuery {
                user_id: "user-1".into(),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(profile.preferences.version, 0);
        assert_eq!(profile.preferences.language, "en");
        assert_eq!(seen.lock().unwrap().as_slice(), &["user-1".to_owned()]);
    }

    #[tokio::test]
    async fn rejects_unbounded_identity_without_touching_port() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe {
            record: None,
            seen: seen.clone(),
        };
        let error = read_profile(
            &probe,
            ProfileQuery {
                user_id: "../admin".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "ACCOUNT_PROFILE_REQUEST_INVALID");
        assert!(seen.lock().unwrap().is_empty());
    }
}
