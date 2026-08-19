//! Provider-neutral boundary for importing validated bank statement facts.
//!
//! Implementations translate an external provider's SDK response into the
//! typed DTOs in this module.  Credentials, tokens, payment instruments, and
//! provider-specific payloads deliberately have no representation here.  The
//! returned domain records can be passed to [`SqlxFinanceBankingStore`] by a
//! caller that owns authorization and persistence.

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use erpx_domain::contracts::{AuthorityScope, CommandContext};
use erpx_domain::finance_banking::{BankAccount, BankStatementImport, ImportedBankTransaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_PROVIDER_NAME_BYTES: usize = 64;
pub const MAX_PROVIDER_ACCOUNT_REFERENCE_BYTES: usize = 191;
pub const MAX_PROVIDER_TRANSACTION_REFERENCE_BYTES: usize = 255;
pub const MAX_PROVIDER_CURSOR_BYTES: usize = 512;
pub const MAX_PROVIDER_PAGE_SIZE: u32 = 500;
pub const MAX_PROVIDER_SERVER_NAME_BYTES: usize = 255;
pub const MAX_PROVIDER_TRUST_ANCHOR_REFERENCE_BYTES: usize = 255;
pub const MAX_PROVIDER_TLS_EVIDENCE_AGE: ChronoDuration = ChronoDuration::minutes(5);
pub const MAX_PROVIDER_TLS_CLOCK_SKEW: ChronoDuration = ChronoDuration::seconds(30);
const MAX_PROVIDER_COMPLETE_STATEMENT_TRANSACTIONS: usize = MAX_PROVIDER_PAGE_SIZE as usize * 200;

/// A deliberately redacted error vocabulary for provider failures and
/// boundary validation.  It carries no provider message, URL, request body,
/// credential, token, or response payload.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BankProviderError {
    #[error("invalid provider adapter input")]
    InvalidInput,
    #[error("invalid provider adapter response")]
    InvalidResponse,
    #[error("provider service is unavailable")]
    Unavailable,
    #[error("provider request was rate limited")]
    RateLimited,
    #[error("provider authentication failed")]
    AuthenticationFailed,
    #[error("provider rejected the request")]
    Rejected,
    #[error("provider protocol failed")]
    ProtocolFailure,
    #[error("provider adapter did not prove a fresh certificate-validated TLS transport")]
    TransportSecurityRejected,
    #[error("validated statement facts could not be mapped to the finance domain")]
    DomainMappingRejected,
}

/// TLS versions accepted for a bank-provider connection. Plain HTTP and
/// unverified TLS cannot be represented by this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankProviderTlsVersion {
    Tls1_2,
    Tls1_3,
}

/// Handshake evidence supplied by a concrete provider adapter after it has
/// completed certificate validation. The worker revalidates freshness at
/// every page boundary, including adapters that override the fetch hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankProviderTlsEvidence {
    server_name: String,
    trust_anchor_reference: String,
    peer_certificate_fingerprint: String,
    negotiated_version: BankProviderTlsVersion,
    verified_at: DateTime<Utc>,
}

impl BankProviderTlsEvidence {
    pub fn try_new(
        server_name: impl Into<String>,
        trust_anchor_reference: impl Into<String>,
        peer_certificate_fingerprint: impl Into<String>,
        negotiated_version: BankProviderTlsVersion,
        verified_at: DateTime<Utc>,
    ) -> Result<Self, BankProviderError> {
        let evidence = Self {
            server_name: server_name.into(),
            trust_anchor_reference: trust_anchor_reference.into(),
            peer_certificate_fingerprint: peer_certificate_fingerprint.into(),
            negotiated_version,
            verified_at,
        };
        evidence.validate_shape()?;
        Ok(evidence)
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn trust_anchor_reference(&self) -> &str {
        &self.trust_anchor_reference
    }

    pub fn peer_certificate_fingerprint(&self) -> &str {
        &self.peer_certificate_fingerprint
    }

    pub const fn negotiated_version(&self) -> BankProviderTlsVersion {
        self.negotiated_version
    }

    pub const fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }

    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), BankProviderError> {
        self.validate_shape()?;
        if now < self.verified_at - MAX_PROVIDER_TLS_CLOCK_SKEW
            || now.signed_duration_since(self.verified_at) > MAX_PROVIDER_TLS_EVIDENCE_AGE
        {
            return Err(BankProviderError::TransportSecurityRejected);
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), BankProviderError> {
        if !is_valid_server_name(&self.server_name)
            || self.trust_anchor_reference.is_empty()
            || self.trust_anchor_reference.len() > MAX_PROVIDER_TRUST_ANCHOR_REFERENCE_BYTES
            || self
                .trust_anchor_reference
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte)))
            || self.peer_certificate_fingerprint.len() != 64
            || !self
                .peer_certificate_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(BankProviderError::TransportSecurityRejected);
        }
        Ok(())
    }
}

