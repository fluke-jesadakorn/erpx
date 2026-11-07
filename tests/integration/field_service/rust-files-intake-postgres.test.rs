//! Files set intake against the disposable actual Account/Holding fixture.
#[allow(dead_code, reason = "Shared real Account and Field Service fixture")]
#[expect(
    unused_imports,
    reason = "Shared fixture reexports helpers used by other test targets"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAuthority, FieldIntakeAvailability, FieldIntakeScope, FieldSetDisposition,
    FieldSetListRequest, FieldSetRequest, create_field_set, list_field_sets, prepare_field_intake,
    prepare_field_set_creation,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, WorkAuthorizationRequest,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, Clock};
use sqlx::PgConnection;

struct Authority;
impl FieldIntakeAuthority for Authority {
    fn authorize<'a>(
        &'a self,
        _: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Box::pin(async move {
            Ok(FieldIntakeScope {
                company_id: actor.selection().workspace().company_id.clone(),
                branch_id: Some("branch".into()),
                department_id: None,
                version: 3,
            })
        })
    }
    fn revalidate<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn native_set_replays_and_reuses_with_persisted_alias() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        let pool = p.pool().raw().clone();
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| { let pool = pool.clone(); Box::pin(async move {
            let mut other = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            let read_authority = Authority;
            let write_authority = Authority;
            let read = prepare_field_intake(c, &read_authority, actor, "issue-order", false, now).await?;
            let write = prepare_field_intake(c, &write_authority, actor, "issue-order", true, now).await?;
            assert!(read.require_current(&mut other, now).await.is_err());
            other.rollback().await?;
            let mut input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: Some(" room ".into()), purpose_key: "site_photo".into(), expected_target_version: 3 };
            let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
            let plan = prepare_field_set_creation(c, &read, Some(&write), &input, "intake-one", &ready, now).await?;
            assert_eq!(plan.disposition(), FieldSetDisposition::New);
            assert_eq!(plan.expected_target_version(), 3);
            assert_eq!(plan.policy_version(), 1);
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'").fetch_one(&mut *c).await?;
            assert_eq!(count, 0);
            sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('planned-drift','company','branch','field_service_visit','issue-order','record','room','site_photo',3,1,'internal','{}','fixture-a',1,1)").execute(&mut *c).await?;
            assert!(matches!(plan.apply(c, &read, Some(&write), now).await, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"));
            sqlx::query("DELETE FROM attachment_sets WHERE id='planned-drift'").execute(&mut *c).await?;
            input.purpose_key = "technician_report".into();
            assert!(plan.apply(c, &read, Some(&write), actor.expires_at()).await.is_err());
            let written = plan.apply(c, &read, Some(&write), now + 1).await?;
            written.require_current(c).await?;
            let mut other = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(written.require_current(&mut other).await.is_err());
            other.rollback().await?;
            let first = written.data().clone();
            assert_eq!(first["purposeKey"], "site_photo");
            assert_eq!(first["createdAt"], now);
            input.purpose_key = "site_photo".into();
            sqlx::query("UPDATE attachment_sets SET classification='restricted' WHERE id=$1").bind(plan.set_id()).execute(&mut *c).await?;
            assert!(written.require_current(c).await.is_err());
            sqlx::query("UPDATE attachment_sets SET classification='internal' WHERE id=$1").bind(plan.set_id()).execute(&mut *c).await?;
            written.require_current(c).await?;
            let mut bom = input.clone();
            bom.component_ref = Some("\u{feff}room\u{feff}".into());
            let replay = create_field_set(c, &read, None, &bom, "intake-one", &ready, now).await?;
            let reused = create_field_set(c, &read, Some(&write), &input, "intake-two", &ready, now).await?;
            let alias_replay = create_field_set(c, &read, None, &input, "intake-two", &ready, now).await?;
            assert_eq!(first, replay);
            assert_eq!(first, reused);
            assert_eq!(first, alias_replay);
            assert_eq!(first["componentRef"], "room");
            let mut changed = input.clone();
            changed.component_ref = Some("\u{0085}room\u{0085}".into());
            assert!(create_field_set(c, &read, None, &changed, "intake-one", &ready, now).await.is_err());
            let unavailable = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: false, preview_ready: true };
            assert!(create_field_set(c, &read, Some(&write), &input, "intake-three", &unavailable, now).await.is_err());
            let list = list_field_sets(c, &read, &FieldSetListRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), purpose_key: None, component_kind: None, expected_target_version: None }, false, now).await?;
            assert_eq!(list, vec![first.clone()]);
            let empty_filters = list_field_sets(c, &read, &FieldSetListRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), purpose_key: Some(String::new()), component_kind: Some(String::new()), expected_target_version: Some(0) }, false, now).await?;
            assert_eq!(empty_filters, vec![first]);
            Ok(())
        }) }).await;
        result.unwrap();
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.reused')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (1, 1));
    }).await;
}

