//! Actual PostgreSQL qualification for the native generic Files remove command.
//!
//! Only disposable leased development databases are used. Owner/record
//! authority is a test port; production authority is owned by composition.
use erpx_server_files::public::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawTarget, withdraw_attachment,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PgWorkFuture,
    TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[path = "../../support/rust-postgres.rs"]
mod fixture;

type Error = PgTransactionError<BusinessError>;

async fn migrate(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

#[derive(Default)]
struct AuthorityInner {
    deny: bool,
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
#[derive(Clone, Default)]
struct Authority(Arc<AuthorityInner>);

impl Authority {
    fn allowed() -> Self {
        Self::default()
    }
    fn denied() -> Self {
        Self(Arc::new(AuthorityInner {
            deny: true,
            ..AuthorityInner::default()
        }))
    }
}

impl AttachmentWithdrawAuthority for Authority {
    fn authorize_withdraw<'a>(
        &'a self,
        _: &'a mut PgConnection,
        target: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.0.calls.fetch_add(1, Ordering::SeqCst);
            self.0.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}|{}|{}",
                target.company_id,
                target.branch_id.unwrap_or(""),
                target.target_type,
                target.target_id,
                target.expected_target_version,
                target.created_by
            ));
            if self.0.deny {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot change files for this record.",
                )
                .into());
            }
            Ok(())
        })
    }
}

/// Simulates an owner/record write that lands between authorization and the
/// Files row lock. The command must not inherit the earlier grant.
struct RacingAuthority;

impl AttachmentWithdrawAuthority for RacingAuthority {
    fn authorize_withdraw<'a>(
        &'a self,
        c: &'a mut PgConnection,
        target: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            sqlx::query("UPDATE attachment_links SET state='bound',bound_at=9999,binding_type='record',binding_id='race' WHERE id=$1")
                .bind(target.link_id)
                .execute(&mut *c)
                .await?;
            Ok(())
        })
    }
}

/// Simulates a current grant that is revoked after the first owner check and
/// before the Files row lock is released. The second port call must stop the
/// command before any write.
#[derive(Default)]
struct StaleAuthority {
    calls: AtomicUsize,
}

impl AttachmentWithdrawAuthority for StaleAuthority {
    fn authorize_withdraw<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn revalidate_withdraw<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(BusinessError::new(
                FailureKind::Conflict,
                "ATTACHMENT_WITHDRAW_AUTHORITY_CHANGED",
                "The selected role or file record changed. Reload and retry.",
            )
            .into())
        })
    }
}

/// Denies one owner category while allowing the generic Files target. The
/// composition root maps the category to the owner read port; this test port
/// proves the command cannot bypass that decision.
struct OwnerReadAuthority {
    deny: &'static str,
}

impl AttachmentWithdrawAuthority for OwnerReadAuthority {
    fn authorize_withdraw<'a>(
        &'a self,
        _: &'a mut PgConnection,
        target: &'a WithdrawTarget<'a>,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if target.target_type == self.deny {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot read this file's record.",
                )
                .into());
            }
            Ok(())
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &DatabasePool,
    prefix: &str,
    company: &str,
    target_type: &str,
    target_id: &str,
    set_state: &str,
    link_state: &str,
    creator: &str,
    withdrawn_by: Option<&str>,
    reason: Option<&str>,
) -> (String, String) {
    let set = format!("set-{prefix}");
    let link = format!("link-{prefix}");
    let bound = set_state == "bound";
    sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,bound_at,bound_command_id,created_by,created_at,updated_at) VALUES($1,$2,'branch',NULL,$3,$4,'record','manual',3,1,'internal','{}',1,10,$5,$6,$7,$8,1000,1000)")
        .bind(&set)
        .bind(company)
        .bind(target_type)
        .bind(target_id)
        .bind(set_state)
        .bind(bound.then_some(9000))
        .bind(bound.then(|| format!("bind-{prefix}")))
        .bind(creator)
        .execute(pool.raw())
        .await
        .unwrap();
    let link_bound = link_state == "bound";
    let link_withdrawn = link_state == "withdrawn";
    sqlx::query("INSERT INTO attachment_links(id,set_id,state,withdrawn_by,withdrawal_reason,withdrawn_at,bound_at,binding_type,binding_id,created_by,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1000,20000)")
        .bind(&link)
        .bind(&set)
        .bind(link_state)
        .bind(withdrawn_by)
        .bind(reason)
        .bind(link_withdrawn.then_some(1500))
        .bind(link_bound.then_some(9000))
        .bind(link_bound.then_some("record"))
        .bind(link_bound.then(|| format!("record-{prefix}")))
        .bind(creator)
        .execute(pool.raw())
        .await
        .unwrap();
    (set, link)
}