/// Opaque provider and account references.  This is not a credential
/// container: only a restricted reference alphabet is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankProviderAccountRef {
    provider: String,
    account_reference: String,
}

impl BankProviderAccountRef {
    pub fn try_new(
        provider: impl Into<String>,
        account_reference: impl Into<String>,
    ) -> Result<Self, BankProviderError> {
        let provider = provider.into();
        let account_reference = account_reference.into();
        validate_provider_key(&provider)?;
        validate_opaque_reference(&account_reference, MAX_PROVIDER_ACCOUNT_REFERENCE_BYTES)?;
        if provider.len() + 1 + account_reference.len() > 255 {
            return Err(BankProviderError::InvalidInput);
        }
        Ok(Self {
            provider,
            account_reference,
        })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn reference(&self) -> &str {
        &self.account_reference
    }

    /// Stable representation suitable for the domain's opaque account
    /// reference field.  It contains no secret material.
    pub fn storage_reference(&self) -> String {
        format!("{}:{}", self.provider, self.account_reference)
    }
}

/// Binds a provider account to one local bank aggregate and one legal-company
/// scope.  The binding is carried in the request and never inferred from a
/// provider response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankProviderAccountBinding {
    local_account_id: Uuid,
    account: BankProviderAccountRef,
    scope: AuthorityScope,
    holding_id: Uuid,
    company_id: Uuid,
    currency: String,
    display_name: String,
}

impl BankProviderAccountBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        local_account_id: Uuid,
        account: BankProviderAccountRef,
        scope: AuthorityScope,
        holding_id: Uuid,
        company_id: Uuid,
        currency: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, BankProviderError> {
        validate_account_binding_scope(&scope, local_account_id, holding_id, company_id)?;
        let binding = Self {
            local_account_id,
            account,
            scope,
            holding_id,
            company_id,
            currency: currency.into(),
            display_name: display_name.into(),
        };
        binding.to_domain_account()?;
        Ok(binding)
    }

    pub fn local_account_id(&self) -> Uuid {
        self.local_account_id
    }

    pub fn account(&self) -> &BankProviderAccountRef {
        &self.account
    }

    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    fn to_domain_account(&self) -> Result<BankAccount, BankProviderError> {
        BankAccount::new(
            self.local_account_id,
            self.scope.clone(),
            self.holding_id,
            self.company_id,
            self.currency.clone(),
            self.account.storage_reference(),
            self.display_name.clone(),
        )
        .map_err(|_| BankProviderError::DomainMappingRejected)
    }
}

/// A provider pagination cursor is opaque and bounded.  It is not a bearer
/// token and the restricted alphabet prevents URLs, headers, and arbitrary
/// serialized payloads from crossing this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct BankProviderCursor(String);

impl BankProviderCursor {
    pub fn try_new(value: impl Into<String>) -> Result<Self, BankProviderError> {
        let value = value.into();
        validate_opaque_reference(&value, MAX_PROVIDER_CURSOR_BYTES)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The exact request identity used to bind a provider page to holding scope,
/// account, period, cursor, and page size.  It is deterministic and safe to
/// use as an idempotency correlation value; it is not a secret.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct BankProviderRequestBinding {
    digest: [u8; 32],
}

impl BankProviderRequestBinding {
    pub fn as_hex(&self) -> String {
        hex::encode(self.digest)
    }

    pub fn as_idempotency_key(&self) -> String {
        format!("bank-provider-page-v1:{}", self.as_hex())
    }
}

/// Bounded, scope-bound statement request passed to an injected provider
/// adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankStatementPageRequest {
    account: BankProviderAccountBinding,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    cursor: Option<BankProviderCursor>,
    page_size: u32,
}

