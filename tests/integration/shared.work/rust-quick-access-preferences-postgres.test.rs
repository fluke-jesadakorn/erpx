//! Native Work Projection quick-access preferences on a disposable Control DB.
//! Pins are selectors only; this test does not grant catalogue authority.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_work_projection::public::quick_access_preferences::{
    QuickAccessPreference, SaveQuickAccessInput, read_quick_access, request_hash, result_json,
    save_quick_access,
};
use sqlx::Connection;

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn input(
    user: &str,
    surface: &str,
    context: &str,
    pins: Option<Vec<&str>>,
    expected: i64,
    available: &[&str],
    key: &str,
) -> SaveQuickAccessInput {
    SaveQuickAccessInput {
        user_id: user.into(),
        surface: surface.into(),
        context_key: context.into(),
        pinned_ids: pins.map(|values| values.into_iter().map(str::to_owned).collect()),
        expected_version: expected,
        available_ids: available.iter().map(|value| (*value).to_owned()).collect(),
        request_key: key.into(),
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::Control)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::Control, &files)
        .await
        .unwrap();
}

#[tokio::test]
async fn defaults_empty_order_and_null_roundtrip() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        assert_eq!(
            read_quick_access(&mut tx, "user-1", "front-pos", "acme:company-1")
                .await
                .unwrap(),
            QuickAccessPreference {
                pinned_ids: None,
                version: 0
            }
        );
        let first = save_quick_access(
            &mut tx,
            &input(
                "user-1",
                "front-pos",
                "acme:company-1",
                Some(vec![]),
                0,
                &["sale", "history"],
                "request-key-1",
            ),
            100,
        )
        .await
        .unwrap();
        assert_eq!(
            first,
            QuickAccessPreference {
                pinned_ids: Some(vec![]),
                version: 1
            }
        );
        let second = save_quick_access(
            &mut tx,
            &input(
                "user-1",
                "front-pos",
                "acme:company-1",
                Some(vec!["history", "sale"]),
                1,
                &["sale", "history"],
                "request-key-2",
            ),
            101,
        )
        .await
        .unwrap();
        assert_eq!(
            second,
            QuickAccessPreference {
                pinned_ids: Some(vec!["history".into(), "sale".into()]),
                version: 2
            }
        );
        let third = save_quick_access(
            &mut tx,
            &input(
                "user-1",
                "front-pos",
                "acme:company-1",
                None,
                2,
                &[],
                "request-key-3",
            ),
            102,
        )
        .await
        .unwrap();
        assert_eq!(
            third,
            QuickAccessPreference {
                pinned_ids: None,
                version: 3
            }
        );
        assert_eq!(
            read_quick_access(&mut tx, "user-1", "front-pos", "acme:company-1")
                .await
                .unwrap(),
            third
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn scope_isolation_replay_conflict_stale_and_availability() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        let first = save_quick_access(
            &mut tx,
            &input(
                "user-1",
                "front-pos",
                "acme:company-1",
                Some(vec!["sale"]),
                0,
                &["sale"],
                "request-key-1",
            ),
            100,
        )
        .await
        .unwrap();
        assert_eq!(first.version, 1);
        // Exact replay returns the committed result.
        assert_eq!(
            save_quick_access(
                &mut tx,
                &input(
                    "user-1",
                    "front-pos",
                    "acme:company-1",
                    Some(vec!["sale"]),
                    0,
                    &["sale"],
                    "request-key-1",
                ),
                100,
            )
            .await
            .unwrap(),
            first
        );
        // A different payload under the same key conflicts.
        assert_eq!(
            business(
                save_quick_access(
                    &mut tx,
                    &input(
                        "user-1",
                        "front-pos",
                        "acme:company-1",
                        Some(vec![]),
                        0,
                        &["sale"],
                        "request-key-1",
                    ),
                    100,
                )
                .await
                .unwrap_err()
            )
            .code,
            "IDEMPOTENCY_CONFLICT"
        );
        // Stale version.
        assert_eq!(
            business(
                save_quick_access(
                    &mut tx,
                    &input(
                        "user-1",
                        "front-pos",
                        "acme:company-1",
                        Some(vec![]),
                        0,
                        &["sale"],
                        "request-key-2",
                    ),
                    100,
                )
                .await
                .unwrap_err()
            )
            .code,
            "VERSION_CONFLICT"
        );
        // A new pin must be currently available.
        assert_eq!(
            business(
                save_quick_access(
                    &mut tx,
                    &input(
                        "user-1",
                        "front-pos",
                        "acme:company-1",
                        Some(vec!["sale", "history"]),
                        1,
                        &["sale"],
                        "request-key-3",
                    ),
                    100,
                )
                .await
                .unwrap_err()
            )
            .code,
            "SHORTCUT_UNAVAILABLE"
        );
        // Existing pins may remain even when no longer available.
        assert_eq!(
            save_quick_access(
                &mut tx,
                &input(
                    "user-1",
                    "front-pos",
                    "acme:company-1",
                    Some(vec!["sale"]),
                    1,
                    &[],
                    "request-key-4",
                ),
                100,
            )
            .await
            .unwrap()
            .version,
            2
        );
        // Other users, surfaces and contexts remain untouched.
        for (user, surface, context) in [
            ("user-2", "front-pos", "acme:company-1"),
            ("user-1", "front-pms", "acme:company-1"),
            ("user-1", "front-pos", "acme:company-2"),
        ] {
            assert_eq!(
                read_quick_access(&mut tx, user, surface, context)
                    .await
                    .unwrap(),
                QuickAccessPreference {
                    pinned_ids: None,
                    version: 0
                }
            );
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn request_hash_and_result_match_actual_typescript() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let application =
        root.join("packages/server-work_projection/src/application/quick-access-preferences.ts");
    let script = r#"import {createHash} from 'node:crypto';
const {createQuickPreferenceService}=await import(process.argv[1]);
let hashValue;
const repo={transactional:true,preference:async()=>null,receipt:async()=>null,savePreference:async()=>{},saveReceipt:async(_key,_user,_surface,_context,hash,result)=>{globalThis.captured={hash,result}},transaction:async(action)=>action(repo)};
const service=createQuickPreferenceService({repository:repo,clock:{now:()=>123},error:(status,code,message)=>Object.assign(new Error(message),{status,code}),hash:value=>{hashValue=value;const hash=createHash('sha256').update(value).digest('hex');globalThis.captured={...(globalThis.captured??{}),hash};return hash}});
const result=await service.saveQuickAccessPreferences({userId:'user-1',surface:'front-pos',contextKey:'acme:company-1',pinnedIds:['sale','history'],expectedVersion:0,availableIds:['sale','history'],requestKey:'request-key-1'});
process.stdout.write(JSON.stringify({value:hashValue,hash:globalThis.captured.hash,result}));"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(&root)
            .args(["--conditions=react-server", "-e", script])
            .arg(application)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "actual TypeScript quick-access oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let input = input(
        "user-1",
        "front-pos",
        "acme:company-1",
        Some(vec!["sale", "history"]),
        0,
        &["sale", "history"],
        "request-key-1",
    );
    // TypeScript preserves the schema declaration order for the nested object.
    let expected_hash_input = r#"["user-1","front-pos","acme:company-1",{"pinnedIds":["sale","history"],"expectedVersion":0}]"#;
    assert_eq!(oracle["value"].as_str().unwrap(), expected_hash_input);
    assert_eq!(oracle["hash"].as_str().unwrap(), request_hash(&input));
    assert_eq!(
        oracle["result"],
        serde_json::from_str::<serde_json::Value>(&result_json(&QuickAccessPreference {
            pinned_ids: Some(vec!["sale".into(), "history".into()]),
            version: 1,
        }))
        .unwrap()
    );
}

