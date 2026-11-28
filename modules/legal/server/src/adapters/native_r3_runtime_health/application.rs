//! Query port and owner handler for the native Legal runtime-health read.
//!
//! No SQL lives here. The retired order is preserved: the permission gate runs
//! first, then the fixed task table and the Company capability probe are read
//! from one injected source, and only then is each heartbeat marked stale.
//! Role, Company and predicate facts are supplied by composition; a request
//! never grants them.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::{BusinessError, Clock, FailureKind};

use super::domain::{
    RuntimeHealthRead, RuntimeHealthTask, RuntimeHealthTaskRow, RUNTIME_HEALTH_NOTICE,
    STALE_AFTER_MILLIS, valid_feature_predicate,
};

pub type RuntimeHealthFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition-supplied authority for one runtime-health read. `None` never
/// happens: the HTTP adapter fails closed when the header is absent or malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeHealthAuthority {
    /// The viewer holds `legal.manage`, the exact retired permission.
    pub can_manage: bool,
}

/// The owner-local facts behind one runtime-health read. Composition injects
/// the Pg implementation; there is no permissive or default reader.
pub trait RuntimeHealthSource: Send + Sync {
    /// Fixed `legal_runtime_health` table read, ordered by task.
    fn tasks<'a>(&'a self) -> RuntimeHealthFuture<'a, Vec<RuntimeHealthTaskRow>>;
    /// Company capability probe with the code-owned scheduled-feature predicate.
    fn company_ready<'a>(
        &'a self,
        company: &'a str,
        feature_predicate: &'a str,
    ) -> RuntimeHealthFuture<'a, bool>;
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LEGAL_RUNTIME_HEALTH_REQUEST_INVALID",
        "Use one bounded Legal runtime-health read.",
    )
}

/// Owner handler. Every fact comes from the same injected source, so a
/// reconfigured source cannot mix Company scopes or predicates.
pub async fn read_runtime_health(
    source: &dyn RuntimeHealthSource,
    authority: RuntimeHealthAuthority,
    company: &str,
    feature_predicate: &str,
    clock: &impl Clock,
) -> Result<RuntimeHealthRead, BusinessError> {
    if !authority.can_manage {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    if company.is_empty() || !valid_feature_predicate(feature_predicate) {
        return Err(invalid());
    }
    let rows = source.tasks().await?;
    let company_ready = source.company_ready(company, feature_predicate).await?;
    let now = clock.now().get();
    let tasks = rows
        .into_iter()
        .map(|row| {
            let stale = now.saturating_sub(row.heartbeat_at) > STALE_AFTER_MILLIS;
            RuntimeHealthTask {
                task: row.task,
                heartbeat_at: row.heartbeat_at,
                last_success_at: row.last_success_at,
                failure_count: row.failure_count,
                last_error: row.last_error,
                stale,
            }
        })
        .collect();
    Ok(RuntimeHealthRead {
        company_ready,
        tasks,
        notice: RUNTIME_HEALTH_NOTICE.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_shared_server::public::TimestampMillis;
    use std::sync::Mutex;

    struct Fixed(i64);
    impl Clock for Fixed {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(self.0)
        }
    }

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        ready: bool,
        rows: Vec<RuntimeHealthTaskRow>,
    }

    impl RuntimeHealthSource for Fake {
        fn tasks<'a>(&'a self) -> RuntimeHealthFuture<'a, Vec<RuntimeHealthTaskRow>> {
            self.calls.lock().unwrap().push("tasks".to_owned());
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
        fn company_ready<'a>(
            &'a self,
            company: &'a str,
            predicate: &'a str,
        ) -> RuntimeHealthFuture<'a, bool> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("ready:{company}:{predicate}"));
            let ready = self.ready;
            Box::pin(async move { Ok(ready) })
        }
    }

    fn row(task: &str, heartbeat_at: i64) -> RuntimeHealthTaskRow {
        RuntimeHealthTaskRow {
            task: task.to_owned(),
            heartbeat_at,
            last_success_at: None,
            failure_count: 2,
            last_error: Some("boom".to_owned()),
        }
    }

    #[tokio::test]
    async fn handler_binds_company_and_marks_only_old_heartbeats_stale() {
        let fake = Fake {
            ready: true,
            rows: vec![row("fresh", 1_000_000), row("old", 0)],
            ..Default::default()
        };
        let read = read_runtime_health(
            &fake,
            RuntimeHealthAuthority { can_manage: true },
            "company-1",
            "TRUE",
            &Fixed(1_000_000 + STALE_AFTER_MILLIS),
        )
        .await
        .expect("read");
        assert!(read.company_ready);
        assert_eq!(read.tasks.len(), 2);
        assert!(!read.tasks[0].stale);
        assert!(read.tasks[1].stale);
        assert_eq!(read.tasks[0].failure_count, 2);
        assert_eq!(read.tasks[0].last_error.as_deref(), Some("boom"));
        assert_eq!(read.notice, RUNTIME_HEALTH_NOTICE);
        assert_eq!(
            fake.calls.lock().unwrap().as_slice(),
            ["tasks", "ready:company-1:TRUE"]
        );
    }

    #[tokio::test]
    async fn permission_and_predicate_gates_never_reach_the_source() {
        for (authority, company, predicate, expected) in [
            (
                RuntimeHealthAuthority { can_manage: false },
                "company-1",
                "TRUE",
                FailureKind::Forbidden,
            ),
            (
                RuntimeHealthAuthority { can_manage: true },
                "",
                "TRUE",
                FailureKind::InvalidInput,
            ),
            (
                RuntimeHealthAuthority { can_manage: true },
                "company-1",
                "TRUE; DROP TABLE legal_items",
                FailureKind::InvalidInput,
            ),
        ] {
            let fake = Fake::default();
            let error = read_runtime_health(&fake, authority, company, predicate, &Fixed(0))
                .await
                .unwrap_err();
            assert_eq!(error.kind, expected);
            assert!(fake.calls.lock().unwrap().is_empty());
        }
    }
}