impl BankStatementPageRequest {
    pub fn try_new(
        account: BankProviderAccountBinding,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        cursor: Option<BankProviderCursor>,
        page_size: u32,
    ) -> Result<Self, BankProviderError> {
        if ends_on < starts_on || page_size == 0 || page_size > MAX_PROVIDER_PAGE_SIZE {
            return Err(BankProviderError::InvalidInput);
        }
        let request = Self {
            account,
            starts_on,
            ends_on,
            cursor,
            page_size,
        };
        request.account.to_domain_account()?;
        Ok(request)
    }

    pub fn account(&self) -> &BankProviderAccountBinding {
        &self.account
    }

    pub fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }

    pub fn cursor(&self) -> Option<&BankProviderCursor> {
        self.cursor.as_ref()
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn binding(&self) -> Result<BankProviderRequestBinding, BankProviderError> {
        let encoded = serde_json::to_vec(self).map_err(|_| BankProviderError::InvalidInput)?;
        Ok(BankProviderRequestBinding {
            digest: Sha256::digest(encoded).into(),
        })
    }

    fn validate_for_provider(&self, provider: &str) -> Result<(), BankProviderError> {
        validate_provider_key(provider)?;
        if self.account.account.provider() == provider {
            Ok(())
        } else {
            Err(BankProviderError::InvalidInput)
        }
    }
}

/// A fixed schema for one provider transaction.  There is intentionally no
/// raw JSON, metadata map, authorization token, card field, or payment
/// instrument field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderBankTransaction {
    transaction_reference: String,
    posted_on: NaiveDate,
    amount_minor: i64,
    currency: String,
    description: String,
}

impl ProviderBankTransaction {
    pub fn try_new(
        transaction_reference: impl Into<String>,
        posted_on: NaiveDate,
        amount_minor: i64,
        currency: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, BankProviderError> {
        let transaction = Self {
            transaction_reference: transaction_reference.into(),
            posted_on,
            amount_minor,
            currency: currency.into(),
            description: description.into(),
        };
        transaction.validate()?;
        Ok(transaction)
    }

    pub fn transaction_reference(&self) -> &str {
        &self.transaction_reference
    }

    pub fn posted_on(&self) -> NaiveDate {
        self.posted_on
    }

    pub fn amount_minor(&self) -> i64 {
        self.amount_minor
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    fn validate(&self) -> Result<(), BankProviderError> {
        validate_opaque_reference(
            &self.transaction_reference,
            MAX_PROVIDER_TRANSACTION_REFERENCE_BYTES,
        )?;
        if self.amount_minor == 0 || !is_currency(&self.currency) {
            return Err(BankProviderError::InvalidResponse);
        }
        validate_text(&self.description, 1_024)?;
        Ok(())
    }

    fn to_domain(
        &self,
        bank_account_id: Uuid,
    ) -> Result<ImportedBankTransaction, BankProviderError> {
        let deterministic_id =
            deterministic_transaction_id(bank_account_id, &self.transaction_reference);
        ImportedBankTransaction::new(
            deterministic_id,
            bank_account_id,
            self.transaction_reference.clone(),
            self.posted_on,
            self.amount_minor,
            self.currency.clone(),
            self.description.clone(),
        )
        .map_err(|_| BankProviderError::DomainMappingRejected)
    }
}

/// Validated statement facts.  A page may contain at most the configured
/// provider page size; the final-page check is performed by the page mapping
/// method before a domain import is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankStatementFacts {
    statement_reference: String,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    currency: String,
    opening_balance_minor: i64,
    closing_balance_minor: i64,
    transactions: Vec<ProviderBankTransaction>,
}

impl BankStatementFacts {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        statement_reference: impl Into<String>,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        currency: impl Into<String>,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        transactions: Vec<ProviderBankTransaction>,
    ) -> Result<Self, BankProviderError> {
        let facts = Self {
            statement_reference: statement_reference.into(),
            starts_on,
            ends_on,
            currency: currency.into(),
            opening_balance_minor,
            closing_balance_minor,
            transactions,
        };
        facts.validate()?;
        Ok(facts)
    }

    /// Construct facts for a complete statement assembled from validated
    /// provider pages. A complete statement may contain more transactions
    /// than one provider page, but it remains bounded at this boundary.
    pub fn try_new_complete(
        statement_reference: impl Into<String>,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        currency: impl Into<String>,
        opening_balance_minor: i64,
        closing_balance_minor: i64,
        transactions: Vec<ProviderBankTransaction>,
    ) -> Result<Self, BankProviderError> {
        let facts = Self {
            statement_reference: statement_reference.into(),
            starts_on,
            ends_on,
            currency: currency.into(),
            opening_balance_minor,
            closing_balance_minor,
            transactions,
        };
        facts.validate_with_limit(MAX_PROVIDER_COMPLETE_STATEMENT_TRANSACTIONS)?;
        Ok(facts)
    }

