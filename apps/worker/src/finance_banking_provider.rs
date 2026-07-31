//! Bounded worker-side coordination for provider bank-statement imports.
//!
//! This module consumes only the validated, provider-neutral adapter boundary.
//! It owns pagination safety and typed fact aggregation; it does not retrieve
//! credentials, perform network calls itself, write SQLx state, register a
//! scheduler, or select a command owner.

use std::collections::HashSet;

use chrono::Utc;
use erpx_domain::contracts::CommandContext;
use erpx_infrastructure::{
    BankProviderAdapter, BankProviderCursor, BankProviderDomainRecords, BankProviderError,
    BankStatementFacts, BankStatementPageRequest,
};
use uuid::Uuid;

/// Fixed upper bound on provider pages consumed by one import.
pub const MAX_BANK_PROVIDER_IMPORT_PAGES: usize = 256;

/// Fixed upper bound on transactions accumulated by one import.
pub const MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS: usize = 100_000;

/// Coordinates a bounded sequence of validated provider pages into one
/// domain-ready bank statement import. It has no persistence or runtime
/// dependencies and is intentionally safe to inject into a future worker
/// composition root.
#[derive(Debug, Clone, Copy, Default)]
pub struct BankProviderImportCoordinator;

impl BankProviderImportCoordinator {
    pub const fn new() -> Self {
        Self
    }

