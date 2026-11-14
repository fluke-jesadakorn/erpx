//! Native PMS public-booking link and private-mail orchestration.
//!
//! Composition supplies the current Account/purchase authority, the runtime
//! secret, the public origin, the provider transport and the database unit. The
//! sealed mail payload is a cipher port so no key material is stored, printed
//! or asserted here. Retained behavior preserved: one signed seven-day link per
//! mail, a 120s delivery lease with `FOR UPDATE SKIP LOCKED`, attempt-preserving
//! package refusals, exponential retry after eight attempts, and a public
//! request bucket that fails only above the limit.
use crate::application::room_board::PmsBoardFuture;
use crate::domain::booking::{
    DELIVERY_BATCH, DEFAULT_RATE_LIMIT, PmsBookingEmailClaim, PmsBookingEmailRow, PmsBookingMailFacts,
    PmsBookingMailKind, PmsBookingMailPayload, PmsBookingQueuedMail, PmsBookingTokenRef,
    PmsBookingTokenRow, TOKEN_TTL_MS, booking_link_token, booking_mail_payload,
    booking_public_url, booking_summary, booking_token_hash, delivery_failure_outcome,
    mail_lease_deadline, rate_limit_bucket, rate_limit_exceeded, rate_limited, record_invalid,
    split_booking_link, stale_rate_limit_bucket, validate_presented_token,
    PmsBookingLinkKind,
};
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeMap;

/// Runtime delivery configuration. The secret is private runtime material and
/// is never debug-printed or serialized by this module.
pub struct PmsBookingMailConfig {
    secret: String,
    from: String,
    origin: String,
    holding_origins: BTreeMap<String, String>,
}

impl PmsBookingMailConfig {
    pub fn new(secret: String, from: String, origin: String) -> Self {
        Self {
            secret,
            from,
            origin,
            holding_origins: BTreeMap::new(),
        }
    }

    /// Registered Holding origins, keyed by Holding slug. The retained routing
    /// prefers the exact registered Holding public URL.
    pub fn with_holding_origins(mut self, origins: BTreeMap<String, String>) -> Self {
        self.holding_origins = origins;
        self
    }

    fn public_url(&self, slug: &str, suffix: &str) -> String {
        booking_public_url(
            &self.origin,
            self.holding_origins.get(slug).map(String::as_str),
            slug,
            suffix,
        )
    }
}

/// At-rest private-mail cipher. Composition owns the key derivation and AEAD
/// implementation; the owner module never stores plaintext or logs the secret.
///
/// The retained stored envelope is the existing TypeScript format so a mail
/// queued by either runtime is deliverable by the other:
/// `{"iv":"<b64>","body":"<b64 ciphertext+tag>","tag":"<b64>"}`, AES-256-GCM,
/// key `sha256("pms-mail:" + BETTER_AUTH_SECRET)`, 12-byte random IV and the
/// mail id as AAD.
pub trait PmsBookingMailCipher: Send + Sync {
    fn protect(&self, mail_id: &str, plaintext: &str) -> Result<String, BusinessError>;
    fn unprotect(&self, mail_id: &str, sealed: &str) -> Result<String, BusinessError>;
}

/// Provider transport (retained Resend POST). The stable idempotency key lets a
/// retried attempt reuse the provider's deduplication window.
pub trait PmsBookingMailProvider: Send + Sync {
    fn send<'a>(
        &'a self,
        idempotency_key: &'a str,
        body: &'a str,
    ) -> PmsBoardFuture<'a, (), BusinessError>;
}

/// Commercial package authority for the mail's property. A refusal pauses the
/// mail without consuming its attempt budget.
pub trait PmsBookingPurchaseGuard: Send + Sync {
    fn require_history<'a>(
        &'a self,
        property_id: &'a str,
    ) -> PmsBoardFuture<'a, (), BusinessError>;
}

/// PMS-owned persistence used by link issuing, delivery turns and the public
/// request bucket. The caller owns the transaction; this trait never commits.
pub trait PmsBookingMailRepository: Send {
    type Error: From<BusinessError> + Send;

    fn load_mail_facts<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBookingMailFacts, Self::Error>;

