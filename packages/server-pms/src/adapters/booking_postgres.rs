//! PostgreSQL adapter for the native PMS booking link and private-mail slice.
//!
//! Only PMS-owned rows are read or written: `pms_properties`, `pms_records`,
//! `pms_booking_tokens`, `pms_booking_emails` and `pms_booking_limits`. The
//! caller owns the transaction; this adapter never commits, retries or mints
//! authority. The delivery claim is the retained single-statement
//! `FOR UPDATE SKIP LOCKED` lease.
use crate::application::booking::PmsBookingMailRepository;
use crate::domain::booking::PmsBookingQueuedMail;
use crate::application::room_board::PmsBoardFuture;
use crate::domain::booking::{
    MAX_SUMMARY_ACCOUNTS, MAX_SUMMARY_ENTRIES, MAX_SUMMARY_ROOMS, PmsBookingEmailClaim,
    PmsBookingEmailRow, PmsBookingMailFacts, PmsBookingMailKind, PmsBookingTokenRef,
    PmsBookingTokenRow, read_limit, record_invalid,
};
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

const MAX_ROOM_TYPES: usize = 500;

fn property_row(row: &PgRow) -> Result<PmsPropertyRow, Error> {
    Ok(PmsPropertyRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        name: row.try_get("name")?,
        business_date: row.try_get("business_date")?,
        state: row.try_get("state")?,
        config_json: row.try_get("config_json")?,
        version: row.try_get("version")?,
    })
}

fn record_row(row: &PgRow) -> Result<PmsRecord, Error> {
    let raw: String = row.try_get("data_json")?;
    let data: Value = serde_json::from_str(&raw).map_err(|_| Error::from(record_invalid()))?;
    Ok(PmsRecord {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        version: row.try_get("version")?,
        updated_at: row.try_get("updated_at")?,
        data,
    })
}