async fn run<T, F>(pool: &DatabasePool, work: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&'a mut PgConnection) -> PgWorkFuture<'a, T, BusinessError> + Send,
{
    PgUnitOfWork::new(pool.clone())
        .run_once::<T, BusinessError, _>(TransactionOptions::default(), work)
        .await
}

fn request<'a>(
    company: &'a str,
    actor: &'a str,
    link: &'a str,
    reason: &'a str,
    key: &'a str,
) -> WithdrawAttachmentRequest<'a> {
    WithdrawAttachmentRequest {
        actor_id: actor,
        company_id: company,
        link_id: link,
        reason,
        command_key: key,
        now: 5_000,
    }
}

async fn state(pool: &DatabasePool, link: &str) -> (String, Option<String>, Option<String>) {
    sqlx::query_as("SELECT state,withdrawn_by,withdrawal_reason FROM attachment_links WHERE id=$1")
        .bind(link)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn events(pool: &DatabasePool, link: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT id,actor_id,details_json FROM attachment_events WHERE link_id=$1 \
         AND action='link.withdrawn' ORDER BY created_at,id",
    )
    .bind(link)
    .fetch_all(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn withdraw_is_company_scoped_idempotent_and_audited() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let (_, own) = seed(
            &pool, "own", "company", "asset", "asset-1", "draft", "draft", "actor", None, None,
        )
        .await;
        let authority = Authority::allowed();
        let withdrawn = run(&pool, {
            let authority = authority.clone();
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &authority,
                        request(
                            "company",
                            "actor",
                            &own,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(withdrawn.state, "withdrawn");
        assert!(!withdrawn.replay);
        assert_eq!(
            state(&pool, &own).await,
            (
                "withdrawn".into(),
                Some("actor".into()),
                Some("duplicate upload".into())
            )
        );
        let audit = events(&pool, &own).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(
            audit[0].0,
            erpx_server_files::public::attachment_command::withdrawal_event_id(
                "company",
                &own,
                "withdraw-key-1"
            )
        );
        assert_eq!(audit[0].1, "actor");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&audit[0].2).unwrap()["reason"],
            "duplicate upload"
        );

        // Same key and same reason: replay without another audit row.
        let replay = run(&pool, {
            let authority = authority.clone();
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &authority,
                        request(
                            "company",
                            "actor",
                            &own,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap();
        assert!(replay.replay);
        assert_eq!(events(&pool, &own).await.len(), 1);

        // A fresh retry key with the same retained reason is still a replay,
        // matching the TypeScript writer's retained-link decision.
        let other_key = run(&pool, {
            let authority = authority.clone();
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &authority,
                        request(
                            "company",
                            "actor",
                            &own,
                            "duplicate upload",
                            "withdraw-key-2",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap();
        assert!(other_key.replay);
        assert_eq!(events(&pool, &own).await.len(), 1);

        // Different reason cannot reuse the withdrawn link.
        let conflict = run(&pool, {
            let authority = authority.clone();
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &authority,
                        request(
                            "company",
                            "actor",
                            &own,
                            "different reason",
                            "withdraw-key-3",
                        ),
                    )
                    .await
                })
            }
        })
        .await;
        assert!(matches!(
            conflict,
            Err(PgTransactionError::Business(BusinessError {
                kind: FailureKind::Conflict,
                code: "IDEMPOTENCY_CONFLICT",
                ..
            }))
        ));
    })
    .await;
}

#[tokio::test]
async fn withdraw_rejects_missing_transaction_isolation_authority_and_races() {
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        let (_, own) = seed(
            &pool, "race", "company", "asset", "asset-1", "draft", "draft", "actor", None, None,
        )
        .await;

        let mut plain = pool.raw().acquire().await.unwrap();
        let error = withdraw_attachment(
            &mut plain,
            &Authority::allowed(),
            request(
                "company",
                "actor",
                &own,
                "duplicate upload",
                "withdraw-key-1",
            ),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(error) if error.code == "FILES_TRANSACTION_REQUIRED"
        ));

        let mut read_committed = plain.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *read_committed)
            .await
            .unwrap();
        let error = withdraw_attachment(
            &mut read_committed,
            &Authority::allowed(),
            request(
                "company",
                "actor",
                &own,
                "duplicate upload",
                "withdraw-key-1",
            ),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(error) if error.code == "FILES_SERIALIZABLE_REQUIRED"
        ));
        read_committed.rollback().await.unwrap();

        let denied = Authority::denied();
        let error = run(&pool, {
            let denied = denied.clone();
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &denied,
                        request(
                            "company",
                            "actor",
                            &own,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Business(BusinessError {
                kind: FailureKind::Forbidden,
                code: "ATTACHMENT_FORBIDDEN",
                ..
            })
        ));
        assert_eq!(denied.0.calls.load(Ordering::SeqCst), 1);
        assert_eq!(state(&pool, &own).await.0, "draft");
        assert!(events(&pool, &own).await.is_empty());

        let error = run(&pool, {
            let own = own.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &RacingAuthority,
                        request(
                            "company",
                            "actor",
                            &own,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Business(BusinessError {
                kind: FailureKind::Conflict,
                code: "ATTACHMENT_LINK_LOCKED",
                ..
            })
        ));
        assert_eq!(state(&pool, &own).await.0, "draft");
        assert!(events(&pool, &own).await.is_empty());
    })
    .await;
}

