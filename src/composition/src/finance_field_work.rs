//! Field-required financial Work, composed from the existing native owners.
use crate::finance_journal_create::FinanceJournalCreateConfiguration;
use erpx_server_finance::public::field_work::{field_financial_work_candidates,field_financial_work_item};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError,Clock,RecordId};
use serde_json::Value;
use sqlx::PgConnection;

pub(crate) async fn read<C:Clock>(c:&mut PgConnection,current:&AuthorizedWorkContext,config:&FinanceJournalCreateConfiguration,clock:&C)->Result<Vec<Value>,PgTransactionError<BusinessError>>{
    let candidates=field_financial_work_candidates(c,&config.company).await?;
    let mut items=Vec::new();
    for(kind,id)in candidates {
        let encoded:String=id.bytes().map(|byte|match byte {
            b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~'|b'!'|b'\''|b'('|b')'|b'*'=>(byte as char).to_string(),
            _=>format!("%{byte:02X}"),
        }).collect();
        let href=format!("/finance/{}/{encoded}",if kind=="invoice"{"invoices"}else{"journals"});
        let result=if kind=="invoice" {
            let context=crate::field_service_source_record::SourceRecordContext{purchase_locks:None,actor:current,holding:&config.holding,company:&config.company,catalogue:&config.catalogue,clock};
            let source=crate::finance_service_invoice_history::read_service_invoice_history(c,&context,&id).await;
            match source {
                Ok(source)=>{
                    let purchase=crate::finance_presentation::read_finance_presentation(c,&config.holding,&config.company,&config.catalogue,clock).await?;
                    let record=serde_json::to_value(&source.data().invoice).map_err(|_|BusinessError::invalid("FINANCE_WORK_INVALID","Invalid financial source."))?;
                    let flow=crate::finance_service_invoice_flow::read_service_invoice_flow(c,current,&record,purchase.can_post_invoice(),clock).await?;
                    let item=field_financial_work_item(&kind,&record,&flow.view,current.user_id().as_str(),href)?;
                    source.require_current(c,&context).await?;
                    flow.require_current(c,current,&record,purchase.can_post_invoice(),clock).await?;
                    purchase.require_current(c,&config.holding,&config.company,&config.catalogue,clock).await?;
                    Ok(item)
                },
                Err(e)=>Err(e),
            }
        }else{
            match crate::finance_field_journal_flow::read_field_journal_flow(c,current,&RecordId::new(&id)?,config,clock).await{
                Ok(source)=>{
                    let record=serde_json::to_value(source.workspace()?.journal).map_err(|_|BusinessError::invalid("FINANCE_WORK_INVALID","Invalid financial source."))?;
                    let item=field_financial_work_item(&kind,&record,&source.view,current.user_id().as_str(),href)?;
                    source.require_current(c,current,config,clock).await?;
                    Ok(item)
                },
                Err(e)=>Err(e),
            }
        };
        match result {
            Ok(Some(item))=>items.push(item),Ok(None)=>{},
            // Hidden history remains hidden. Source drift, corrupt lineage,
            // stale identity and unavailable dependencies never select legacy.
            Err(PgTransactionError::Business(e))if matches!(e.kind,erpx_server_shared::public::FailureKind::Forbidden|erpx_server_shared::public::FailureKind::NotFound)=>{},
            Err(e)=>return Err(e),
        }
    }
    Ok(items)
}