    fn insert_token<'a>(
        &'a mut self,
        row: &'a PmsBookingTokenRow,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    fn insert_email<'a>(
        &'a mut self,
        row: &'a PmsBookingEmailRow,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    /// Claim the oldest pending or lease-expired mail for one bounded turn.
    fn claim_next_email<'a>(
        &'a mut self,
        lease_until: i64,
        now: i64,
    ) -> PmsBoardFuture<'a, Option<PmsBookingEmailClaim>, Self::Error>;

    fn renew_token<'a>(
        &'a mut self,
        token_id: &'a str,
        expires_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    fn mark_sent<'a>(&'a mut self, id: &'a str, now: i64) -> PmsBoardFuture<'a, (), Self::Error>;

    fn mark_retry<'a>(
        &'a mut self,
        id: &'a str,
        state: &'a str,
        next_attempt_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    fn mark_blocked<'a>(
        &'a mut self,
        id: &'a str,
        code: &'a str,
        next_attempt_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    fn prune_rate_limits<'a>(
        &'a mut self,
        oldest_bucket: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error>;

    /// Increment and return the current count for this hashed key and bucket.
    fn bump_rate_limit<'a>(
        &'a mut self,
        key_hash: &'a str,
        bucket: i64,
    ) -> PmsBoardFuture<'a, i64, Self::Error>;

    fn read_access_token<'a>(
        &'a mut self,
        token_hash: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, Option<PmsBookingTokenRef>, Self::Error>;
}

/// Issue one private management link and queue its mail payload.
///
/// Both rows are written together by the caller's unit. `token_id` and
/// `email_id` are supplied by composition (retained `randomUUID`).
#[allow(clippy::too_many_arguments)]
pub async fn queue_booking_mail<R, C>(
    repository: &mut R,
    cipher: &C,
    config: &PmsBookingMailConfig,
    property_id: &str,
    reservation_id: &str,
    holding_slug: &str,
    kind: PmsBookingMailKind,
    token_id: &str,
    email_id: &str,
    now: i64,
) -> Result<PmsBookingQueuedMail, R::Error>
where
    R: PmsBookingMailRepository,
    C: PmsBookingMailCipher + ?Sized,
{
    if token_id.is_empty() || email_id.is_empty() || holding_slug.is_empty() {
        return Err(record_invalid().into());
    }
    let facts = repository
        .load_mail_facts(property_id, reservation_id)
        .await?;
    if facts.property.id != property_id || facts.reservation.id != reservation_id {
        return Err(record_invalid().into());
    }
    let summary = booking_summary(&facts, now)?;
    let token = booking_link_token(&config.secret, PmsBookingLinkKind::Access, token_id);
    let link = config.public_url(
        holding_slug,
        &format!("/manage#token={token}"),
    );
    let payload = booking_mail_payload(kind, &config.from, &summary, &link);
    let plaintext = serde_json::to_string(&payload).map_err(|_| record_invalid())?;
    let sealed = cipher.protect(email_id, &plaintext)?;
    let expires_at = now.saturating_add(TOKEN_TTL_MS);
    repository
        .insert_token(&PmsBookingTokenRow {
            id: token_id.to_owned(),
            property_id: property_id.to_owned(),
            reservation_id: reservation_id.to_owned(),
            token_hash: booking_token_hash(&token),
            expires_at,
            created_at: now,
        })
        .await?;
    repository
        .insert_email(&PmsBookingEmailRow {
            id: email_id.to_owned(),
            property_id: property_id.to_owned(),
            reservation_id: reservation_id.to_owned(),
            token_id: token_id.to_owned(),
            holding_slug: holding_slug.to_owned(),
            kind,
            payload_json: sealed.clone(),
            next_attempt_at: now,
            created_at: now,
        })
        .await?;
    Ok(PmsBookingQueuedMail {
        token,
        token_id: token_id.to_owned(),
        email_id: email_id.to_owned(),
        expires_at,
        payload_json: sealed,
    })
}

/// Outcome counters for one bounded delivery turn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PmsBookingDeliveryReport {
    pub sent: usize,
    pub retried: usize,
    pub blocked: usize,
}

