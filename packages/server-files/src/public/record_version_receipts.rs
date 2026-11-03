//! Compiled read relation for actual contiguous record binding commands. This
//! supplies no file access or business-write authority. The outer transaction
//! also prepares Workflow's receipt sources before its first source snapshot.
pub const CONTRACT:&str="files.record-version-receipts.v1";
pub const RECORD_ATTACHMENT_VERSION_STEPS:&str=include_str!("../adapters/record_version_receipts.sql");

pub async fn lock_record_version_sources(c:&mut sqlx::PgConnection)->Result<(),erpx_server_runtime::public::postgres::PgTransactionError<erpx_server_shared::public::BusinessError>>{
    use sqlx::Connection;
    if !c.is_in_transaction(){return Err(erpx_server_runtime::public::postgres::PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE attachment_sets,attachment_events IN SHARE MODE").execute(c).await?;
    Ok(())
}