    pub fn statement_reference(&self) -> &str {
        &self.statement_reference
    }

    pub fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }

    pub fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }

    pub fn opening_balance_minor(&self) -> i64 {
        self.opening_balance_minor
    }

    pub fn closing_balance_minor(&self) -> i64 {
        self.closing_balance_minor
    }

    pub fn transactions(&self) -> &[ProviderBankTransaction] {
        &self.transactions
    }

    /// Map complete, validated facts to the existing domain records without
    /// performing persistence. The request supplies the authoritative account
    /// and scope; no provider response can replace them.
    pub fn map_to_domain(
        &self,
        request: &BankStatementPageRequest,
        import_id: Uuid,
        context: CommandContext,
    ) -> Result<BankProviderDomainRecords, BankProviderError> {
        if self.starts_on() != request.starts_on()
            || self.ends_on() != request.ends_on()
            || self.currency() != request.account.currency()
        {
            return Err(BankProviderError::InvalidResponse);
        }
        self.validate_with_limit(MAX_PROVIDER_COMPLETE_STATEMENT_TRANSACTIONS)?;
        let account = request.account.to_domain_account()?;
        let transactions = self
            .transactions()
            .iter()
            .map(|transaction| transaction.to_domain(account.id()))
            .collect::<Result<Vec<_>, _>>()?;
        let statement = BankStatementImport::new(
            import_id,
            account.id(),
            account.scope().clone(),
            account.holding_id(),
            account.company_id(),
            self.statement_reference().to_owned(),
            self.starts_on(),
            self.ends_on(),
            self.opening_balance_minor(),
            self.closing_balance_minor(),
            transactions,
            context,
        )
        .map_err(|_| BankProviderError::DomainMappingRejected)?;
        let records = BankProviderDomainRecords { account, statement };
        records.validate_for_persistence()?;
        Ok(records)
    }

    fn validate(&self) -> Result<(), BankProviderError> {
        self.validate_with_limit(MAX_PROVIDER_PAGE_SIZE as usize)
    }

    fn validate_with_limit(&self, max_transactions: usize) -> Result<(), BankProviderError> {
        validate_opaque_reference(&self.statement_reference, 255)?;
        if self.ends_on < self.starts_on || !is_currency(&self.currency) {
            return Err(BankProviderError::InvalidResponse);
        }
        if self.transactions.len() > max_transactions {
            return Err(BankProviderError::InvalidResponse);
        }
        let mut previous = None;
        let mut transaction_total_minor = 0_i64;
        for transaction in &self.transactions {
            transaction.validate()?;
            if transaction.currency() != self.currency
                || transaction.posted_on() < self.starts_on
                || transaction.posted_on() > self.ends_on
            {
                return Err(BankProviderError::InvalidResponse);
            }
            if previous
                .is_some_and(|reference: &str| reference >= transaction.transaction_reference())
            {
                return Err(BankProviderError::InvalidResponse);
            }
            previous = Some(transaction.transaction_reference());
            transaction_total_minor = transaction_total_minor
                .checked_add(transaction.amount_minor())
                .ok_or(BankProviderError::InvalidResponse)?;
        }
        let expected_closing_balance_minor = self
            .opening_balance_minor
            .checked_add(transaction_total_minor)
            .ok_or(BankProviderError::InvalidResponse)?;
        if expected_closing_balance_minor != self.closing_balance_minor {
            return Err(BankProviderError::InvalidResponse);
        }
        Ok(())
    }
}

/// A provider response bound to the exact request that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankStatementPage {
    request_binding: BankProviderRequestBinding,
    account: BankProviderAccountRef,
    facts: BankStatementFacts,
    next_cursor: Option<BankProviderCursor>,
    has_more: bool,
}