/// Deliver at most ten queued mails. Every claimed mail is settled exactly once
/// for this turn; a crash after provider acceptance and before `sent` reuses the
/// provider idempotency key on the next lease.
pub async fn deliver_booking_emails<R, P, G, C>(
    repository: &mut R,
    provider: &P,
    guard: &G,
    cipher: &C,
    configured: bool,
    now: i64,
) -> Result<PmsBookingDeliveryReport, R::Error>
where
    R: PmsBookingMailRepository,
    P: PmsBookingMailProvider + ?Sized,
    G: PmsBookingPurchaseGuard + ?Sized,
    C: PmsBookingMailCipher + ?Sized,
{
    // Retained early return: an unconfigured Resend key/from never touches the
    // queue, so mails stay pending until delivery is configured.
    if !configured {
        return Ok(PmsBookingDeliveryReport::default());
    }
    repository
        .prune_rate_limits(stale_rate_limit_bucket(now))
        .await?;
    let mut report = PmsBookingDeliveryReport::default();
    for _ in 0..DELIVERY_BATCH {
        let Some(claim) = repository
            .claim_next_email(mail_lease_deadline(now), now)
            .await?
        else {
            break;
        };
        // Keep the provider body stable across retries even if staff edit a
        // guest later: the link token is renewed before the plaintext read.
        // Repository failures abort the turn; only an authority, cipher or
        // provider refusal consumes or pauses the mail attempt.
        let mut failure: Option<BusinessError> = None;
        match guard.require_history(&claim.property_id).await {
            Ok(()) => {
                repository
                    .renew_token(&claim.token_id, now.saturating_add(TOKEN_TTL_MS))
                    .await?;
                match cipher.unprotect(&claim.id, &claim.payload_json) {
                    Ok(plaintext) => {
                        if let Err(error) = provider
                            .send(&format!("pms-booking-{}", claim.id), &plaintext)
                            .await
                        {
                            failure = Some(error);
                        }
                    }
                    Err(error) => failure = Some(error),
                }
            }
            Err(error) => failure = Some(error),
        }
        match failure {
            None => {
                repository.mark_sent(&claim.id, now).await?;
                report.sent += 1;
            }
            Some(error) => match delivery_failure_outcome(claim.attempts, Some(error.code), now) {
                crate::domain::booking::PmsBookingDeliveryOutcome::Blocked {
                    code,
                    next_attempt_at,
                } => {
                    repository
                        .mark_blocked(&claim.id, &code, next_attempt_at)
                        .await?;
                    report.blocked += 1;
                }
                crate::domain::booking::PmsBookingDeliveryOutcome::Retry {
                    state,
                    next_attempt_at,
                } => {
                    repository
                        .mark_retry(&claim.id, state, next_attempt_at)
                        .await?;
                    report.retried += 1;
                }
                crate::domain::booking::PmsBookingDeliveryOutcome::Sent => {}
            },
        }
    }
    Ok(report)
}

/// Resolve a presented management link. Only the hash is looked up; the
/// returned identity grants nothing until the package guard passes.
pub async fn authorize_booking_link<R, G>(
    repository: &mut R,
    guard: &G,
    token: &str,
    now: i64,
) -> Result<PmsBookingTokenRef, R::Error>
where
    R: PmsBookingMailRepository,
    G: PmsBookingPurchaseGuard + ?Sized,
{
    validate_presented_token(token)?;
    // Reject a payment signature presented as a management link.
    split_booking_link(PmsBookingLinkKind::Access, token)?;
    let hash = booking_token_hash(token);
    let Some(reference) = repository.read_access_token(&hash, now).await? else {
        return Err(crate::domain::booking::link_expired().into());
    };
    guard.require_history(&reference.property_id).await?;
    Ok(reference)
}

/// Retained public request bucket. The key is the caller's bounded identity
/// material (hashed here); no raw IP or email is stored.
pub async fn booking_rate_limit<R>(
    repository: &mut R,
    key: &str,
    limit: Option<i64>,
    now: i64,
) -> Result<(), R::Error>
where
    R: PmsBookingMailRepository,
{
    // Retained default parameter: an omitted limit is 30, an explicit limit is
    // used verbatim (an explicit 0 refuses the first request).
    let limit = limit.unwrap_or(DEFAULT_RATE_LIMIT);
    let count = repository
        .bump_rate_limit(&booking_token_hash(key), rate_limit_bucket(now))
        .await?;
    if rate_limit_exceeded(count, limit) {
        return Err(rate_limited().into());
    }
    Ok(())
}

