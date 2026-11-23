//! Owner-side Quality work port and detail-core repository method over real
//! leased PostgreSQL units. No composition mount or legacy consumer is used.
#[path = "../../support/quality/quality-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::quality_http::{PgQualityRead, lock_quality_read_sources};
use erpx_server_quality::public::quality_read::{
    QualityActor, QualityReadRepository, quality_detail_core_from_source,
};
use erpx_server_quality::public::quality_work::{
    FnSourceAccess, MappedSourceAccess, QualitySourceAccess, quality_work,
};
use erpx_server_shared::public::{CompanyId, RecordId, UserId};

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn branch() -> RecordId {
    RecordId::new("branch").unwrap()
}

fn actor() -> QualityActor {
    QualityActor {
        user_id: UserId::new("inspector").unwrap(),
        company_id: company(),
        branch_id: Some(branch()),
        department_id: None,
        permissions: vec!["quality.read".into(), "quality.inspect".into()],
    }
}

#[tokio::test]
async fn quality_work_and_detail_core_read_the_same_scoped_hold_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "work-hold").await;
        facts::specification(&pool, "spec").await;
        facts::inspection(&pool, "work-hold", "inspection", "pass").await;

        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_quality_read_sources(&mut tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET spec_id='spec', spec_version=2 WHERE id='work-hold'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let mut read = PgQualityRead {
            connection: &mut tx,
        };
        let actor = actor();

        let core = read
            .detail_core(&actor.company_id, actor.branch_id.as_ref(), "work-hold")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(core.hold.id, "work-hold");
        assert_eq!(core.inspections.len(), 1);
        assert_eq!(core.spec_metrics.len(), 2);
        assert!(core.alerts.is_empty());
        // Cancellation history is deliberately absent from the core port.
        let detail = quality_detail_core_from_source(&actor, core).unwrap();
        assert_eq!(detail.hold.hold.id, "work-hold");
        assert_eq!(detail.hold.next_action, "Record an independent inspection");

        let open = FnSourceAccess::new(|_| None);
        let allowed = quality_work(&actor, &mut read, &open).await.unwrap();
        assert_eq!(allowed.items.len(), 1);
        let item = &allowed.items[0];
        assert_eq!(item.id, "work-hold");
        assert_eq!(item.status, "held");
        assert!(item.can_act);
        assert_eq!(item.queue, "todo");
        assert_eq!(item.source.href, "/quality/work-hold");
        assert_eq!(item.due_at, Some(1_794_268_800_000));
        assert!(item.blocker.is_some());

        let gated = MappedSourceAccess::new().with(
            "work-hold",
            Some(QualitySourceAccess {
                new_work: true,
                settle_existing: false,
                history: true,
                revision: "rev-1".into(),
            }),
        );
        let downgraded = quality_work(&actor, &mut read, &gated).await.unwrap();
        assert_eq!(downgraded.items.len(), 1);
        let item = &downgraded.items[0];
        assert!(!item.can_act);
        assert_eq!(item.action_kind, "work");
        assert_eq!(item.queue, "waiting");
        assert!(item.relationships.is_empty());

        let foreign = QualityActor {
            company_id: CompanyId::new("foreign").unwrap(),
            ..actor.clone()
        };
        let none = quality_work(&foreign, &mut read, &open).await.unwrap();
        assert!(none.items.is_empty());

        tx.rollback().await.unwrap();
    })
    .await;
}
