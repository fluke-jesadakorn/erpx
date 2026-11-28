//! Query port and owner handler for the native Legal processing-status read.
//!
//! No SQL lives here. The handler preserves the retired order: the Company-scoped
//! point read decides existence first, then the composition-supplied item-access
//! authority is enforced. Purchase and role decisions are never granted by a
//! request path, query string or broad role; composition supplies the witness
//! from the authoritative Legal record-access check.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::domain::ProcessingStatusRead;

pub type ProcessingStatusFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition-supplied authority for one processing-status read. `None` never
/// happens: the HTTP adapter fails closed when the header is absent or malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessingStatusAuthority {
    /// The viewer may access the processing job's Legal item.
    pub item_accessible: bool,
}

/// The owner-local fact behind one processing-status read. Composition injects
/// the Pg implementation; there is no permissive or default reader.
pub trait ProcessingStatusSource: Send + Sync {
    /// Company-scoped point read of one processing job.
    fn status<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> ProcessingStatusFuture<'a, Option<ProcessingStatusRead>>;
}

fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_PROCESSING_FORBIDDEN",
        "The processing job is unavailable.",
    )
}

/// Owner handler. Existence is decided before the access gate so a missing job
/// still returns the retired `LEGAL_NOT_FOUND` rather than an authorization error.
/// Every fact comes from the same injected source, so a reconfigured source
/// cannot mix Company scopes.
pub async fn read_processing_status(
    source: &dyn ProcessingStatusSource,
    authority: ProcessingStatusAuthority,
    company: &str,
    id: &str,
) -> Result<ProcessingStatusRead, BusinessError> {
    let read = source
        .status(company, id)
        .await?
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "LEGAL_NOT_FOUND", "Processing job unavailable."))?;
    if !authority.item_accessible {
        return Err(denied());
    }
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        read: Option<ProcessingStatusRead>,
    }

    impl ProcessingStatusSource for Fake {
        fn status<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> ProcessingStatusFuture<'a, Option<ProcessingStatusRead>> {
            let calls = &self.calls;
            let read = &self.read;
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                calls.lock().unwrap().push(format!("status:{company}:{id}"));
                Ok(read.clone())
            })
        }
    }

    fn read() -> ProcessingStatusRead {
        ProcessingStatusRead {
            id: "job-1".into(),
            item_id: "item-1".into(),
            document_id: "doc-1".into(),
            state: "queued".into(),
            error: None,
            attempts: 1,
            job_id: "job-1".into(),
            result: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn handler_binds_company_scope_and_returns_complete_payload() {
        let fake = Fake {
            read: Some(read()),
            ..Default::default()
        };
        let value = read_processing_status(
            &fake,
            ProcessingStatusAuthority {
                item_accessible: true,
            },
            "company-1",
            "job-1",
        )
        .await
        .expect("read");
        assert_eq!(value.id, "job-1");
        assert_eq!(value.job_id, "job-1");
        assert_eq!(value.result, serde_json::Value::Null);
        assert_eq!(
            fake.calls.lock().unwrap().as_slice(),
            ["status:company-1:job-1"]
        );
    }

    #[tokio::test]
    async fn missing_job_is_not_found_before_the_access_gate() {
        let fake = Fake::default();
        let error = read_processing_status(
            &fake,
            ProcessingStatusAuthority {
                item_accessible: false,
            },
            "company-1",
            "missing",
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "LEGAL_NOT_FOUND");
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(
            fake.calls.lock().unwrap().as_slice(),
            ["status:company-1:missing"]
        );
    }

    #[tokio::test]
    async fn inaccessible_item_is_forbidden_after_the_point_read() {
        let fake = Fake {
            read: Some(read()),
            ..Default::default()
        };
        let error = read_processing_status(
            &fake,
            ProcessingStatusAuthority {
                item_accessible: false,
            },
            "company-1",
            "job-1",
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "LEGAL_PROCESSING_FORBIDDEN");
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(
            fake.calls.lock().unwrap().as_slice(),
            ["status:company-1:job-1"]
        );
    }
}