#[tokio::test]
async fn late_set_mutation_rolls_back_creation_and_receipt() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        sqlx::raw_sql("CREATE FUNCTION files_intake_late_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE attachment_sets SET bound_at=123 WHERE id=NEW.set_id; RETURN NEW; END $$; CREATE TRIGGER files_intake_late_change AFTER INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='set.created') EXECUTE FUNCTION files_intake_late_change()").execute(p.pool().raw()).await.unwrap();
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| Box::pin(async move {
            let a = Authority;
            let b = Authority;
            let read = prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
            let write = prepare_field_intake(c, &b, actor, "issue-order", true, now).await?;
            let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: 3 };
            let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
            create_field_set(c, &read, Some(&write), &input, "late-change", &ready, now).await?;
            Ok(())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"), "{result:?}");
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.created')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (0, 0));
    }).await;
}

#[tokio::test]
async fn late_created_event_identity_mutation_rolls_back() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        sqlx::raw_sql("CREATE FUNCTION files_intake_late_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.id := 'tampered-event'; RETURN NEW; END $$; CREATE TRIGGER files_intake_late_change BEFORE INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='set.created') EXECUTE FUNCTION files_intake_late_change()").execute(p.pool().raw()).await.unwrap();
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| Box::pin(async move {
            let a = Authority;
            let b = Authority;
            let read = prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
            let write = prepare_field_intake(c, &b, actor, "issue-order", true, now).await?;
            let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: 3 };
            let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
            create_field_set(c, &read, Some(&write), &input, "late-change", &ready, now).await?;
            Ok(())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"), "{result:?}");
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.created')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (0, 0));
    }).await;
}

struct ReadDrift(std::sync::atomic::AtomicUsize);
impl FieldIntakeAuthority for ReadDrift {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Authority.authorize(c, actor, target, write)
    }
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        _: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 2 {
                sqlx::query(
                    "UPDATE attachment_sets SET classification='restricted' WHERE id='seed-set'",
                )
                .execute(c)
                .await?;
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn alias_receipt_tampering_and_read_drift_roll_back() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        sqlx::raw_sql("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('seed-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',1,1); INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('seed-event','company','seed-set','fixture-a','set.created','{\"compatibilityAdapter\":false}',1); CREATE FUNCTION files_alias_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.details_json := '{}'; RETURN NEW; END $$; CREATE TRIGGER files_alias_tamper BEFORE INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='set.reused') EXECUTE FUNCTION files_alias_tamper()").execute(p.pool().raw()).await.unwrap();
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| Box::pin(async move {
            let a = Authority; let b = Authority;
            let read = prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
            let write = prepare_field_intake(c, &b, actor, "issue-order", true, now).await?;
            let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: 3 };
            let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
            create_field_set(c, &read, Some(&write), &input, "tampered-alias", &ready, now).await?; Ok(())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"), "{result:?}");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_events WHERE action='set.reused'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(count, 0);
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| Box::pin(async move {
            let a = ReadDrift(std::sync::atomic::AtomicUsize::new(0));
            let read = prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
            list_field_sets(c, &read, &FieldSetListRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), purpose_key: None, component_kind: None, expected_target_version: None }, false, now).await?; Ok(())
        })).await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"), "{result:?}");
        let classification: String = sqlx::query_scalar("SELECT classification FROM attachment_sets WHERE id='seed-set'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(classification, "internal");
    }).await;
}