#[tokio::test]
async fn invalid_bounds_and_concurrent_expected_version_have_one_winner() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        for (pins, expected, key, code) in [
            (
                Some(vec!["sale", "sale"]),
                0,
                "request-key-1",
                "INVALID_SHORTCUTS",
            ),
            (
                Some(vec!["1", "2", "3", "4", "5", "6", "7"]),
                0,
                "request-key-2",
                "INVALID_SHORTCUTS",
            ),
            (
                Some(vec![&"x".repeat(201)[..]]),
                0,
                "request-key-3",
                "INVALID_SHORTCUTS",
            ),
            (None, -1, "request-key-4", "INVALID_SHORTCUTS"),
            (None, 0, "", "IDEMPOTENCY_KEY_REQUIRED"),
        ] {
            let mut value = input(
                "user-1",
                "front-pos",
                "acme:company-1",
                pins,
                expected,
                &[],
                key,
            );
            if code == "INVALID_SHORTCUTS" && value.pinned_ids.is_none() {
                value.expected_version = -1;
            }
            assert_eq!(
                business(save_quick_access(&mut tx, &value, 100).await.unwrap_err()).code,
                code
            );
        }
        tx.rollback().await.unwrap();
        drop(c);

        let first_pool = pool.clone();
        let second_pool = pool.clone();
        let first = tokio::spawn(async move {
            let mut c = first_pool.raw().acquire().await.unwrap();
            let mut tx = c.begin().await.unwrap();
            let result = save_quick_access(
                &mut tx,
                &input(
                    "user-1",
                    "front-pos",
                    "acme:company-1",
                    Some(vec!["sale"]),
                    0,
                    &["sale", "history"],
                    "request-key-a",
                ),
                100,
            )
            .await;
            match result {
                Ok(value) => {
                    tx.commit().await.unwrap();
                    Ok(value)
                }
                Err(error) => {
                    tx.rollback().await.unwrap();
                    Err(error)
                }
            }
        });
        let second = tokio::spawn(async move {
            let mut c = second_pool.raw().acquire().await.unwrap();
            let mut tx = c.begin().await.unwrap();
            let result = save_quick_access(
                &mut tx,
                &input(
                    "user-1",
                    "front-pos",
                    "acme:company-1",
                    Some(vec!["history"]),
                    0,
                    &["sale", "history"],
                    "request-key-b",
                ),
                100,
            )
            .await;
            match result {
                Ok(value) => {
                    tx.commit().await.unwrap();
                    Ok(value)
                }
                Err(error) => {
                    tx.rollback().await.unwrap();
                    Err(error)
                }
            }
        });
        let mut results = vec![first.await.unwrap(), second.await.unwrap()];
        let winners = results.iter().filter(|result| result.is_ok()).count();
        assert_eq!(winners, 1);
        let loser = results
            .iter()
            .position(|result| result.is_err())
            .expect("one competing save must lose");
        let loser = results.remove(loser).unwrap_err();
        assert_eq!(business(loser).code, "VERSION_CONFLICT");
    })
    .await;
}