/// PMS-owned repository bound to one caller-owned connection.
pub struct PgPmsBookingMailRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsBookingMailRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn property(
        &mut self,
        property_id: &str,
    ) -> Result<PmsPropertyRow, Error> {
        let row = sqlx::query(
            "SELECT id,company_id,branch_id,name,business_date,state,config_json,version \
             FROM pms_properties WHERE id=$1",
        )
        .bind(property_id)
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(|| Error::from(not_found()))?;
        property_row(&row)
    }

    async fn record(
        &mut self,
        property_id: &str,
        id: &str,
        kind: &str,
    ) -> Result<Option<PmsRecord>, Error> {
        let row = sqlx::query(
            "SELECT id,kind,data_json,version,updated_at FROM pms_records \
             WHERE property_id=$1 AND id=$2 AND kind=$3",
        )
        .bind(property_id)
        .bind(id)
        .bind(kind)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.as_ref().map(record_row).transpose()
    }

    async fn records(
        &mut self,
        sql: &'static str,
        property_id: &str,
        value: &str,
        limit: usize,
        code: &'static str,
    ) -> Result<Vec<PmsRecord>, Error> {
        let rows = sqlx::query(sql)
            .bind(property_id)
            .bind(value)
            .bind(limit as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        if rows.len() > limit {
            return Err(Error::from(read_limit(code)));
        }
        rows.iter().map(record_row).collect()
    }

    async fn records_bounded(
        &mut self,
        sql: &'static str,
        property_id: &str,
        limit: usize,
        code: &'static str,
    ) -> Result<Vec<PmsRecord>, Error> {
        let rows = sqlx::query(sql)
            .bind(property_id)
            .bind(limit as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        if rows.len() > limit {
            return Err(Error::from(read_limit(code)));
        }
        rows.iter().map(record_row).collect()
    }
}

impl PmsBookingMailRepository for PgPmsBookingMailRepository<'_> {
    type Error = Error;

    fn load_mail_facts<'a>(
        &'a mut self,
        property_id: &'a str,
        reservation_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBookingMailFacts, Self::Error> {
        Box::pin(async move {
            let property = self.property(property_id).await?;
            let reservation = self
                .record(property_id, reservation_id, "reservation")
                .await?
                .ok_or_else(|| Error::from(not_found()))?;
            let guest_id = reservation
                .data
                .get("guestId")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::from(record_invalid()))?;
            let guest = self
                .record(property_id, guest_id, "guest")
                .await?
                .ok_or_else(|| Error::from(record_invalid()))?;
            let stays = self
                .records(
                    "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                     WHERE property_id=$1 AND kind='stay' \
                     AND data_json::jsonb->>'reservationId'=$2 ORDER BY id LIMIT $3",
                    property_id,
                    reservation_id,
                    MAX_SUMMARY_ROOMS,
                    "The booking has more stays than this mail can read.",
                )
                .await?;
            let room_types = self
                .records_bounded(
                    "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                     WHERE property_id=$1 AND kind='room_type' ORDER BY id LIMIT $2",
                    property_id,
                    MAX_ROOM_TYPES,
                    "The hotel has more room types than this mail can read.",
                )
                .await?;
            let accounts = self
                .records(
                    "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                     WHERE property_id=$1 AND kind='guest_account' \
                     AND data_json::jsonb->>'reservationId'=$2 ORDER BY id LIMIT $3",
                    property_id,
                    reservation_id,
                    MAX_SUMMARY_ACCOUNTS,
                    "The booking has more guest accounts than this mail can read.",
                )
                .await?;
            let entries = self
                .records_bounded(
                    "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                     WHERE property_id=$1 AND kind='entry' ORDER BY id LIMIT $2",
                    property_id,
                    MAX_SUMMARY_ENTRIES,
                    "The hotel has more folio entries than this mail can read.",
                )
                .await?;
            let refunds = self
                .records_bounded(
                    "SELECT id,kind,data_json,version,updated_at FROM pms_records \
                     WHERE property_id=$1 AND kind='refund' ORDER BY id LIMIT $2",
                    property_id,
                    MAX_SUMMARY_ENTRIES,
                    "The hotel has more refunds than this mail can read.",
                )
                .await?;
            Ok(PmsBookingMailFacts {
                property,
                reservation,
                guest,
                room_types,
                stays,
                accounts,
                entries,
                refunds,
            })
        })
    }

    fn insert_token<'a>(
        &'a mut self,
        row: &'a PmsBookingTokenRow,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pms_booking_tokens(id,property_id,reservation_id,token_hash,expires_at,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6)",
            )
            .bind(&row.id)
            .bind(&row.property_id)
            .bind(&row.reservation_id)
            .bind(&row.token_hash)
            .bind(row.expires_at)
            .bind(row.created_at)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn insert_email<'a>(
        &'a mut self,
        row: &'a PmsBookingEmailRow,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pms_booking_emails(id,property_id,reservation_id,token_id,holding_slug,kind,payload_json,next_attempt_at,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(&row.id)
            .bind(&row.property_id)
            .bind(&row.reservation_id)
            .bind(&row.token_id)
            .bind(&row.holding_slug)
            .bind(row.kind.as_str())
            .bind(&row.payload_json)
            .bind(row.next_attempt_at)
            .bind(row.created_at)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn claim_next_email<'a>(
        &'a mut self,
        lease_until: i64,
        now: i64,
    ) -> PmsBoardFuture<'a, Option<PmsBookingEmailClaim>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "UPDATE pms_booking_emails SET state='sending',attempts=attempts+1,lease_until=$1 \
                 WHERE id=(SELECT id FROM pms_booking_emails \
                   WHERE (state='pending' OR (state='sending' AND lease_until<$2)) \
                   AND next_attempt_at<=$3 ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) \
                 RETURNING id,property_id,reservation_id,token_id,holding_slug,kind,attempts,payload_json",
            )
            .bind(lease_until)
            .bind(now)
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(PmsBookingEmailClaim {
                    id: row.try_get("id")?,
                    property_id: row.try_get("property_id")?,
                    reservation_id: row.try_get("reservation_id")?,
                    token_id: row.try_get("token_id")?,
                    holding_slug: row.try_get("holding_slug")?,
                    kind: row.try_get("kind")?,
                    attempts: i64::from(row.try_get::<i32, _>("attempts")?),
                    payload_json: row.try_get("payload_json")?,
                })
            })
            .transpose()
        })
    }

    fn renew_token<'a>(
        &'a mut self,
        token_id: &'a str,
        expires_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query("UPDATE pms_booking_tokens SET expires_at=$1 WHERE id=$2")
                .bind(expires_at)
                .bind(token_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn mark_sent<'a>(
        &'a mut self,
        id: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_emails SET state='sent',sent_at=$1,lease_until=NULL,last_error=NULL \
                 WHERE id=$2",
            )
            .bind(now)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn mark_retry<'a>(
        &'a mut self,
        id: &'a str,
        state: &'a str,
        next_attempt_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_emails SET state=$1,next_attempt_at=$2,lease_until=NULL,\
                 last_error='EMAIL_DELIVERY_FAILED' WHERE id=$3",
            )
            .bind(state)
            .bind(next_attempt_at)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn mark_blocked<'a>(
        &'a mut self,
        id: &'a str,
        code: &'a str,
        next_attempt_at: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE pms_booking_emails SET state='pending',attempts=GREATEST(0,attempts-1),\
                 lease_until=NULL,next_attempt_at=$1,last_error=$2 WHERE id=$3",
            )
            .bind(next_attempt_at)
            .bind(code)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn prune_rate_limits<'a>(
        &'a mut self,
        oldest_bucket: i64,
    ) -> PmsBoardFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query("DELETE FROM pms_booking_limits WHERE bucket<$1")
                .bind(oldest_bucket)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn bump_rate_limit<'a>(
        &'a mut self,
        key_hash: &'a str,
        bucket: i64,
    ) -> PmsBoardFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "INSERT INTO pms_booking_limits(key,bucket,count) VALUES($1,$2,1) \
                 ON CONFLICT(key) DO UPDATE SET \
                 count=CASE WHEN pms_booking_limits.bucket=excluded.bucket \
                   THEN pms_booking_limits.count+1 ELSE 1 END,bucket=excluded.bucket \
                 RETURNING count",
            )
            .bind(key_hash)
            .bind(bucket)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(i64::from(row.try_get::<i32, _>("count")?))
        })
    }

    fn read_access_token<'a>(
        &'a mut self,
        token_hash: &'a str,
        now: i64,
    ) -> PmsBoardFuture<'a, Option<PmsBookingTokenRef>, Self::Error> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT property_id,reservation_id FROM pms_booking_tokens \
                 WHERE token_hash=$1 AND expires_at>$2",
            )
            .bind(token_hash)
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(PmsBookingTokenRef {
                    property_id: row.try_get("property_id")?,
                    reservation_id: row.try_get("reservation_id")?,
                })
            })
            .transpose()
        })
    }
}