    /// Fetch and combine one complete statement.
    ///
    /// Every follow-up request is rebuilt from the original account/date/
    /// currency/page-size facts and the validated opaque cursor returned by
    /// the preceding page. Only the final, combined facts are mapped to the
    /// existing domain records.
    pub async fn import_statement<A>(
        &self,
        adapter: &A,
        initial_request: BankStatementPageRequest,
        import_id: Uuid,
        context: CommandContext,
    ) -> Result<BankProviderDomainRecords, BankProviderError>
    where
        A: BankProviderAdapter + ?Sized,
    {
        let mut request = initial_request.clone();
        let mut seen_cursors = HashSet::<BankProviderCursor>::new();
        if let Some(cursor) = request.cursor().cloned() {
            seen_cursors.insert(cursor);
        }

        let mut page_count = 0_usize;
        let mut transaction_count = 0_usize;
        let mut statement_reference: Option<String> = None;
        let mut opening_balance_minor: Option<i64> = None;
        let mut previous_closing_balance_minor: Option<i64> = None;
        let mut last_transaction_reference: Option<String> = None;
        let mut transaction_total_minor = 0_i64;
        let mut transactions = Vec::new();

        loop {
            if page_count >= MAX_BANK_PROVIDER_IMPORT_PAGES {
                return Err(BankProviderError::InvalidResponse);
            }

            // Recheck TLS evidence at the aggregation boundary. The adapter
            // hook is overrideable, so this explicit check must remain here.
            adapter.tls_evidence()?.validate_at(Utc::now())?;
            let page = adapter.fetch_validated_statement_page(&request).await?;
            page_count = page_count.saturating_add(1);

            // The trait's validation method is overrideable. Revalidate at
            // the aggregation boundary so a custom adapter cannot return a
            // page bound to another cursor, period, account, or page size.
            page.validate_for_request(&request)?;

            // The adapter's default method performs this validation already;
            // retain the invariant here so custom implementations cannot
            // replace the request-bound account identity.
            if page.account() != request.account().account() {
                return Err(BankProviderError::InvalidResponse);
            }

            let facts = page.facts();
            if let Some(expected_reference) = statement_reference.as_deref() {
                if expected_reference != facts.statement_reference() {
                    return Err(BankProviderError::InvalidResponse);
                }
            } else {
                statement_reference = Some(facts.statement_reference().to_owned());
                opening_balance_minor = Some(facts.opening_balance_minor());
            }
            if let Some(previous_closing_balance) = previous_closing_balance_minor
                && previous_closing_balance != facts.opening_balance_minor()
            {
                return Err(BankProviderError::InvalidResponse);
            }

            let page_transaction_count = facts.transactions().len();
            if page_transaction_count
                > MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS.saturating_sub(transaction_count)
            {
                return Err(BankProviderError::InvalidResponse);
            }

            for transaction in facts.transactions() {
                if last_transaction_reference
                    .as_deref()
                    .is_some_and(|previous| previous >= transaction.transaction_reference())
                {
                    return Err(BankProviderError::InvalidResponse);
                }
                last_transaction_reference = Some(transaction.transaction_reference().to_owned());
            }
            let page_transaction_total_minor =
                facts
                    .transactions()
                    .iter()
                    .try_fold(0_i64, |total, transaction| {
                        total
                            .checked_add(transaction.amount_minor())
                            .ok_or(BankProviderError::InvalidResponse)
                    })?;
            let expected_page_closing_balance = facts
                .opening_balance_minor()
                .checked_add(page_transaction_total_minor)
                .ok_or(BankProviderError::InvalidResponse)?;
            if expected_page_closing_balance != facts.closing_balance_minor() {
                return Err(BankProviderError::InvalidResponse);
            }
            transaction_total_minor = transaction_total_minor
                .checked_add(page_transaction_total_minor)
                .ok_or(BankProviderError::InvalidResponse)?;
            transaction_count = transaction_count.saturating_add(page_transaction_count);
            transactions.extend(facts.transactions().iter().cloned());
            let closing_balance_minor = facts.closing_balance_minor();
            previous_closing_balance_minor = Some(closing_balance_minor);

            match (page.has_more(), page.next_cursor().cloned()) {
                (true, Some(next_cursor)) => {
                    if page_count >= MAX_BANK_PROVIDER_IMPORT_PAGES
                        || !seen_cursors.insert(next_cursor.clone())
                    {
                        return Err(BankProviderError::InvalidResponse);
                    }
                    request = BankStatementPageRequest::try_new(
                        initial_request.account().clone(),
                        initial_request.starts_on(),
                        initial_request.ends_on(),
                        Some(next_cursor),
                        initial_request.page_size(),
                    )?;
                }
                (true, None) | (false, Some(_)) => {
                    return Err(BankProviderError::InvalidResponse);
                }
                (false, None) => {
                    let opening_balance_minor =
                        opening_balance_minor.ok_or(BankProviderError::InvalidResponse)?;
                    let expected_closing_balance = opening_balance_minor
                        .checked_add(transaction_total_minor)
                        .ok_or(BankProviderError::InvalidResponse)?;
                    if expected_closing_balance != closing_balance_minor {
                        return Err(BankProviderError::InvalidResponse);
                    }
                    let complete_facts = BankStatementFacts::try_new_complete(
                        statement_reference.ok_or(BankProviderError::InvalidResponse)?,
                        initial_request.starts_on(),
                        initial_request.ends_on(),
                        initial_request.account().currency().to_owned(),
                        opening_balance_minor,
                        closing_balance_minor,
                        transactions,
                    )?;
                    let records =
                        complete_facts.map_to_domain(&initial_request, import_id, context)?;
                    records.validate_for_persistence()?;
                    return Ok(records);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use chrono::{NaiveDate, Utc};
    use erpx_domain::contracts::{ActorAttribution, ActorKind, TenantScope};
    use erpx_infrastructure::{
        BankProviderAccountBinding, BankProviderAccountRef, BankProviderAdapter,
        BankProviderCursor, BankProviderError, BankProviderTlsEvidence, BankProviderTlsVersion,
        BankStatementFacts, BankStatementPage, BankStatementPageRequest, MAX_PROVIDER_PAGE_SIZE,
        ProviderBankTransaction,
    };
    use uuid::Uuid;

    use super::{
        BankProviderImportCoordinator, MAX_BANK_PROVIDER_IMPORT_PAGES,
        MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS,
    };

    const HOLDING: Uuid = Uuid::from_u128(1);
    const COMPANY: Uuid = Uuid::from_u128(2);
    const ACCOUNT: Uuid = Uuid::from_u128(3);
    const IMPORT: Uuid = Uuid::from_u128(4);

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: HOLDING,
            company_id: COMPANY,
        }
    }

    fn account() -> BankProviderAccountBinding {
        BankProviderAccountBinding::try_new(
            ACCOUNT,
            BankProviderAccountRef::try_new("mockbank", "acct-opaque-1").expect("account ref"),
            scope(),
            HOLDING,
            COMPANY,
            "THB",
            "Operating account",
        )
        .expect("account binding")
    }

    fn request(cursor: Option<BankProviderCursor>) -> BankStatementPageRequest {
        BankStatementPageRequest::try_new(
            account(),
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            cursor,
            MAX_PROVIDER_PAGE_SIZE,
        )
        .expect("page request")
    }

    fn context() -> erpx_domain::contracts::CommandContext {
        erpx_domain::contracts::CommandContext::try_new(
            "bank-provider-import-test",
            ActorAttribution {
                principal_ref: "finance-user".to_owned(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::from_u128(5)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            "bank provider import test",
        )
        .expect("command context")
    }

    fn transaction(reference: &str, amount_minor: i64) -> ProviderBankTransaction {
        ProviderBankTransaction::try_new(
            reference,
            NaiveDate::from_ymd_opt(2026, 1, 10).expect("date"),
            amount_minor,
            "THB",
            "Provider-neutral statement transaction",
        )
        .expect("transaction")
    }

    fn facts(
        statement_reference: &str,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        transactions: Vec<ProviderBankTransaction>,
    ) -> BankStatementFacts {
        BankStatementFacts::try_new(
            statement_reference,
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            "THB",
            opening_balance_minor,
            closing_balance_minor,
            transactions,
        )
        .expect("facts")
    }

    fn page(
        request: &BankStatementPageRequest,
        statement_reference: &str,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        transactions: Vec<ProviderBankTransaction>,
        next_cursor: Option<BankProviderCursor>,
        has_more: bool,
    ) -> BankStatementPage {
        BankStatementPage::try_new(
            request,
            facts(
                statement_reference,
                opening_balance_minor,
                closing_balance_minor,
                transactions,
            ),
            next_cursor,
            has_more,
        )
        .expect("page")
    }

    #[derive(Debug)]
    struct MockAdapter {
        responses: Mutex<VecDeque<Result<BankStatementPage, BankProviderError>>>,
        requests: Arc<Mutex<Vec<BankStatementPageRequest>>>,
    }

    impl MockAdapter {
        fn new(responses: Vec<Result<BankStatementPage, BankProviderError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn requests(&self) -> Vec<BankStatementPageRequest> {
            self.requests.lock().expect("request lock").clone()
        }
    }

    #[async_trait]
    impl BankProviderAdapter for MockAdapter {
        fn provider_key(&self) -> &str {
            "mockbank"
        }

        fn tls_evidence(&self) -> Result<BankProviderTlsEvidence, BankProviderError> {
            BankProviderTlsEvidence::try_new(
                "bank.example.test",
                "fixture/provider-ca",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                BankProviderTlsVersion::Tls1_3,
                Utc::now(),
            )
        }

        async fn fetch_statement_page(
            &self,
            request: &BankStatementPageRequest,
        ) -> Result<BankStatementPage, BankProviderError> {
            self.requests
                .lock()
                .expect("request lock")
                .push(request.clone());
            self.responses
                .lock()
                .expect("response lock")
                .pop_front()
                .unwrap_or(Err(BankProviderError::InvalidResponse))
        }
    }

    #[derive(Debug)]
    struct BypassingValidationAdapter {
        page: BankStatementPage,
    }

    #[async_trait]
    impl BankProviderAdapter for BypassingValidationAdapter {
        fn provider_key(&self) -> &str {
            "mockbank"
        }

        fn tls_evidence(&self) -> Result<BankProviderTlsEvidence, BankProviderError> {
            BankProviderTlsEvidence::try_new(
                "bank.example.test",
                "fixture/provider-ca",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                BankProviderTlsVersion::Tls1_3,
                Utc::now(),
            )
        }

        async fn fetch_statement_page(
            &self,
            _request: &BankStatementPageRequest,
        ) -> Result<BankStatementPage, BankProviderError> {
            Err(BankProviderError::InvalidResponse)
        }

        async fn fetch_validated_statement_page(
            &self,
            _request: &BankStatementPageRequest,
        ) -> Result<BankStatementPage, BankProviderError> {
            Ok(self.page.clone())
        }
    }

    #[tokio::test]
    async fn imports_single_and_multiple_pages_with_boundary_balances() {
        let initial_request = request(None);
        let single = MockAdapter::new(vec![Ok(page(
            &initial_request,
            "statement-2026-01",
            1_000,
            1_100,
            vec![transaction("transaction-001", 100)],
            None,
            false,
        ))]);
        let records = BankProviderImportCoordinator::new()
            .import_statement(&single, initial_request.clone(), IMPORT, context())
            .await
            .expect("single page import");
        assert_eq!(records.statement().transactions().len(), 1);
        assert_eq!(records.statement().opening_balance_minor(), 1_000);
        assert_eq!(records.statement().closing_balance_minor(), 1_100);
        records
            .validate_for_persistence()
            .expect("provider records pass the authoritative write seam");

        let first_cursor = BankProviderCursor::try_new("cursor-001").expect("cursor");
        let second_request = request(Some(first_cursor.clone()));
        let multiple = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_050,
                vec![transaction("transaction-001", 50)],
                Some(first_cursor),
                true,
            )),
            Ok(page(
                &second_request,
                "statement-2026-01",
                1_050,
                1_100,
                vec![transaction("transaction-002", 50)],
                None,
                false,
            )),
        ]);
        let records = BankProviderImportCoordinator::new()
            .import_statement(&multiple, initial_request, IMPORT, context())
            .await
            .expect("multiple page import");
        assert_eq!(records.statement().transactions().len(), 2);
        assert_eq!(records.statement().opening_balance_minor(), 1_000);
        assert_eq!(records.statement().closing_balance_minor(), 1_100);
        let requests = multiple.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[1].cursor().map(BankProviderCursor::as_str),
            Some("cursor-001")
        );
    }