#[tokio::test]
async fn withdraw_rejects_bound_foreign_creator_native_owner_and_closed_sets() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let (_, bound) = seed(
            &pool, "bound", "company", "asset", "asset-1", "bound", "bound", "actor", None, None,
        )
        .await;
        let (_, foreign) = seed(
            &pool, "foreign", "other", "asset", "asset-1", "draft", "draft", "actor", None, None,
        )
        .await;
        let (_, other_creator) = seed(
            &pool,
            "creator",
            "company",
            "asset",
            "asset-1",
            "draft",
            "draft",
            "someone-else",
            None,
            None,
        )
        .await;
        let (_, field) = seed(
            &pool,
            "field",
            "company",
            "field_service_visit",
            "visit-1",
            "draft",
            "draft",
            "actor",
            None,
            None,
        )
        .await;
        let (_, closed_set) = seed(
            &pool, "closed", "company", "asset", "asset-1", "bound", "draft", "actor", None, None,
        )
        .await;

        for (link, code, kind, expected_state) in [
            (
                &bound,
                "BOUND_ATTACHMENT_IMMUTABLE",
                FailureKind::Conflict,
                "bound",
            ),
            (
                &foreign,
                "ATTACHMENT_LINK_NOT_FOUND",
                FailureKind::NotFound,
                "draft",
            ),
            (
                &other_creator,
                "ATTACHMENT_WITHDRAW_FORBIDDEN",
                FailureKind::Forbidden,
                "draft",
            ),
            (
                &field,
                "FILES_NATIVE_OWNER_REQUIRED",
                FailureKind::Unavailable,
                "draft",
            ),
            (
                &closed_set,
                "ATTACHMENT_SET_LOCKED",
                FailureKind::Conflict,
                "draft",
            ),
        ] {
            let error = run(&pool, {
                let link = link.clone();
                move |c| {
                    Box::pin(async move {
                        withdraw_attachment(
                            c,
                            &Authority::allowed(),
                            request(
                                "company",
                                "actor",
                                &link,
                                "duplicate upload",
                                "withdraw-key-1",
                            ),
                        )
                        .await
                    })
                }
            })
            .await
            .unwrap_err();
            let PgTransactionError::Business(error) = error else {
                panic!("{link} expected a business error");
            };
            assert_eq!(error.code, code, "{link}");
            assert_eq!(error.kind, kind, "{link}");
            assert_eq!(state(&pool, link).await.0, expected_state, "{link}");
            assert!(events(&pool, link).await.is_empty(), "{link}");
        }
    })
    .await;
}

#[tokio::test]
async fn withdraw_rejects_stale_authorization_after_the_row_lock() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let (_, draft) = seed(
            &pool, "stale", "company", "asset", "asset-1", "draft", "draft", "actor", None, None,
        )
        .await;
        let authority = std::sync::Arc::new(StaleAuthority::default());
        let error = run(&pool, {
            let authority = authority.clone();
            let draft = draft.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        authority.as_ref(),
                        request(
                            "company",
                            "actor",
                            &draft,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Business(BusinessError {
                kind: FailureKind::Conflict,
                code: "ATTACHMENT_WITHDRAW_AUTHORITY_CHANGED",
                ..
            })
        ));
        // Both the pre-lock grant and the post-lock stale check ran, and no
        // link/audit write survived the refused command.
        assert_eq!(authority.calls.load(Ordering::SeqCst), 2);
        assert_eq!(state(&pool, &draft).await.0, "draft");
        assert!(events(&pool, &draft).await.is_empty());
    })
    .await;
}

