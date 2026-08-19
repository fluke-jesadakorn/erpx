use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_infrastructure::{
    BankProviderAccountBinding, BankProviderAccountRef, BankProviderAdapter, BankProviderCursor,
    BankProviderError, BankProviderTlsEvidence, BankProviderTlsVersion, BankStatementFacts,
    BankStatementPage, BankStatementPageRequest, MAX_PROVIDER_PAGE_SIZE, ProviderBankTransaction,
};
use uuid::Uuid;

const HOLDING: Uuid = Uuid::from_u128(1);
const COMPANY: Uuid = Uuid::from_u128(2);
const OTHER_COMPANY: Uuid = Uuid::from_u128(3);
const ACCOUNT: Uuid = Uuid::from_u128(10);
const IMPORT: Uuid = Uuid::from_u128(11);

fn scope(company_id: Uuid) -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: HOLDING,
        company_id,
    }
}

fn provider_account() -> BankProviderAccountRef {
    BankProviderAccountRef::try_new("mockbank", "acct-opaque-1").expect("opaque account ref")
}

fn binding(company_id: Uuid) -> BankProviderAccountBinding {
    BankProviderAccountBinding::try_new(
        ACCOUNT,
        provider_account(),
        scope(company_id),
        HOLDING,
        company_id,
        "THB",
        "Operating account",
    )
    .expect("account binding")
}

fn request(company_id: Uuid, cursor: Option<BankProviderCursor>) -> BankStatementPageRequest {
    BankStatementPageRequest::try_new(
        binding(company_id),
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
        cursor,
        10,
    )
    .expect("statement request")
}

fn facts() -> BankStatementFacts {
    BankStatementFacts::try_new(
        "statement-2026-01",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
        "THB",
        1_000,
        1_100,
        vec![
            ProviderBankTransaction::try_new(
                "transaction-1",
                NaiveDate::from_ymd_opt(2026, 1, 10).expect("date"),
                100,
                "THB",
                "Statement transaction",
            )
            .expect("transaction"),
        ],
    )
    .expect("statement facts")
}

fn context(company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-user".to_owned(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::from_u128(20)),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(company_id),
        "finance banking provider import",
    )
    .expect("command context")
}

fn page(request: &BankStatementPageRequest) -> BankStatementPage {
    BankStatementPage::try_new(request, facts(), None, false).expect("complete statement page")
}

#[test]
fn bounds_and_credential_shaped_references_fail_closed() {
    assert!(BankProviderCursor::try_new("").is_err());
    assert!(BankProviderCursor::try_new("Bearer-secret").is_err());
    assert!(BankProviderCursor::try_new("cursor/with-url-data").is_err());
    assert!(
        BankStatementPageRequest::try_new(
            binding(COMPANY),
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            None,
            0,
        )
        .is_err()
    );
    assert!(
        BankStatementPageRequest::try_new(
            binding(COMPANY),
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("date"),
            None,
            MAX_PROVIDER_PAGE_SIZE + 1,
        )
        .is_err()
    );
    assert!(BankProviderAccountRef::try_new("mockbank", "api_key-secret").is_err());

    let first_page_request = request(COMPANY, None);
    let cursor = BankProviderCursor::try_new("cursor-1").expect("cursor");
    assert!(
        BankStatementPage::try_new(&first_page_request, facts(), Some(cursor.clone()), false,)
            .is_err()
    );
    let next_page_request = request(COMPANY, Some(cursor.clone()));
    assert!(BankStatementPage::try_new(&next_page_request, facts(), Some(cursor), true,).is_err());

    let incomplete = BankStatementPage::try_new(
        &first_page_request,
        facts(),
        Some(BankProviderCursor::try_new("cursor-2").expect("cursor")),
        true,
    )
    .expect("bounded non-final page");
    assert!(
        incomplete
            .map_to_domain(
                &first_page_request,
                IMPORT,
                context(COMPANY, "incomplete-import")
            )
            .is_err()
    );
}

#[test]
fn scope_and_company_binding_are_not_inferred_from_provider_output() {
    let company_request = request(COMPANY, None);
    let other_company_request = request(OTHER_COMPANY, None);
    let company_page = page(&company_request);

    assert!(
        company_page
            .validate_for_request(&other_company_request)
            .is_err()
    );
    let mapped = company_page
        .map_to_domain(&company_request, IMPORT, context(COMPANY, "import-1"))
        .expect("matching scope maps");
    assert_eq!(mapped.account().holding_id(), HOLDING);
    assert_eq!(mapped.account().company_id(), COMPANY);
    assert_eq!(mapped.statement().holding_id(), HOLDING);
    assert_eq!(mapped.statement().company_id(), COMPANY);
    assert!(
        company_page
            .map_to_domain(&company_request, IMPORT, context(OTHER_COMPANY, "import-2"))
            .is_err()
    );
}

#[test]
fn provider_statement_balance_drift_is_rejected_before_persistence_mapping() {
    let request = request(COMPANY, None);
    let transaction = ProviderBankTransaction::try_new(
        "transaction-1",
        NaiveDate::from_ymd_opt(2026, 1, 10).expect("date"),
        100,
        "THB",
        "Statement transaction",
    )
    .expect("transaction");
    assert_eq!(
        BankStatementFacts::try_new(
            "statement-balance-drift",
            request.starts_on(),
            request.ends_on(),
            "THB",
            1_000,
            1_101,
            vec![transaction],
        ),
        Err(BankProviderError::InvalidResponse)
    );

    let mapped = page(&request)
        .map_to_domain(&request, IMPORT, context(COMPANY, "persistence-boundary"))
        .expect("valid provider facts map");
    mapped
        .validate_for_persistence()
        .expect("domain records remain valid at the persistence seam");
}