/// Queue a retained private link on the caller's connection. Convenience for
/// composition; the same behavior is available through the repository trait.
#[allow(clippy::too_many_arguments)]
pub async fn queue_booking_mail_on_postgres<C>(
    connection: &mut PgConnection,
    cipher: &C,
    config: &crate::application::booking::PmsBookingMailConfig,
    property_id: &str,
    reservation_id: &str,
    holding_slug: &str,
    kind: PmsBookingMailKind,
    token_id: &str,
    email_id: &str,
    now: i64,
) -> Result<PmsBookingQueuedMail, Error>
where
    C: crate::application::booking::PmsBookingMailCipher + ?Sized,
{
    let mut repository = PgPmsBookingMailRepository::new(connection);
    crate::application::booking::queue_booking_mail(
        &mut repository,
        cipher,
        config,
        property_id,
        reservation_id,
        holding_slug,
        kind,
        token_id,
        email_id,
        now,
    )
    .await
}

/// One retained `pmsWork` delivery turn over the caller's connection.
///
/// Composition mounts this from the Holding jobs runtime with the current
/// purchase guard, the Resend transport, the at-rest cipher and the pinned
/// clock. The turn returns the claim outcomes; it never retries the whole turn
/// or invents a provider result.
#[allow(clippy::too_many_arguments)]
pub async fn drain_booking_mail_turn<P, G, C>(
    connection: &mut PgConnection,
    provider: &P,
    guard: &G,
    cipher: &C,
    configured: bool,
    now: i64,
) -> Result<crate::application::booking::PmsBookingDeliveryReport, Error>
where
    P: crate::application::booking::PmsBookingMailProvider + ?Sized,
    G: crate::application::booking::PmsBookingPurchaseGuard + ?Sized,
    C: crate::application::booking::PmsBookingMailCipher + ?Sized,
{
    let mut repository = PgPmsBookingMailRepository::new(connection);
    crate::application::booking::deliver_booking_emails(
        &mut repository,
        provider,
        guard,
        cipher,
        configured,
        now,
    )
    .await
}

