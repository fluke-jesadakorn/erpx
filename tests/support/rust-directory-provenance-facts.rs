//! Explicit synthetic durable facts for IAM-only tests. These values do not
//! claim an actual LDAP/profile match; provider workflow tests use the native
//! source change adapter instead. Never imported by production code.
use erpx_server_runtime::public::postgres::DatabasePool;
async fn require_disposable(pool: &DatabasePool) {
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool.raw())
        .await
        .unwrap();
    assert!(
        name.starts_with("erpx_dev_rust_"),
        "synthetic source facts require the guarded disposable database"
    );
}
pub async fn approve_synthetic_source(pool: &DatabasePool, id: &str, company: &str, now: i64) {
    require_disposable(pool).await;
    sqlx::query("INSERT INTO iam_directory_source_selections(connection_id,company_id,revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,selected_at) SELECT id,company_id,1,1,'synthetic_facts_only',decode(repeat('55',32),'hex'),version,sha256(convert_to(config::text,'UTF8')),$3 FROM iam_connections WHERE id=$1 AND company_id=$2")
        .bind(id).bind(company).bind(now).execute(pool.raw()).await.unwrap();
}
pub async fn stamp_synthetic_observation(
    pool: &DatabasePool,
    id: &str,
    company: &str,
    observed_at: i64,
) {
    require_disposable(pool).await;
    sqlx::query("INSERT INTO iam_directory_observation_sources(connection_id,company_id,source_revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,observation_id,observed_at) SELECT connection_id,company_id,revision,format_version,key_id,profile_tag,connection_version,config_fingerprint,$3,$4 FROM iam_directory_source_selections WHERE connection_id=$1 AND company_id=$2 ON CONFLICT(connection_id) DO UPDATE SET source_revision=excluded.source_revision,format_version=excluded.format_version,key_id=excluded.key_id,profile_tag=excluded.profile_tag,connection_version=excluded.connection_version,config_fingerprint=excluded.config_fingerprint,observation_id=excluded.observation_id,observed_at=excluded.observed_at")
        .bind(id).bind(company).bind(uuid::Uuid::new_v4()).bind(observed_at).execute(pool.raw()).await.unwrap();
}
