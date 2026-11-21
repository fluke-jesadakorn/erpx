#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_workflow::public::commands::CommandField;
use erpx_server_workflow::public::rfq_command_receipt::*;
use erpx_server_workflow::public::source_ordered_json::{
    ScopedWorkAuthority, SourceOrderedFingerprintActor, SourceOrderedJson,
};
use sqlx::Connection;

#[tokio::test]
async fn exact_typescript_fingerprint_and_receipt_source_are_retained() {
    let script = r#"const {rfqActionSchema}=require('./packages/server-procurement/src/domain/rfqs.ts');const crypto=require('node:crypto');const input=rfqActionSchema.parse({reason:'  reason  ',action:'submit',expectedVersion:1,offerId:'x'.repeat(2*1024*1024+20)});const fingerprint={actor:'user',assignment:'assignment',role:'r'.repeat(129),authority:{sessionId:'session',revision:0,assignmentVersion:1},company:'company',branch:null,department:null,operation:'procurement.rfq.submit',recordId:'rfq',input};process.stdout.write(JSON.stringify({input:JSON.stringify(input),hash:crypto.createHash('sha256').update(JSON.stringify(fingerprint)).digest('hex')}));"#;
    let output =
        std::process::Command::new(std::env::var("ERPX_NODE_BIN").expect("pinned Node required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import=tsx", "-e", script])
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let golden: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let raw = golden["input"].as_str().unwrap().to_owned();
    let hash = golden["hash"].as_str().unwrap().to_owned();
    fixture::with_database(2,move|pool|async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE public.command_receipts(command_id text PRIMARY KEY,request_hash text NOT NULL,result_json text,operation text NOT NULL,actor_id text NOT NULL,authority_json text); INSERT INTO public.command_receipts VALUES('key','placeholder','{\"id\":\"rfq\",\"version\":2}','procurement.rfq.submit','user','{\"companyId\":\"company\"}');").execute(&mut *c).await.unwrap();
        sqlx::query("UPDATE public.command_receipts SET request_hash=$1 WHERE command_id='key'").bind(&hash).execute(&mut *c).await.unwrap();
        let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        assert!(inspect_rfq_receipt(&mut tx,"key").await.is_err());lock_rfq_receipts(&mut tx).await.unwrap();
        let receipt=inspect_rfq_receipt(&mut tx,"key").await.unwrap();let missing=inspect_rfq_receipt(&mut tx,"missing").await.unwrap();
        let role="r".repeat(129);let actor=SourceOrderedFingerprintActor{user_id:"user",assignment_id:"assignment",role:&role,company_id:"company",branch:CommandField::Null,department:CommandField::Null,authority:CommandField::Value(ScopedWorkAuthority{session_id:"session",revision:0,assignment_version:1})};
        let input=SourceOrderedJson::parse(&raw).unwrap();
        assert!(matches!(receipt.compare(&actor,"procurement.rfq.submit","rfq",&input).unwrap(),RfqReceiptMatch::Match(Some(_))));
        assert!(matches!(receipt.compare(&actor,"procurement.rfq.submit","other",&input).unwrap(),RfqReceiptMatch::Conflict));
        assert!(matches!(missing.compare(&actor,"procurement.rfq.submit","rfq",&input).unwrap(),RfqReceiptMatch::Missing));
        assert!(receipt.selects_purchase_history(&mut tx,"procurement.rfq.submit","user","company").await.unwrap());
        assert!(!receipt.selects_purchase_history(&mut tx,"procurement.rfq.submit","other","company").await.unwrap());
        assert!(!receipt.selects_purchase_history(&mut tx,"procurement.rfq.submit","user","other").await.unwrap());
        assert!(!receipt.selects_purchase_history(&mut tx,"procurement.rfq.cancel","user","company").await.unwrap());
        assert!(!missing.selects_purchase_history(&mut tx,"procurement.rfq.submit","user","company").await.unwrap());
        // Purchase classifies history from the row even when replay's hash conflicts.
        assert!(matches!(receipt.compare(&actor,"procurement.rfq.submit","other",&input).unwrap(),RfqReceiptMatch::Conflict));
        assert!(receipt.selects_purchase_history(&mut tx,"procurement.rfq.submit","user","company").await.unwrap());
        sqlx::query("INSERT INTO public.command_receipts VALUES('bad','hash',NULL,'procurement.rfq.submit','user','invalid')").execute(&mut *tx).await.unwrap();
        let bad=inspect_rfq_receipt(&mut tx,"bad").await.unwrap();
        sqlx::query("SAVEPOINT malformed_authority").execute(&mut *tx).await.unwrap();
        assert!(bad.selects_purchase_history(&mut tx,"procurement.rfq.submit","user","company").await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT malformed_authority").execute(&mut *tx).await.unwrap();
        assert!(receipt.compare(&actor,"procurement.rfq.offer","rfq",&input).is_err());receipt.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE public.command_receipts SET result_json='null' WHERE command_id='key'").execute(&mut *tx).await.unwrap();
        assert!(receipt.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let mut next=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_rfq_receipts(&mut next).await.unwrap();assert!(receipt.require_current(&mut next).await.is_err());next.rollback().await.unwrap();
    }).await;
}