#[cfg(test)]
#[path = "../../../../tests/support/rust-postgres.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use super::fixture;
    use crate::application::booking::{
        PmsBookingMailCipher, PmsBookingMailConfig, PmsBookingPurchaseGuard, authorize_booking_link,
        booking_rate_limit, deliver_booking_emails, queue_booking_mail,
    };
    use crate::application::room_board::PmsBoardFuture;
    use crate::domain::booking::{
        PmsBookingLinkKind, TOKEN_TTL_MS, booking_token_hash, booking_link_token,
    };
    use erpx_server_runtime::public::postgres::{
        DatabasePool, MigrationTrack, load_migrations, migrate_database,
    };
    use serde_json::json;
    use sqlx::Row;
    use std::sync::Mutex;

    struct ReverseCipher;
    impl PmsBookingMailCipher for ReverseCipher {
        fn protect(&self, _mail_id: &str, plaintext: &str) -> Result<String, BusinessError> {
            // The database enforces valid JSON on `payload_json`; the real
            // composition cipher seals the plaintext into a JSON envelope too.
            Ok(json!({"sealed": plaintext.chars().rev().collect::<String>()}).to_string())
        }
        fn unprotect(&self, _mail_id: &str, sealed: &str) -> Result<String, BusinessError> {
            let value: Value = serde_json::from_str(sealed).map_err(|_| record_invalid())?;
            value
                .get("sealed")
                .and_then(Value::as_str)
                .map(|plain| plain.chars().rev().collect())
                .ok_or_else(record_invalid)
        }
    }

    struct NoopGuard;
    impl PmsBookingPurchaseGuard for NoopGuard {
        fn require_history<'a>(
            &'a self,
            _property_id: &'a str,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            Box::pin(async { Ok(()) })
        }
    }

    struct RecordingProvider(Mutex<Vec<(String, String)>>);
    impl crate::application::booking::PmsBookingMailProvider for RecordingProvider {
        fn send<'a>(
            &'a self,
            idempotency_key: &'a str,
            body: &'a str,
        ) -> PmsBoardFuture<'a, (), BusinessError> {
            self.0
                .lock()
                .unwrap()
                .push((idempotency_key.to_owned(), body.to_owned()));
            Box::pin(async { Ok(()) })
        }
    }

    async fn migrate(pool: &DatabasePool) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
            .await
            .unwrap();
    }

    fn config_json() -> String {
        json!({
            "country": "TH",
            "currency": "THB",
            "minorUnit": 2,
            "timezone": "Asia/Bangkok",
            "checkIn": "14:00",
            "checkOut": "12:00",
            "accounts": {"cash": "cash"},
            "booking": {
                "address": "1 River Road",
                "contactEmail": "stay@example.com",
                "contactPhone": "+66 2 000 0000"
            }
        })
        .to_string()
    }

    async fn seed(pool: &DatabasePool) {
        migrate(pool).await;
        sqlx::query(
            "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) \
             VALUES('property','company','branch','Riverside','2026-11-04','open',$1,1,'maker',1)",
        )
        .bind(config_json())
        .execute(pool.raw())
        .await
        .unwrap();
        for (id, kind, data) in [
            (
                "reservation-1",
                "reservation",
                json!({"number": "BK-1", "state": "confirmed", "guestId": "guest-1", "bookingPolicy": {"deadline": 9000, "text": "Free cancellation"}}),
            ),
            (
                "guest-1",
                "guest",
                json!({"name": "Ada", "email": "ada@example.com"}),
            ),
            ("type-1", "room_type", json!({"name": "Deluxe", "capacity": 2})),
            (
                "stay-1",
                "stay",
                json!({"reservationId": "reservation-1", "typeId": "type-1", "rateId": "rate-1", "guestAccountId": "account-1", "arrival": "2026-11-10", "departure": "2026-11-11", "adults": 2, "childAges": [], "state": "reserved", "prices": [{"total": 1000}]}),
            ),
            (
                "account-1",
                "guest_account",
                json!({"reservationId": "reservation-1", "advance": 0, "due": 1000}),
            ),
        ] {
            sqlx::query(
                "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES($1,'property',$2,$3,1,1)",
            )
            .bind(id)
            .bind(kind)
            .bind(data.to_string())
            .execute(pool.raw())
            .await
            .unwrap();
        }
    }

    async fn email_state(pool: &DatabasePool, id: &str) -> (String, i64, Option<i64>, Option<String>) {
        let row = sqlx::query(
            "SELECT state,attempts,lease_until,last_error FROM pms_booking_emails WHERE id=$1",
        )
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        (
            row.get("state"),
            i64::from(row.get::<i32, _>("attempts")),
            row.get("lease_until"),
            row.get("last_error"),
        )
    }

    #[tokio::test]
    async fn booking_mail_postgres_lifecycle() {
        fixture::with_database(2, |pool| async move {
            seed(&pool).await;
            let config = PmsBookingMailConfig::new(
                "secret".into(),
                "stay@example.com".into(),
                "https://console-dev-erpx.jesadakorn.com".into(),
            );
            let mut connection = pool.raw().acquire().await.unwrap();
            let mut repository = PgPmsBookingMailRepository::new(&mut connection);

            let queued = queue_booking_mail(
                &mut repository,
                &ReverseCipher,
                &config,
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
            assert_eq!(queued.token, booking_link_token("secret", PmsBookingLinkKind::Access, "token-1"));

            let token_row = sqlx::query("SELECT token_hash,expires_at FROM pms_booking_tokens WHERE id='token-1'")
                .fetch_one(&mut *repository.connection)
                .await
                .unwrap();
            assert_eq!(token_row.get::<String, _>("token_hash"), booking_token_hash(&queued.token));
            assert_eq!(token_row.get::<i64, _>("expires_at"), queued.expires_at);

            let reference = authorize_booking_link(&mut repository, &NoopGuard, &queued.token, 1_000)
                .await
                .unwrap();
            assert_eq!(reference.property_id, "property");
            assert_eq!(reference.reservation_id, "reservation-1");
            assert!(authorize_booking_link(&mut repository, &NoopGuard, &queued.token, 1_000 + TOKEN_TTL_MS).await.is_err());

            // First claim takes the lease and increments attempts.
            let claim = repository.claim_next_email(121_000, 1_000).await.unwrap().unwrap();
            assert_eq!(claim.id, "mail-1");
            assert_eq!(claim.attempts, 1);
            assert_eq!(email_state(&pool, "mail-1").await.0, "sending");
            // A live lease is never stolen and next_attempt_at has not arrived.
            assert!(repository.claim_next_email(121_000, 1_000).await.unwrap().is_none());

            // Package refusal returns the mail to pending without consuming the attempt.
            repository
                .mark_blocked("mail-1", "PACKAGE_REQUIRED", 61_000)
                .await
                .unwrap();
            let (state, attempts, lease, error) = email_state(&pool, "mail-1").await;
            assert_eq!((state.as_str(), attempts, lease), ("pending", 0, None));
            assert_eq!(error.as_deref(), Some("PACKAGE_REQUIRED"));

            // A due retry is claimed again, then a hard failure consumes the attempt.
            let claim = repository.claim_next_email(121_000, 61_000).await.unwrap().unwrap();
            assert_eq!(claim.attempts, 1);
            repository
                .mark_retry("mail-1", "pending", 63_000)
                .await
                .unwrap();
            let (state, attempts, lease, error) = email_state(&pool, "mail-1").await;
            assert_eq!((state.as_str(), attempts, lease), ("pending", 1, None));
            assert_eq!(error.as_deref(), Some("EMAIL_DELIVERY_FAILED"));

            // Successful delivery marks the row sent and clears the lease.
            let claim = repository.claim_next_email(163_000, 63_000).await.unwrap().unwrap();
            assert_eq!(claim.attempts, 2);
            repository.mark_sent("mail-1", 63_000).await.unwrap();
            let (state, attempts, lease, error) = email_state(&pool, "mail-1").await;
            assert_eq!((state.as_str(), attempts, lease, error), ("sent", 2, None, None));
            assert!(repository.claim_next_email(163_000, 63_000).await.unwrap().is_none());

            // The bounded worker turn renews the link and settles the mail.
            let second = queue_booking_mail(
                &mut repository,
                &ReverseCipher,
                &config,
                "property",
                "reservation-1",
                "acme",
                PmsBookingMailKind::Cancellation,
                "token-2",
                "mail-2",
                70_000,
            )
            .await
            .unwrap();
            let provider = RecordingProvider(Mutex::new(Vec::new()));
            let report = deliver_booking_emails(
                &mut repository,
                &provider,
                &NoopGuard,
                &ReverseCipher,
                true,
                70_000,
            )
            .await
            .unwrap();
            assert_eq!(report.sent, 1);
            {
                let calls = provider.0.lock().unwrap();
                assert_eq!(calls[0].0, "pms-booking-mail-2");
                assert!(calls[0].1.contains(&second.token));
            }
            let renewed = sqlx::query("SELECT expires_at FROM pms_booking_tokens WHERE id='token-2'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(renewed.get::<i64, _>("expires_at"), 70_000 + TOKEN_TTL_MS);

            // Public request bucket: three allowed, the fourth refused, stale pruned.
            for _ in 0..3 {
                booking_rate_limit(&mut repository, "ip:1.2.3.4", Some(3), 1_000)
                    .await
                    .unwrap();
            }
            assert_eq!(
                match booking_rate_limit(&mut repository, "ip:1.2.3.4", Some(3), 1_000)
                    .await
                    .unwrap_err()
                {
                    PgTransactionError::Business(error) => error.code,
                    PgTransactionError::Persistence(_) => "PERSISTENCE",
                },
                "PMS_RATE_LIMIT"
            );
            sqlx::query("INSERT INTO pms_booking_limits(key,bucket,count) VALUES('stale',-5,4)")
                .execute(pool.raw())
                .await
                .unwrap();
            repository.prune_rate_limits(0).await.unwrap();
            let remaining: i64 =
                sqlx::query_scalar("SELECT count(*) FROM pms_booking_limits")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(remaining, 1);
        })
        .await;
    }
}