#[tokio::test]
async fn withdraw_requires_the_owning_owner_read_grant() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let (_, asset) = seed(
            &pool, "owner-a", "company", "asset", "asset-1", "draft", "draft", "actor", None, None,
        )
        .await;
        let (_, legal) = seed(
            &pool,
            "owner-b",
            "company",
            "legal_item",
            "legal-1",
            "draft",
            "draft",
            "actor",
            None,
            None,
        )
        .await;

        let withdrawn = run(&pool, {
            let asset = asset.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &OwnerReadAuthority { deny: "legal_item" },
                        request(
                            "company",
                            "actor",
                            &asset,
                            "duplicate upload",
                            "withdraw-key-1",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(withdrawn.state, "withdrawn");

        let error = run(&pool, {
            let legal = legal.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &OwnerReadAuthority { deny: "legal_item" },
                        request(
                            "company",
                            "actor",
                            &legal,
                            "duplicate upload",
                            "withdraw-key-2",
                        ),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Business(BusinessError {
                kind: FailureKind::Forbidden,
                code: "ATTACHMENT_FORBIDDEN",
                ..
            })
        ));
        assert_eq!(state(&pool, &legal).await.0, "draft");
        assert!(events(&pool, &legal).await.is_empty());
    })
    .await;
}

#[test]
fn generic_owner_read_categories_are_stable() {
    use erpx_server_files::public::attachment_command::{
        WithdrawOwnerAccess, has_native_owner_command, withdraw_owner_access,
    };
    assert_eq!(
        withdraw_owner_access("asset"),
        WithdrawOwnerAccess::RecordsRead
    );
    assert_eq!(
        withdraw_owner_access("holding_brand"),
        WithdrawOwnerAccess::Brand
    );
    assert_eq!(
        withdraw_owner_access("branch_brand"),
        WithdrawOwnerAccess::Brand
    );
    assert_eq!(
        withdraw_owner_access("company_storefront_brand"),
        WithdrawOwnerAccess::Brand
    );
    assert_eq!(
        withdraw_owner_access("legal_item"),
        WithdrawOwnerAccess::Legal
    );
    assert_eq!(
        withdraw_owner_access("legal_external"),
        WithdrawOwnerAccess::Legal
    );
    assert_eq!(
        withdraw_owner_access("messaging_room"),
        WithdrawOwnerAccess::Messaging
    );
    assert_eq!(
        withdraw_owner_access("customer_access_binding"),
        WithdrawOwnerAccess::Portal
    );
    for target in [
        "field_service_visit",
        "project",
        "finance_journal",
        "finance_payment",
    ] {
        assert!(has_native_owner_command(target), "{target}");
    }
    assert!(!has_native_owner_command("legal_item"));
}

#[tokio::test]
async fn withdraw_replay_requires_the_original_audit_receipt() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let (_, no_event) = seed(
            &pool,
            "replay-a",
            "company",
            "asset",
            "asset-a",
            "draft",
            "withdrawn",
            "actor",
            Some("actor"),
            Some("duplicate upload"),
        )
        .await;
        let error = run(&pool, {
            let link = no_event.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &Authority::allowed(),
                        request("company", "actor", &link, "duplicate upload", "withdraw-key-1"),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        let PgTransactionError::Business(error) = error else {
            panic!("missing receipt expected a business error");
        };
        assert_eq!(error.code, "ATTACHMENT_WITHDRAW_RECEIPT_MISSING");

        let (wrong_set, wrong_event) = seed(
            &pool,
            "replay-b",
            "company",
            "asset",
            "asset-b",
            "draft",
            "withdrawn",
            "actor",
            Some("actor"),
            Some("duplicate upload"),
        )
        .await;
        sqlx::query("INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES('event-foreign-reason','company',$1,$2,'actor','link.withdrawn','{\"reason\":\"another reason\"}',1500)")
            .bind(&wrong_event)
            .bind(&wrong_set)
            .execute(pool.raw())
            .await
            .unwrap();
        let error = run(&pool, {
            let link = wrong_event.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &Authority::allowed(),
                        request("company", "actor", &link, "duplicate upload", "withdraw-key-1"),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap_err();
        let PgTransactionError::Business(error) = error else {
            panic!("wrong receipt expected a business error");
        };
        assert_eq!(error.code, "ATTACHMENT_WITHDRAW_RECEIPT_MISSING");

        let (good_set, good_event) = seed(
            &pool,
            "replay-c",
            "company",
            "asset",
            "asset-c",
            "draft",
            "withdrawn",
            "actor",
            Some("actor"),
            Some("duplicate upload"),
        )
        .await;
        sqlx::query("INSERT INTO attachment_events(id,company_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES('event-matching-reason','company',$1,$2,'actor','link.withdrawn','{\"reason\":\"duplicate upload\"}',1500)")
            .bind(&good_event)
            .bind(&good_set)
            .execute(pool.raw())
            .await
            .unwrap();
        let replay = run(&pool, {
            let link = good_event.clone();
            move |c| {
                Box::pin(async move {
                    withdraw_attachment(
                        c,
                        &Authority::allowed(),
                        request("company", "actor", &link, "duplicate upload", "withdraw-key-1"),
                    )
                    .await
                })
            }
        })
        .await
        .unwrap();
        assert!(replay.replay);
    })
    .await;
}