/// Retained `bookingSummary` read used by the management page and the private
/// mail. Exposed for composition; it is a projection, not a receipt.
pub async fn booking_management_summary<R>(
    repository: &mut R,
    property_id: &str,
    reservation_id: &str,
    now: i64,
) -> Result<crate::domain::booking::PmsBookingSummary, R::Error>
where
    R: PmsBookingMailRepository,
{
    let facts = repository
        .load_mail_facts(property_id, reservation_id)
        .await?;
    Ok(booking_summary(&facts, now)?)
}

/// Retained provider idempotency key for one queued mail.
pub fn booking_mail_idempotency_key(mail_id: &str) -> String {
    format!("pms-booking-{mail_id}")
}

/// Retained payload serialization helper used by the provider adapter contract.
pub fn booking_mail_body(payload: &PmsBookingMailPayload) -> Result<String, BusinessError> {
    serde_json::to_string(payload).map_err(|_| record_invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::booking::{PmsBookingDeliveryOutcome, delivery_retry_outcome};
    use crate::domain::booking::{PmsBookingEmailClaim, PmsBookingSummary};
    use crate::domain::room_board::{PmsPropertyRow, PmsRecord};
    use erpx_server_shared::public::FailureKind;
    use serde_json::{Value, json};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeRepository {
        facts: Option<PmsBookingMailFacts>,
        tokens: Vec<PmsBookingTokenRow>,
        emails: Vec<PmsBookingEmailRow>,
        claims: VecDeque<PmsBookingEmailClaim>,
        sent: Vec<String>,
        retries: Vec<(String, String, i64)>,
        blocked: Vec<(String, String, i64)>,
        renewed: Vec<(String, i64)>,
        pruned: Vec<i64>,
        bumps: Vec<(String, i64)>,
        buckets: BTreeMap<i64, i64>,
        access: Option<PmsBookingTokenRef>,
        access_hash: Option<String>,
    }

    impl PmsBookingMailRepository for FakeRepository {
        type Error = BusinessError;

        fn load_mail_facts<'a>(
            &'a mut self,
            _property_id: &'a str,
            _reservation_id: &'a str,
        ) -> PmsBoardFuture<'a, PmsBookingMailFacts, Self::Error> {
            Box::pin(async move { self.facts.clone().ok_or_else(record_invalid) })
        }

        fn insert_token<'a>(
            &'a mut self,
            row: &'a PmsBookingTokenRow,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.tokens.push(row.clone());
            Box::pin(async { Ok(()) })
        }

        fn insert_email<'a>(
            &'a mut self,
            row: &'a PmsBookingEmailRow,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.emails.push(row.clone());
            Box::pin(async { Ok(()) })
        }

        fn claim_next_email<'a>(
            &'a mut self,
            _lease_until: i64,
            _now: i64,
        ) -> PmsBoardFuture<'a, Option<PmsBookingEmailClaim>, Self::Error> {
            let claim = self.claims.pop_front();
            Box::pin(async move { Ok(claim) })
        }

        fn renew_token<'a>(
            &'a mut self,
            token_id: &'a str,
            expires_at: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.renewed.push((token_id.to_owned(), expires_at));
            Box::pin(async { Ok(()) })
        }

        fn mark_sent<'a>(
            &'a mut self,
            id: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.sent.push(id.to_owned());
            Box::pin(async { Ok(()) })
        }

        fn mark_retry<'a>(
            &'a mut self,
            id: &'a str,
            state: &'a str,
            next_attempt_at: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.retries
                .push((id.to_owned(), state.to_owned(), next_attempt_at));
            Box::pin(async { Ok(()) })
        }

        fn mark_blocked<'a>(
            &'a mut self,
            id: &'a str,
            code: &'a str,
            next_attempt_at: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.blocked
                .push((id.to_owned(), code.to_owned(), next_attempt_at));
            Box::pin(async { Ok(()) })
        }

        fn prune_rate_limits<'a>(
            &'a mut self,
            oldest_bucket: i64,
        ) -> PmsBoardFuture<'a, (), Self::Error> {
            self.pruned.push(oldest_bucket);
            Box::pin(async { Ok(()) })
        }

        fn bump_rate_limit<'a>(
            &'a mut self,
            key_hash: &'a str,
            bucket: i64,
        ) -> PmsBoardFuture<'a, i64, Self::Error> {
            self.bumps.push((key_hash.to_owned(), bucket));
            let count = self.buckets.entry(bucket).or_insert(0);
            *count += 1;
            let count = *count;
            Box::pin(async move { Ok(count) })
        }

        fn read_access_token<'a>(
            &'a mut self,
            token_hash: &'a str,
            _now: i64,
        ) -> PmsBoardFuture<'a, Option<PmsBookingTokenRef>, Self::Error> {
            let access = self
                .access
                .clone()
                .filter(|_| self.access_hash.as_deref() == Some(token_hash));
            Box::pin(async move { Ok(access) })
        }
    }

    struct ReverseCipher;
    impl PmsBookingMailCipher for ReverseCipher {
        fn protect(&self, _mail_id: &str, plaintext: &str) -> Result<String, BusinessError> {
            Ok(format!("sealed:{}", plaintext.chars().rev().collect::<String>()))
        }
        fn unprotect(&self, _mail_id: &str, sealed: &str) -> Result<String, BusinessError> {
            sealed
                .strip_prefix("sealed:")
                .map(|plain| plain.chars().rev().collect())
                .ok_or_else(record_invalid)
        }
    }

    struct FakeProvider {
        failure: Option<&'static str>,
        sent: std::sync::Mutex<Vec<(String, String)>>,
    }
    impl PmsBookingMailProvider for FakeProvider {
        fn send<'a>(
            &'a self,
            idempotency_key: &'a str,
            body: &'a str,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            self.sent
                .lock()
                .unwrap()
                .push((idempotency_key.to_owned(), body.to_owned()));
            let failure = self.failure;
            Box::pin(async move {
                match failure {
                    Some(code) => Err(BusinessError::new(FailureKind::Unavailable, code, "boom")),
                    None => Ok(()),
                }
            })
        }
    }

    struct FakeGuard(Option<&'static str>);
    impl PmsBookingPurchaseGuard for FakeGuard {
        fn require_history<'a>(
            &'a self,
            _property_id: &'a str,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            let failure = self.0;
            Box::pin(async move {
                match failure {
                    Some(code) => Err(BusinessError::new(FailureKind::Forbidden, code, "blocked")),
                    None => Ok(()),
                }
            })
        }
    }

    fn property() -> PmsPropertyRow {
        PmsPropertyRow {
            id: "property".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            name: "Riverside".into(),
            business_date: "2026-11-04".into(),
            state: "open".into(),
            config_json: json!({
                "currency": "THB",
                "booking": {"address": "1 River", "contactEmail": "stay@example.com", "contactPhone": "+66"}
            })
            .to_string(),
            version: 1,
        }
    }

    fn record(id: &str, kind: &str, data: Value) -> PmsRecord {
        PmsRecord {
            id: id.into(),
            kind: kind.into(),
            version: 1,
            updated_at: 1,
            data,
        }
    }

    fn facts() -> PmsBookingMailFacts {
        PmsBookingMailFacts {
            property: property(),
            reservation: record(
                "reservation-1",
                "reservation",
                json!({"number": "BK-1", "state": "confirmed", "guestId": "guest-1", "bookingPolicy": {"deadline": 9_000, "text": "Free cancellation"}}),
            ),
            guest: record(
                "guest-1",
                "guest",
                json!({"name": "Ada", "email": "ada@example.com"}),
            ),
            room_types: vec![record("type-1", "room_type", json!({"name": "Deluxe"}))],
            stays: vec![record(
                "stay-1",
                "stay",
                json!({"reservationId": "reservation-1", "typeId": "type-1", "rateId": "rate-1", "guestAccountId": "account-1", "arrival": "2026-11-10", "departure": "2026-11-11", "adults": 2, "childAges": [], "state": "reserved", "prices": [{"total": 1_000}]}),
            )],
            accounts: vec![record(
                "account-1",
                "guest_account",
                json!({"reservationId": "reservation-1", "advance": 0, "due": 1_000}),
            )],
            entries: Vec::new(),
            refunds: Vec::new(),
        }
    }

    fn claim(id: &str) -> PmsBookingEmailClaim {
        PmsBookingEmailClaim {
            id: id.into(),
            property_id: "property".into(),
            reservation_id: "reservation-1".into(),
            token_id: "token-1".into(),
            holding_slug: "acme".into(),
            kind: "confirmation".into(),
            attempts: 1,
            payload_json: ReverseCipher.protect(id, "{\"from\":\"a\"}").unwrap(),
        }
    }

    fn config() -> PmsBookingMailConfig {
        PmsBookingMailConfig::new(
            "secret".into(),
            "stay@example.com".into(),
            "https://console-dev-erpx.jesadakorn.com".into(),
        )
        .with_holding_origins(BTreeMap::from([(
            "acme".to_owned(),
            "https://acme.jesadakorn.com".to_owned(),
        )]))
    }

    #[tokio::test]
    async fn queues_a_signed_private_link_and_mail() {
        let mut repository = FakeRepository {
            facts: Some(facts()),
            ..Default::default()
        };
        let queued = queue_booking_mail(
            &mut repository,
            &ReverseCipher,
            &config(),
            "property",
            "reservation-1",
            "acme",
            PmsBookingMailKind::Confirmation,
            "token-1",
            "mail-1",
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(queued.expires_at, 1_000 + TOKEN_TTL_MS);
        assert_eq!(repository.tokens.len(), 1);
        assert_eq!(repository.emails.len(), 1);
        assert_eq!(
            repository.tokens[0].token_hash,
            crate::domain::booking::booking_token_hash(&queued.token)
        );
        let sealed = repository.emails[0].payload_json.clone();
        let plain = ReverseCipher.unprotect("mail-1", &sealed).unwrap();
        let payload: Value = serde_json::from_str(&plain).unwrap();
        assert_eq!(payload["to"][0], "ada@example.com");
        assert!(
            payload["text"]
                .as_str()
                .unwrap()
                .contains("https://acme.jesadakorn.com/book/manage#token=")
        );
        assert!(payload["text"].as_str().unwrap().contains(&queued.token));
    }

    #[tokio::test]
    async fn delivery_settles_sent_retry_and_blocked() {
        let mut repository = FakeRepository {
            claims: VecDeque::from([claim("mail-1")]),
            ..Default::default()
        };
        let provider = FakeProvider {
            failure: None,
            sent: std::sync::Mutex::new(Vec::new()),
        };
        let report = deliver_booking_emails(
            &mut repository,
            &provider,
            &FakeGuard(None),
            &ReverseCipher,
            true,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(repository.sent, vec!["mail-1"]);
        assert_eq!(repository.renewed, vec![("token-1".to_owned(), 1_000 + TOKEN_TTL_MS)]);
        assert_eq!(repository.pruned, vec![-60]);

        let mut repository = FakeRepository {
            claims: VecDeque::from([claim("mail-2")]),
            ..Default::default()
        };
        let provider = FakeProvider {
            failure: Some("EMAIL_DELIVERY_FAILED"),
            sent: std::sync::Mutex::new(Vec::new()),
        };
        let report = deliver_booking_emails(
            &mut repository,
            &provider,
            &FakeGuard(None),
            &ReverseCipher,
            true,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(report.retried, 1);
        assert_eq!(
            repository.retries,
            vec![("mail-2".to_owned(), "pending".to_owned(), 3_000)]
        );

        let mut repository = FakeRepository {
            claims: VecDeque::from([claim("mail-3")]),
            ..Default::default()
        };
        let report = deliver_booking_emails(
            &mut repository,
            &FakeProvider {
                failure: None,
                sent: std::sync::Mutex::new(Vec::new()),
            },
            &FakeGuard(Some("PACKAGE_REQUIRED")),
            &ReverseCipher,
            true,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(report.blocked, 1);
        assert_eq!(
            repository.blocked,
            vec![("mail-3".to_owned(), "PACKAGE_REQUIRED".to_owned(), 61_000)]
        );
        assert!(repository.sent.is_empty());
        assert!(repository.renewed.is_empty());

        assert_eq!(
            delivery_retry_outcome(8, 1_000),
            PmsBookingDeliveryOutcome::Retry {
                state: "failed",
                next_attempt_at: 1_000 + 256_000
            }
        );

        // An unconfigured provider leaves the queue untouched.
        let mut repository = FakeRepository {
            claims: VecDeque::from([claim("mail-4")]),
            ..Default::default()
        };
        let report = deliver_booking_emails(
            &mut repository,
            &FakeProvider {
                failure: None,
                sent: std::sync::Mutex::new(Vec::new()),
            },
            &FakeGuard(None),
            &ReverseCipher,
            false,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(report, PmsBookingDeliveryReport::default());
        assert!(repository.pruned.is_empty());
        assert_eq!(repository.claims.len(), 1);
    }

    #[tokio::test]
    async fn delivery_stops_at_the_retained_batch_bound() {
        let mut repository = FakeRepository {
            claims: (0..25).map(|index| claim(&format!("mail-{index}"))).collect(),
            ..Default::default()
        };
        let report = deliver_booking_emails(
            &mut repository,
            &FakeProvider {
                failure: None,
                sent: std::sync::Mutex::new(Vec::new()),
            },
            &FakeGuard(None),
            &ReverseCipher,
            true,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(report.sent, DELIVERY_BATCH);
        assert_eq!(repository.claims.len(), 15);
    }

    #[tokio::test]
    async fn authorization_requires_the_access_audience_and_a_token() {
        let token = booking_link_token("secret", PmsBookingLinkKind::Access, "token-1");
        let mut repository = FakeRepository {
            access: Some(PmsBookingTokenRef {
                property_id: "property".into(),
                reservation_id: "reservation-1".into(),
            }),
            access_hash: Some(crate::domain::booking::booking_token_hash(&token)),
            ..Default::default()
        };
        let reference = authorize_booking_link(&mut repository, &FakeGuard(None), &token, 1_000)
            .await
            .unwrap();
        assert_eq!(reference.reservation_id, "reservation-1");

        let payment = booking_link_token("secret", PmsBookingLinkKind::Payment, "token-1");
        assert_eq!(
            authorize_booking_link(&mut repository, &FakeGuard(None), &payment, 1_000)
                .await
                .unwrap_err()
                .code,
            "PMS_LINK_EXPIRED"
        );
        let mut empty = FakeRepository::default();
        assert_eq!(
            authorize_booking_link(&mut empty, &FakeGuard(None), &token, 1_000)
                .await
                .unwrap_err()
                .code,
            "PMS_LINK_EXPIRED"
        );
        assert_eq!(
            authorize_booking_link(&mut repository, &FakeGuard(None), &"x".repeat(201), 1_000)
                .await
                .unwrap_err()
                .code,
            "PMS_LINK_EXPIRED"
        );
    }

    #[tokio::test]
    async fn rate_limit_fails_only_above_the_limit() {
        let mut repository = FakeRepository::default();
        for _ in 0..3 {
            booking_rate_limit(&mut repository, "ip:1.2.3.4", Some(3), 1_000)
                .await
                .unwrap();
        }
        assert_eq!(
            booking_rate_limit(&mut repository, "ip:1.2.3.4", Some(3), 1_000)
                .await
                .unwrap_err()
                .code,
            "PMS_RATE_LIMIT"
        );
        assert_eq!(repository.bumps[0].1, 0);
        assert_eq!(
            repository.bumps[0].0,
            crate::domain::booking::booking_token_hash("ip:1.2.3.4")
        );
        // A new window starts a fresh bucket.
        booking_rate_limit(&mut repository, "ip:1.2.3.4", Some(1), 60_000)
            .await
            .unwrap();
        assert_eq!(repository.bumps.last().unwrap().1, 1);
    }

    #[tokio::test]
    async fn summary_read_does_not_invent_authority() {
        let mut repository = FakeRepository {
            facts: Some(facts()),
            ..Default::default()
        };
        let summary: PmsBookingSummary =
            booking_management_summary(&mut repository, "property", "reservation-1", 1_000)
                .await
                .unwrap();
        assert_eq!(summary.number, "BK-1");
        assert_eq!(booking_mail_idempotency_key("mail-1"), "pms-booking-mail-1");
        assert!(booking_mail_body(&PmsBookingMailPayload {
            from: "a".into(),
            to: vec!["b".into()],
            subject: "c".into(),
            text: "d".into(),
        })
        .unwrap()
        .contains("\"subject\":\"c\""));
    }
}