#[tokio::test]
async fn oversized_history_is_rejected_before_list_or_alias_creation() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        sqlx::raw_sql("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('large-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',1,1); INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('large-event','company','large-set','fixture-a','set.created',jsonb_build_object('compatibilityAdapter',false,'padding',repeat('x',2100000))::text,1)").execute(p.pool().raw()).await.unwrap();
        use sha2::{Digest, Sha256};
        let digest = hex::encode(Sha256::digest(b"company\0fixture-a\0oversized-replay"));
        let requested = format!("attachment-set-{}", &digest[..32]);
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,'company','large-set','fixture-a','set.reused',jsonb_build_object('requestedSetId',$2::text)::text,1)").bind(format!("set-alias-{requested}")).bind(requested).execute(p.pool().raw()).await.unwrap();
        let result = p.review.authorizer.execute_with_sources("acme.example.test", p.review.maker.clone(), p.selection.clone(), WorkAuthorizationRequest { permission: "records.read".into(), module: Some("field_service".into()), destination: "field-service".into(), api: true }, p.review.configuration.clone(), move |c, actor| Box::pin(async move {
            let a = Authority; let b = Authority;
            let read = prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
            let write = prepare_field_intake(c, &b, actor, "issue-order", true, now).await?;
            let list = list_field_sets(c, &read, &FieldSetListRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), purpose_key: None, component_kind: None, expected_target_version: None }, false, now).await;
            assert!(matches!(list, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "ATTACHMENT_READ_LIMIT"));
            let input = FieldSetRequest { target_type: "field_service_visit".into(), target_id: "issue-order".into(), component_kind: "record".into(), component_ref: None, purpose_key: "site_photo".into(), expected_target_version: 3 };
            let ready = FieldIntakeAvailability { uploads_frozen: false, scanner_ready: true, preview_ready: true };
            let replay = create_field_set(c, &read, None, &input, "oversized-replay", &ready, now).await;
            assert!(matches!(replay, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "ATTACHMENT_READ_LIMIT"));
            create_field_set(c, &read, Some(&write), &input, "oversized-alias", &ready, now).await?; Ok(())
        })).await;
        assert!(matches!(result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "ATTACHMENT_READ_LIMIT"));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_events WHERE action='set.reused'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(count, 1);
    }).await;
}

struct MembershipDrift(std::sync::atomic::AtomicUsize);
impl FieldIntakeAuthority for MembershipDrift {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a AuthorizedWorkContext,
        target: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Authority.authorize(c, actor, target, write)
    }
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        _: &'a AuthorizedWorkContext,
        _: &'a str,
        _: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 2 {
                sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('inserted-set','company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',1,1)").execute(c).await?;
            }
            Ok(())
        })
    }
}
#[tokio::test]
async fn insertion_during_empty_list_revalidation_rolls_back() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let now = p.review.system.scenario.clock.now().get();
        let result = p
            .review
            .authorizer
            .execute_with_sources(
                "acme.example.test",
                p.review.maker.clone(),
                p.selection.clone(),
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: Some("field_service".into()),
                    destination: "field-service".into(),
                    api: true,
                },
                p.review.configuration.clone(),
                move |c, actor| {
                    Box::pin(async move {
                        let a = MembershipDrift(std::sync::atomic::AtomicUsize::new(0));
                        let read =
                            prepare_field_intake(c, &a, actor, "issue-order", false, now).await?;
                        list_field_sets(
                            c,
                            &read,
                            &FieldSetListRequest {
                                target_type: "field_service_visit".into(),
                                target_id: "issue-order".into(),
                                purpose_key: None,
                                component_kind: None,
                                expected_target_version: None,
                            },
                            false,
                            now,
                        )
                        .await?;
                        Ok(())
                    })
                },
            )
            .await;
        assert!(matches!(&result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "SERVICE_ATTACHMENT_CHANGED"), "{result:?}");
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE id='inserted-set'")
                .fetch_one(p.pool().raw())
                .await
                .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}