impl BankStatementPage {
    pub fn try_new(
        request: &BankStatementPageRequest,
        facts: BankStatementFacts,
        next_cursor: Option<BankProviderCursor>,
        has_more: bool,
    ) -> Result<Self, BankProviderError> {
        facts.validate()?;
        if facts.starts_on() != request.starts_on()
            || facts.ends_on() != request.ends_on()
            || facts.currency() != request.account.currency()
            || facts.transactions().len() > request.page_size() as usize
        {
            return Err(BankProviderError::InvalidResponse);
        }
        if has_more {
            let Some(next_cursor) = next_cursor.as_ref() else {
                return Err(BankProviderError::InvalidResponse);
            };
            if request.cursor().is_some_and(|cursor| cursor == next_cursor) {
                return Err(BankProviderError::InvalidResponse);
            }
        } else if next_cursor.is_some() {
            return Err(BankProviderError::InvalidResponse);
        }
        Ok(Self {
            request_binding: request.binding()?,
            account: request.account.account.clone(),
            facts,
            next_cursor,
            has_more,
        })
    }

    pub fn request_binding(&self) -> &BankProviderRequestBinding {
        &self.request_binding
    }

    pub fn account(&self) -> &BankProviderAccountRef {
        &self.account
    }

    pub fn facts(&self) -> &BankStatementFacts {
        &self.facts
    }

    pub fn next_cursor(&self) -> Option<&BankProviderCursor> {
        self.next_cursor.as_ref()
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }

    pub fn content_digest(&self) -> Result<String, BankProviderError> {
        let encoded = serde_json::to_vec(self).map_err(|_| BankProviderError::InvalidResponse)?;
        Ok(hex::encode(Sha256::digest(encoded)))
    }

    pub fn validate_for_request(
        &self,
        request: &BankStatementPageRequest,
    ) -> Result<(), BankProviderError> {
        if self.request_binding != request.binding()? || self.account != *request.account.account()
        {
            return Err(BankProviderError::InvalidResponse);
        }
        Self::try_new(
            request,
            self.facts.clone(),
            self.next_cursor.clone(),
            self.has_more,
        )
        .map(|_| ())
    }

    /// Convert only a validated, complete statement into the exact domain
    /// records consumed by `SqlxFinanceBankingStore::register_account` and
    /// `import_statement`.  Multi-page responses must be accumulated by the
    /// caller as typed facts and mapped only after the final page.
    pub fn map_to_domain(
        &self,
        request: &BankStatementPageRequest,
        import_id: Uuid,
        context: CommandContext,
    ) -> Result<BankProviderDomainRecords, BankProviderError> {
        self.validate_for_request(request)?;
        if self.has_more {
            return Err(BankProviderError::InvalidResponse);
        }
        self.facts.map_to_domain(request, import_id, context)
    }
}

/// Domain-ready values for the existing SQLx write methods.  No persistence
/// occurs while constructing this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankProviderDomainRecords {
    account: BankAccount,
    statement: BankStatementImport,
}

impl BankProviderDomainRecords {
    pub fn account(&self) -> &BankAccount {
        &self.account
    }

    pub fn statement(&self) -> &BankStatementImport {
        &self.statement
    }

    pub fn into_parts(self) -> (BankAccount, BankStatementImport) {
        (self.account, self.statement)
    }

    /// Re-run the exact domain checks immediately before handing these values
    /// to a SQLx write boundary. Provider DTO validation is necessary but is
    /// not authoritative for persistence: the domain aggregate and its
    /// account binding must still be valid at the hand-off point.
    pub fn validate_for_persistence(&self) -> Result<(), BankProviderError> {
        self.account
            .validate()
            .map_err(|_| BankProviderError::DomainMappingRejected)?;
        self.statement
            .validate_against_account(&self.account)
            .map_err(|_| BankProviderError::DomainMappingRejected)
    }
}

/// Injected provider seam.  Implementations may perform network I/O outside
/// this module, but must translate the response to the fixed DTOs.  Callers
/// should use `fetch_validated_statement_page`, which enforces provider and
/// request/page binding before accepting output.
#[async_trait]
pub trait BankProviderAdapter: Send + Sync {
    fn provider_key(&self) -> &str;

    /// Return fresh evidence from the certificate-validating transport. The
    /// default is intentionally fail-closed so an adapter cannot accidentally
    /// introduce plaintext or unverified TLS communication.
    fn tls_evidence(&self) -> Result<BankProviderTlsEvidence, BankProviderError> {
        Err(BankProviderError::TransportSecurityRejected)
    }

    async fn fetch_statement_page(
        &self,
        request: &BankStatementPageRequest,
    ) -> Result<BankStatementPage, BankProviderError>;