    #[tokio::test]
    async fn reconciles_credit_and_debit_pages_before_mapping() {
        let initial_request = request(None);
        let cursor = BankProviderCursor::try_new("cursor-debit").expect("cursor");
        let next_request = request(Some(cursor.clone()));
        let adapter = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_100,
                vec![transaction("transaction-001", 100)],
                Some(cursor),
                true,
            )),
            Ok(page(
                &next_request,
                "statement-2026-01",
                1_100,
                1_075,
                vec![transaction("transaction-002", -25)],
                None,
                false,
            )),
        ]);

        let records = BankProviderImportCoordinator::new()
            .import_statement(&adapter, initial_request, IMPORT, context())
            .await
            .expect("balanced credit and debit pages");
        assert_eq!(records.statement().transactions().len(), 2);
        assert_eq!(records.statement().opening_balance_minor(), 1_000);
        assert_eq!(records.statement().closing_balance_minor(), 1_075);
        records
            .validate_for_persistence()
            .expect("balanced records pass the persistence seam");
    }

    #[tokio::test]
    async fn rejects_a_page_when_an_adapter_bypasses_request_binding_validation() {
        let initial_request = request(None);
        let bound_to_other_request = request(Some(
            BankProviderCursor::try_new("cursor-bound-to-other-request").expect("cursor"),
        ));
        let page = page(
            &bound_to_other_request,
            "statement-2026-01",
            1_000,
            1_100,
            vec![transaction("transaction-001", 100)],
            None,
            false,
        );
        let adapter = BypassingValidationAdapter { page };

        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&adapter, initial_request, IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn rejects_repeated_cursor_changed_facts_and_non_increasing_references() {
        let initial_request = request(None);
        let cursor_one = BankProviderCursor::try_new("cursor-001").expect("cursor");
        let cursor_two = BankProviderCursor::try_new("cursor-002").expect("cursor");
        let request_one = request(Some(cursor_one.clone()));
        let request_two = request(Some(cursor_two.clone()));
        let repeated = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_010,
                vec![transaction("transaction-001", 10)],
                Some(cursor_one.clone()),
                true,
            )),
            Ok(page(
                &request_one,
                "statement-2026-01",
                1_010,
                1_020,
                vec![transaction("transaction-002", 10)],
                Some(cursor_two.clone()),
                true,
            )),
            Ok(page(
                &request_two,
                "statement-2026-01",
                1_020,
                1_030,
                vec![transaction("transaction-003", 10)],
                Some(cursor_one),
                true,
            )),
        ]);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&repeated, initial_request.clone(), IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );

        let changed_request = BankStatementPageRequest::try_new(
            account(),
            NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 2, 28).expect("date"),
            Some(BankProviderCursor::try_new("cursor-001").expect("cursor")),
            MAX_PROVIDER_PAGE_SIZE,
        )
        .expect("changed request");
        let changed_facts = BankStatementFacts::try_new(
            "statement-2026-02",
            changed_request.starts_on(),
            changed_request.ends_on(),
            "THB",
            1_010,
            1_020,
            vec![
                ProviderBankTransaction::try_new(
                    "transaction-002",
                    NaiveDate::from_ymd_opt(2026, 2, 10).expect("date"),
                    10,
                    "THB",
                    "Changed statement",
                )
                .expect("transaction"),
            ],
        )
        .expect("changed facts");
        let changed_page = BankStatementPage::try_new(&changed_request, changed_facts, None, false)
            .expect("changed page");
        let changed = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_010,
                vec![transaction("transaction-001", 10)],
                Some(BankProviderCursor::try_new("cursor-001").expect("cursor")),
                true,
            )),
            Ok(changed_page),
        ]);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&changed, initial_request.clone(), IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );

        let non_increasing = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_010,
                vec![transaction("transaction-002", 10)],
                Some(BankProviderCursor::try_new("cursor-001").expect("cursor")),
                true,
            )),
            Ok(page(
                &request(Some(
                    BankProviderCursor::try_new("cursor-001").expect("cursor"),
                )),
                "statement-2026-01",
                1_010,
                1_020,
                vec![transaction("transaction-001", 10)],
                None,
                false,
            )),
        ]);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&non_increasing, initial_request, IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );

        let duplicate = MockAdapter::new(vec![
            Ok(page(
                &request(None),
                "statement-2026-01",
                1_000,
                1_010,
                vec![transaction("transaction-001", 10)],
                Some(BankProviderCursor::try_new("cursor-001").expect("cursor")),
                true,
            )),
            Ok(page(
                &request(Some(
                    BankProviderCursor::try_new("cursor-001").expect("cursor"),
                )),
                "statement-2026-01",
                1_010,
                1_020,
                vec![transaction("transaction-001", 10)],
                None,
                false,
            )),
        ]);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&duplicate, request(None), IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn rejects_discontinuous_page_balances() {
        let initial_request = request(None);
        let cursor = BankProviderCursor::try_new("cursor-001").expect("cursor");
        let next_request = request(Some(cursor.clone()));
        let adapter = MockAdapter::new(vec![
            Ok(page(
                &initial_request,
                "statement-2026-01",
                1_000,
                1_010,
                vec![transaction("transaction-001", 10)],
                Some(cursor),
                true,
            )),
            Ok(page(
                &next_request,
                "statement-2026-01",
                1_009,
                1_020,
                vec![transaction("transaction-002", 11)],
                None,
                false,
            )),
        ]);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&adapter, initial_request, IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn rejects_page_and_transaction_limits_without_persistence() {
        let initial_request = request(None);
        let mut page_responses = Vec::new();
        let mut cursor = None;
        for page_index in 0..MAX_BANK_PROVIDER_IMPORT_PAGES {
            let page_request = if let Some(cursor) = cursor.clone() {
                request(Some(cursor))
            } else {
                initial_request.clone()
            };
            let next_cursor =
                BankProviderCursor::try_new(format!("page-{page_index:03}")).expect("cursor");
            cursor = Some(next_cursor.clone());
            page_responses.push(Ok(page(
                &page_request,
                "statement-2026-01",
                1_000 + page_index as i64,
                1_001 + page_index as i64,
                vec![transaction(&format!("transaction-{page_index:08}"), 1)],
                Some(next_cursor),
                true,
            )));
        }
        let too_many_pages = MockAdapter::new(page_responses);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&too_many_pages, initial_request.clone(), IMPORT, context())
                .await,
            Err(BankProviderError::InvalidResponse)
        );

        let mut transaction_responses = Vec::new();
        let per_page = MAX_PROVIDER_PAGE_SIZE as usize;
        let page_count = MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS / per_page + 1;
        let mut cursor = None;
        for page_index in 0..page_count {
            let page_request = if let Some(cursor) = cursor.clone() {
                request(Some(cursor))
            } else {
                initial_request.clone()
            };
            let start = page_index * per_page;
            let transactions = (start..start + per_page)
                .map(|index| transaction(&format!("transaction-{index:08}"), 1))
                .collect();
            let has_more = page_index + 1 < page_count;
            let next_cursor = has_more.then(|| {
                BankProviderCursor::try_new(format!("transaction-page-{page_index:03}"))
                    .expect("cursor")
            });
            cursor = next_cursor.clone();
            transaction_responses.push(Ok(page(
                &page_request,
                "statement-2026-01",
                1_000 + start as i64,
                1_000 + (start + per_page) as i64,
                transactions,
                next_cursor,
                has_more,
            )));
        }
        let too_many_transactions = MockAdapter::new(transaction_responses);
        assert_eq!(
            BankProviderImportCoordinator::new()
                .import_statement(&too_many_transactions, initial_request, IMPORT, context(),)
                .await,
            Err(BankProviderError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn provider_error_and_invalid_final_metadata_are_redacted() {
        let adapter = MockAdapter::new(vec![Err(BankProviderError::AuthenticationFailed)]);
        let error = BankProviderImportCoordinator::new()
            .import_statement(&adapter, request(None), IMPORT, context())
            .await
            .expect_err("provider error");
        assert_eq!(error, BankProviderError::AuthenticationFailed);
        let rendered = format!("{error:?} {error}");
        for secret_marker in ["secret", "token", "bearer", "password", "card"] {
            assert!(!rendered.contains(secret_marker));
        }

        // The provider DTO constructor rejects a final page carrying a
        // cursor. The coordinator receives that as a redacted boundary error.
        let final_request = request(None);
        let invalid_final = BankStatementPage::try_new(
            &final_request,
            facts(
                "statement-2026-01",
                1_000,
                1_100,
                vec![transaction("transaction-001", 100)],
            ),
            Some(BankProviderCursor::try_new("cursor-final").expect("cursor")),
            false,
        );
        assert_eq!(invalid_final, Err(BankProviderError::InvalidResponse));
    }
}
