//! The caller binds the Records employee-request witness to its retained
//! unit. There is no commit, authorization or cross-owner read here.
use crate::domain::employee_request_source::{
    EmployeeRequestRow, EmployeeRequestScope, require_quotation_source, validate_id, validate_row,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub trait EmployeeRequestSourcePort: Send {
    type Error: From<BusinessError> + Send;
    type Witness: Send;
    /// Read the retained guarded row in the caller's unit. The witness
    /// must capture the exact guard tuple so `require_current` can
    /// revalidate it after deferred effects.
    fn read_request(
        &mut self,
        scope: &EmployeeRequestScope,
        id: &str,
    ) -> impl Future<Output = Result<(EmployeeRequestRow, Self::Witness), Self::Error>> + Send;
    /// Revalidate the same witness, after deferred effects and before
    /// commit.
    fn require_current(
        &mut self,
        witness: &Self::Witness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

#[derive(Debug)]
pub struct PreparedEmployeeRequestSource<W> {
    pub request: EmployeeRequestRow,
    witness: W,
}

impl<W> PreparedEmployeeRequestSource<W> {
    pub fn into_parts(self) -> (EmployeeRequestRow, W) {
        (self.request, self.witness)
    }

    pub async fn require_current<P: EmployeeRequestSourcePort<Witness = W>>(
        &self,
        port: &mut P,
    ) -> Result<(), P::Error> {
        port.require_current(&self.witness).await
    }
}

pub async fn prepare_employee_request_source<P: EmployeeRequestSourcePort>(
    port: &mut P,
    scope: &EmployeeRequestScope,
    id: &str,
) -> Result<PreparedEmployeeRequestSource<P::Witness>, P::Error> {
    validate_id(id)?;
    // The port must only return rows within the retained selection; the
    // domain check below is the authority, so a port that returns a
    // foreign row is rejected rather than trusted.
    let (row, witness) = port.read_request(scope, id).await?;
    validate_row(&row, scope)?;
    Ok(PreparedEmployeeRequestSource {
        request: row,
        witness,
    })
}

pub async fn prepare_quotation_source<P: EmployeeRequestSourcePort>(
    port: &mut P,
    scope: &EmployeeRequestScope,
    actor_user_id: &str,
    id: &str,
) -> Result<PreparedEmployeeRequestSource<P::Witness>, P::Error> {
    let prepared = prepare_employee_request_source(port, scope, id).await?;
    require_quotation_source(&prepared.request, actor_user_id)?;
    Ok(prepared)
}

pub async fn prepare_order_handoff_source<P: EmployeeRequestSourcePort>(
    port: &mut P,
    scope: &EmployeeRequestScope,
    actor_user_id: &str,
    id: &str,
    require_assigned: bool,
) -> Result<PreparedEmployeeRequestSource<P::Witness>, P::Error> {
    let prepared = prepare_employee_request_source(port, scope, id).await?;
    if require_assigned {
        require_quotation_source(&prepared.request, actor_user_id)?;
    } else {
        crate::domain::employee_request_source::require_approved_purchase(&prepared.request)?;
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::employee_request_source::{
        EmployeeRequestGuard, guard_for, require_current,
    };
    use erpx_server_shared::public::{BusinessError, FailureKind};

    fn missing<E: From<BusinessError>>() -> E {
        BusinessError::new(
            FailureKind::NotFound,
            "REQUEST_NOT_FOUND",
            "Request not found.",
        )
        .into()
    }
    use std::pin::Pin;

    fn test_row() -> EmployeeRequestRow {
        EmployeeRequestRow {
            id: "request-1".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            kind: "purchase".into(),
            state: "in_progress".into(),
            version: 2,
            evidence_version: 1,
            items_json: "[]".into(),
            approval_json: "{\"decision\":\"approved\"}".into(),
            title: "Cables".into(),
            currency: "THB".into(),
            requester_user_id: "requester".into(),
            created_by: "creator".into(),
            assignee_user_id: Some("handler".into()),
            purchase_order_id: None,
        }
    }

    fn test_scope() -> EmployeeRequestScope {
        EmployeeRequestScope {
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
        }
    }

    struct FakePort {
        row: Option<EmployeeRequestRow>,
        current: Option<EmployeeRequestRow>,
        read_calls: usize,
    }

    impl EmployeeRequestSourcePort for FakePort {
        type Error = BusinessError;
        type Witness = EmployeeRequestGuard;

        #[allow(refining_impl_trait)]
        fn read_request(
            &mut self,
            _scope: &EmployeeRequestScope,
            _id: &str,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<(EmployeeRequestRow, Self::Witness), Self::Error>>
                    + Send
                    + '_,
            >,
        > {
            self.read_calls += 1;
            let row = self.row.clone();
            Box::pin(async move {
                let Some(row) = row else {
                    return Err(missing());
                };
                Ok((row.clone(), guard_for(&row)))
            })
        }

        #[allow(refining_impl_trait)]
        fn require_current(
            &mut self,
            witness: &Self::Witness,
        ) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>> {
            let current = self.current.clone();
            let witness = witness.clone();
            Box::pin(async move {
                let Some(row) = current else {
                    return Err(missing());
                };
                require_current(&witness, &row, &test_scope())
            })
        }
    }

    #[tokio::test]
    async fn prepare_returns_the_guarded_row_and_revalidates_it() {
        let mut port = FakePort {
            row: Some(test_row()),
            current: Some(test_row()),
            read_calls: 0,
        };
        let prepared = prepare_employee_request_source(&mut port, &test_scope(), "request-1")
            .await
            .unwrap();
        assert_eq!(prepared.request.id, "request-1");
        assert_eq!(port.read_calls, 1);
        prepared.require_current(&mut port).await.unwrap();
    }

    #[tokio::test]
    async fn prepare_rejects_a_foreign_row_returned_by_the_port() {
        let mut foreign = test_row();
        foreign.company_id = "foreign".into();
        let mut port = FakePort {
            row: Some(foreign),
            current: None,
            read_calls: 0,
        };
        assert_eq!(
            prepare_employee_request_source(&mut port, &test_scope(), "request-1")
                .await
                .unwrap_err()
                .code,
            "REQUEST_NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn prepare_rejects_an_invalid_id_before_reading() {
        let mut port = FakePort {
            row: Some(test_row()),
            current: None,
            read_calls: 0,
        };
        assert!(
            prepare_employee_request_source(&mut port, &test_scope(), "")
                .await
                .is_err()
        );
        assert_eq!(port.read_calls, 0);
    }

    #[tokio::test]
    async fn prepare_detects_version_drift_at_revalidation() {
        let mut drifted = test_row();
        drifted.version = 3;
        let mut port = FakePort {
            row: Some(test_row()),
            current: Some(drifted),
            read_calls: 0,
        };
        let prepared = prepare_employee_request_source(&mut port, &test_scope(), "request-1")
            .await
            .unwrap();
        assert_eq!(
            prepared.require_current(&mut port).await.unwrap_err().code,
            "VERSION_CONFLICT"
        );
    }

    #[tokio::test]
    async fn quotation_prepare_rejects_a_wrong_assignee() {
        let mut port = FakePort {
            row: Some(test_row()),
            current: None,
            read_calls: 0,
        };
        assert_eq!(
            prepare_quotation_source(&mut port, &test_scope(), "other", "request-1")
                .await
                .unwrap_err()
                .code,
            "PURCHASE_REQUEST_REQUIRED"
        );
    }

    #[tokio::test]
    async fn missing_row_is_not_found() {
        let mut port = FakePort {
            row: None,
            current: None,
            read_calls: 0,
        };
        assert_eq!(
            prepare_employee_request_source(&mut port, &test_scope(), "request-1")
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
    }
}