    async fn fetch_validated_statement_page(
        &self,
        request: &BankStatementPageRequest,
    ) -> Result<BankStatementPage, BankProviderError> {
        request.validate_for_provider(self.provider_key())?;
        self.tls_evidence()?.validate_at(Utc::now())?;
        let page = self.fetch_statement_page(request).await?;
        page.validate_for_request(request)?;
        Ok(page)
    }
}

fn validate_provider_key(value: &str) -> Result<(), BankProviderError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_NAME_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        Err(BankProviderError::InvalidInput)
    } else {
        Ok(())
    }
}

fn is_valid_server_name(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_SERVER_NAME_BYTES
        || value.starts_with('.')
        || value.ends_with('.')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn validate_account_binding_scope(
    scope: &AuthorityScope,
    local_account_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
) -> Result<(), BankProviderError> {
    if local_account_id.is_nil() || holding_id.is_nil() || company_id.is_nil() {
        return Err(BankProviderError::InvalidInput);
    }
    let matches_scope = match scope {
        AuthorityScope::Platform => false,
        AuthorityScope::Holding {
            holding_id: scoped_holding,
        } => scoped_holding == &holding_id,
        AuthorityScope::Company {
            holding_id: scoped_holding,
            company_id: scoped_company,
        }
        | AuthorityScope::Branch {
            holding_id: scoped_holding,
            company_id: scoped_company,
            ..
        } => scoped_holding == &holding_id && scoped_company == &company_id,
        AuthorityScope::Subtree {
            holding_id: scoped_holding,
            organization_node_id,
            ..
        } => scoped_holding == &holding_id && organization_node_id == &company_id,
    };
    if matches_scope {
        Ok(())
    } else {
        Err(BankProviderError::InvalidInput)
    }
}

fn validate_opaque_reference(value: &str, max: usize) -> Result<(), BankProviderError> {
    let lowercase = value.to_ascii_lowercase();
    let credential_markers = [
        "access_token",
        "refresh_token",
        "api_key",
        "apikey",
        "bearer",
        "password",
        "secret",
        "token",
        "card",
        "pan",
        "cvv",
    ];
    if value.is_empty()
        || value.len() > max
        || credential_markers
            .iter()
            .any(|marker| lowercase.contains(marker))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
    {
        Err(BankProviderError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_text(value: &str, max: usize) -> Result<(), BankProviderError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(BankProviderError::InvalidResponse)
    } else {
        Ok(())
    }
}

fn is_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn deterministic_transaction_id(account_id: Uuid, transaction_reference: &str) -> Uuid {
    let digest = Sha256::digest(
        format!("erpx/finance/banking/{account_id}/{transaction_reference}").as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Preserve UUID's standard variant/version bits while retaining a
    // deterministic, collision-resistant value for the typed source fact.
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_account() -> BankProviderAccountRef {
        BankProviderAccountRef::try_new("acme_bank", "account-123").expect("provider account")
    }

    #[test]
    fn provider_account_binding_requires_consistent_scope_identity() {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let scope = AuthorityScope::Company {
            holding_id,
            company_id,
        };

        assert!(
            BankProviderAccountBinding::try_new(
                Uuid::from_u128(3),
                provider_account(),
                scope.clone(),
                holding_id,
                company_id,
                "USD",
                "Operating account",
            )
            .is_ok()
        );
        assert!(
            BankProviderAccountBinding::try_new(
                Uuid::from_u128(3),
                provider_account(),
                scope.clone(),
                Uuid::from_u128(4),
                company_id,
                "USD",
                "Operating account",
            )
            .is_err()
        );
        assert!(
            BankProviderAccountBinding::try_new(
                Uuid::from_u128(3),
                provider_account(),
                scope,
                holding_id,
                Uuid::from_u128(5),
                "USD",
                "Operating account",
            )
            .is_err()
        );
    }

    #[test]
    fn provider_account_binding_rejects_nil_or_platform_identity() {
        let account = provider_account();
        assert!(
            BankProviderAccountBinding::try_new(
                Uuid::nil(),
                account.clone(),
                AuthorityScope::Platform,
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                "USD",
                "Operating account",
            )
            .is_err()
        );
        assert!(
            BankProviderAccountBinding::try_new(
                Uuid::from_u128(3),
                account,
                AuthorityScope::Platform,
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                "USD",
                "Operating account",
            )
            .is_err()
        );
    }
}
