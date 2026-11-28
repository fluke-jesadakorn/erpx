//! Composition backend for the Company-keyed native people-payroll command
//! route used by system workers. The owner crate owns the allow-list, SQL and
//! adapter; this module only opens the caller Company's serializable unit.
use erpx_people_payroll_server::public::native_command::{
    NativeCommand, PgNativeCommandRepository, execute_native_command,
};
use erpx_people_payroll_server::public::native_command_http::{
    NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpResponse,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;
use std::collections::BTreeMap;

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            erpx_platform_shared_server::public::FailureKind::Unavailable,
            "NATIVE_PEOPLE_PAYROLL_UNAVAILABLE",
            "The native people-payroll service is unavailable.",
        ),
    }
}

pub struct PeopleCommandBackend {
    companies: BTreeMap<String, DatabasePool>,
}

impl PeopleCommandBackend {
    pub fn new(companies: BTreeMap<String, DatabasePool>) -> Self {
        Self { companies }
    }
}

impl NativeCommandHttpBackend for PeopleCommandBackend {
    fn execute<'a>(
        &'a self,
        request: NativeCommandHttpRequest<'a>,
    ) -> NativeCommandHttpFuture<'a> {
        Box::pin(async move {
            let company = request
                .headers
                .get("x-erpx-context-company")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "NATIVE_PEOPLE_PAYROLL_CONTEXT_REQUIRED",
                        "Use one bounded native people-payroll command.",
                    )
                })?
                .to_owned();
            let pool = self.companies.get(&company).cloned().ok_or_else(|| {
                BusinessError::new(
                    erpx_platform_shared_server::public::FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                )
            })?;
            let command = NativeCommand {
                kind: request.command.kind.clone(),
                values: request.command.values.clone(),
                now: request.command.now,
            };
            let key = request.key.as_str().to_owned();
            let label = company.clone();
            PgUnitOfWork::new(pool)
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let command = command.clone();
                    let key = key.clone();
                    let label = label.clone();
                    Box::pin(async move {
                        let mut repository = PgNativeCommandRepository::new(connection);
                        let outcome = execute_native_command(&command, &key, &mut repository)
                            .await?;
                        Ok(NativeCommandHttpResponse {
                            data: json!({"kind": outcome.kind, "rows": outcome.rows}),
                            context: json!({"companyId": label}),
                        })
                    })
                })
                .await
                .map_err(business)
        })
    }
}