#[test]
fn page_order_and_request_idempotency_binding_are_deterministic() {
    let first = request(COMPANY, None);
    let second = request(COMPANY, None);
    assert_eq!(
        first.binding().expect("binding"),
        second.binding().expect("binding")
    );
    assert_eq!(
        first.binding().expect("binding").as_idempotency_key(),
        second.binding().expect("binding").as_idempotency_key()
    );

    let first_page = page(&first);
    let second_page = page(&second);
    assert_eq!(
        first_page.content_digest().expect("digest"),
        second_page.content_digest().expect("digest")
    );
    assert_eq!(first_page.request_binding(), second_page.request_binding());

    let unsorted = BankStatementFacts::try_new(
        "statement-2026-01",
        first.starts_on(),
        first.ends_on(),
        "THB",
        1_000,
        1_100,
        vec![
            ProviderBankTransaction::try_new(
                "transaction-2",
                NaiveDate::from_ymd_opt(2026, 1, 10).expect("date"),
                100,
                "THB",
                "Second",
            )
            .expect("transaction"),
            ProviderBankTransaction::try_new(
                "transaction-1",
                NaiveDate::from_ymd_opt(2026, 1, 11).expect("date"),
                100,
                "THB",
                "First",
            )
            .expect("transaction"),
        ],
    );
    assert!(unsorted.is_err());
}

#[test]
fn mapping_is_domain_ready_and_transaction_ids_are_stable() {
    let statement_request = request(COMPANY, None);
    let page = page(&statement_request);
    let first = page
        .map_to_domain(&statement_request, IMPORT, context(COMPANY, "import-1"))
        .expect("domain records");
    let second = page
        .map_to_domain(&statement_request, IMPORT, context(COMPANY, "import-2"))
        .expect("domain replay records");
    assert_eq!(first.account(), second.account());
    assert_eq!(
        first.statement().content_digest(),
        second.statement().content_digest()
    );
    assert_eq!(
        first.statement().transactions()[0].id(),
        second.statement().transactions()[0].id()
    );
}

struct FakeAdapter {
    page: BankStatementPage,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl BankProviderAdapter for FakeAdapter {
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.page.clone())
    }
}

#[tokio::test]
async fn injected_fake_adapter_is_bound_without_network_or_raw_payloads() {
    let initial_request = request(COMPANY, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let adapter = FakeAdapter {
        page: page(&initial_request),
        calls: Arc::clone(&calls),
    };
    let fetched = adapter
        .fetch_validated_statement_page(&initial_request)
        .await
        .expect("validated fake page");
    assert_eq!(
        fetched.request_binding(),
        &initial_request.binding().expect("binding")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let wrong_cursor = BankProviderCursor::try_new("cursor-2").expect("cursor");
    let wrong_request = request(COMPANY, Some(wrong_cursor));
    assert!(
        adapter
            .fetch_validated_statement_page(&wrong_request)
            .await
            .is_err()
    );
}

#[test]
fn provider_errors_are_redacted() {
    let errors = [
        BankProviderError::Unavailable,
        BankProviderError::AuthenticationFailed,
        BankProviderError::ProtocolFailure,
    ];
    for error in errors {
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("token"));
        assert!(!rendered.contains("card"));
    }
}

#[test]
fn tls_evidence_requires_a_fresh_verified_certificate_and_safe_endpoint() {
    let now = Utc::now();
    let fingerprint = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let evidence = BankProviderTlsEvidence::try_new(
        "bank.example.test",
        "fixture/provider-ca",
        fingerprint,
        BankProviderTlsVersion::Tls1_3,
        now,
    )
    .expect("valid TLS evidence");
    assert_eq!(evidence.server_name(), "bank.example.test");
    assert_eq!(evidence.peer_certificate_fingerprint(), fingerprint);
    assert!(evidence.validate_at(now).is_ok());

    assert!(
        BankProviderTlsEvidence::try_new(
            "https://bank.example.test",
            "fixture/provider-ca",
            fingerprint,
            BankProviderTlsVersion::Tls1_3,
            now,
        )
        .is_err()
    );
    assert!(
        BankProviderTlsEvidence::try_new(
            "bank.example.test",
            "fixture/provider-ca",
            "not-a-certificate-fingerprint",
            BankProviderTlsVersion::Tls1_3,
            now,
        )
        .is_err()
    );

    let stale = BankProviderTlsEvidence::try_new(
        "bank.example.test",
        "fixture/provider-ca",
        fingerprint,
        BankProviderTlsVersion::Tls1_2,
        now - erpx_infrastructure::MAX_PROVIDER_TLS_EVIDENCE_AGE - Duration::seconds(1),
    )
    .expect("stale evidence shape");
    assert_eq!(
        stale.validate_at(now),
        Err(BankProviderError::TransportSecurityRejected)
    );
    let future = BankProviderTlsEvidence::try_new(
        "bank.example.test",
        "fixture/provider-ca",
        fingerprint,
        BankProviderTlsVersion::Tls1_2,
        now + erpx_infrastructure::MAX_PROVIDER_TLS_CLOCK_SKEW + Duration::seconds(1),
    )
    .expect("future evidence shape");
    assert_eq!(
        future.validate_at(now),
        Err(BankProviderError::TransportSecurityRejected)
    );
}
