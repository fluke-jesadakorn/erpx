//! Native responsibility command for a source-verified Field parts journal.
use crate::{
    finance_field_journal_flow::{CurrentFieldJournalFlow,read_field_journal_flow},
    finance_journal_create::{FinanceJournalCreateConfiguration,Ids},
    finance_journal_review::{FinanceJournalConfiguration,FinanceJournalEnvironment,command_actor},
    finance_service_invoice_flow_command::physical,
};
use erpx_server_collaboration::public::field_journal_flow_notification::*;
use erpx_server_identity_access::public::{
    DeviceProof,effective_authority::{AuthorizedWorkContext,HoldingWorkAuthorizer,HoldingWorkSourcePreparation,WorkAuthorizationRequest},
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people;
use erpx_server_runtime::public::postgres::{PgTransactionError,PgWorkFuture};
use erpx_server_shared::public::{BusinessError,Clock,FailureKind,RecordId,TimestampMillis};
use erpx_server_workflow::public::{commands::*,field_journal_flow::*};
use sqlx::PgConnection;
use std::sync::Arc;
type Error=PgTransactionError<BusinessError>;
fn changed()->BusinessError{BusinessError::new(FailureKind::Conflict,"FLOW_SOURCE_CHANGED","The current Field journal workflow changed. Reload its source.")}
#[derive(Clone)]
pub(crate) struct Sources(FinanceJournalConfiguration);
impl Sources{
    pub(crate) fn new(config:&FinanceJournalCreateConfiguration)->Result<Self,BusinessError>{
        Ok(Self(FinanceJournalConfiguration::new_enforced_post(FinanceJournalEnvironment::Development,config.holding.clone(),config.company.clone(),config.catalogue.clone())?.with_field_service_post()?))
    }
}
impl HoldingWorkSourcePreparation for Sources{
    fn prepare<'c>(&'c self,c:&'c mut PgConnection)->PgWorkFuture<'c,(),BusinessError>{
        Box::pin(async move{
            self.0.prepare(c).await?;
            erpx_server_finance::public::journal_workspace::lock_journal_workspace_sources(c).await?;
            erpx_server_people_payroll::public::finance_journal_names::lock_finance_journal_name_sources(c).await?;
            lock_field_journal_flow_sources(c).await?;
            lock_field_journal_flow_notices(c).await
        })
    }
}
struct Authority<'a,C>{current:&'a AuthorizedWorkContext,config:&'a FinanceJournalCreateConfiguration,clock:&'a C}
impl<C:Clock+Send+Sync> PgCommandAuthority for Authority<'_,C>{
    async fn lock_sources(&mut self,c:&mut PgConnection,_:&WorkflowRequest)->Result<(),Error>{Sources::new(self.config)?.prepare(c).await}
    async fn authorize(&mut self,c:&mut PgConnection,r:&WorkflowRequest)->Result<CommandActor,Error>{
        require_selected_actor_current(c,self.current,self.clock.now().get()).await?;
        if self.current.holding_id()!=&self.config.holding||self.current.selection().workspace().company_id!=self.config.company||r.operation!="flow.assign"||r.input.property("kind")?.as_ref().map(CommandJson::as_str)!=Some("\"journal\""){
            return Err(changed().into())
        }
        self.current.require_permission("finance.read")?;
        self.current.require_permission("module.finance.read")?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(&mut self,c:&mut PgConnection,actor:&CommandActor,r:&WorkflowRequest,disposition:CommandDisposition)->Result<(),Error>{
        if self.authorize(c,r).await?!=*actor{return Err(changed().into())}
        let id=RecordId::new(&r.record_id)?;
        let flow=read_field_journal_flow(c,self.current,&id,self.config,self.clock).await?;
        if disposition==CommandDisposition::Execute{
            flow.require_settle_existing(c,self.config,self.clock).await?;
            if flow.view["canManage"]!=true{return Err(BusinessError::new(FailureKind::Forbidden,"FLOW_ADMIN_REQUIRED","A company administrator must assign workflow responsibilities.").into())}
        }
        flow.require_current(c,self.current,self.config,self.clock).await
    }
}
struct Sample(TimestampMillis);
impl Clock for Sample{fn now(&self)->TimestampMillis{self.0}}
struct Effects<'a,C>{
    current:&'a AuthorizedWorkContext,config:&'a FinanceJournalCreateConfiguration,clock:&'a C,now:i64,
    source:Option<CurrentFieldJournalFlow>,plan:Option<FieldJournalFlowAssignment>,notice:Option<RetainedFieldJournalFlowNotice>,
}
impl<C:Clock+Send+Sync> PgOwnerCommandEffects for Effects<'_,C>{
    async fn lock_sources(&mut self,c:&mut PgConnection,_:&WorkflowRequest)->Result<(),Error>{Sources::new(self.config)?.prepare(c).await}
    async fn prepare(&mut self,c:&mut PgConnection,_:&CommandActor,r:&WorkflowRequest)->Result<WorkflowOutput,Error>{
        let input=physical(&r.input)?;
        let flow=read_field_journal_flow(c,self.current,&RecordId::new(&r.record_id)?,self.config,self.clock).await?;
        let people=current_journal_flow_people(c,&self.config.company).await?;
        let name=people.iter().find(|p|p.user_id==self.current.user_id().as_str()).map(|p|p.name.as_str()).unwrap_or(self.current.user_id().as_str());
        let event=uuid::Uuid::new_v4().to_string();
        let plan=flow.plan_assignment(FieldJournalFlowAssignmentInput{step:&input.step,user:&input.user,expected_flow:input.expected,expected_record:input.record,reason:&input.reason,save_default:input.save,actor:self.current.user_id().as_str(),actor_name:name,command:&r.key,event:&event,now:self.now})?;
        let result=WorkflowOutput{version:input.record,result:CommandJson::from_serializable(&plan.result)?,event_scope:None};
        self.source=Some(flow);self.plan=Some(plan);Ok(result)
    }
    async fn apply(&mut self,c:&mut PgConnection,_:&CommandActor,r:&WorkflowRequest)->Result<(),Error>{
        self.source.as_ref().ok_or_else(changed)?.require_current(c,self.current,self.config,self.clock).await?;
        let plan=self.plan.as_mut().ok_or_else(changed)?;plan.apply(c).await?;
        self.notice=Some(stage_field_journal_flow_notice(c,FieldJournalFlowNotice{id:&format!("field-journal-flow-notice-{}",r.key),company:self.config.company.as_str(),recipient:&plan.recipient,journal:&r.record_id,version:physical(&r.input)?.record,message:&plan.message,now:self.now}).await?);
        Ok(())
    }
    async fn retain(&mut self,c:&mut PgConnection,_:&CommandActor,r:&WorkflowRequest)->Result<(),Error>{
        let source=self.source.as_ref().ok_or_else(changed)?;
        source.require_source_current(c,self.current,self.config,self.clock).await?;
        self.plan.as_ref().ok_or_else(changed)?.require_after(c).await?;
        self.notice.as_ref().ok_or_else(changed)?.require_current(c).await?;
        let next=read_field_journal_flow(c,self.current,&RecordId::new(&r.record_id)?,self.config,self.clock).await?;
        if next.view["canManage"]!=true||next.view["steps"][0]["candidates"]!=source.view["steps"][0]["candidates"]{return Err(changed().into())}
        next.require_current(c,self.current,self.config,self.clock).await
    }
}
pub async fn execute_field_journal_flow_assignment<C,F,P>(
    authorizer:&HoldingWorkAuthorizer,host:&str,proof:DeviceProof,selection:F,
    config:FinanceJournalCreateConfiguration,clock:Arc<C>,prepare:P,
)->Result<CommandJson,Error>
where C:Clock+Send+Sync+'static,F:FnOnce()->Result<HoldingWorkspaceRequest,BusinessError>+Send+'static,
P:Fn()->Result<WorkflowRequest,BusinessError>+Clone+Send+Sync+'static{
    let company=config.company.clone();let sources=Sources::new(&config)?;
    Box::pin(authorizer.execute_with_sources_deferred(host,proof,move||{let mut s=selection()?;if s.company_id!=company{return Err(changed())}s.mutation=true;Ok(s)},WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/journals".into(),api:true},sources,move|c,current|{
        let(config,clock,prepare)=(config.clone(),clock.clone(),prepare.clone());
        Box::pin(async move{
            let request=prepare()?;let sample=Sample(clock.now());
            let mut authority=Authority{current,config:&config,clock:clock.as_ref()};
            let mut effects=Effects{current,config:&config,clock:clock.as_ref(),now:sample.0.get(),source:None,plan:None,notice:None};
            let mut ids=Ids;
            let result=PgCommandLedger::new(config.company.clone(),c,&mut authority,&sample,&mut ids).execute(&request,&mut effects).await?;
            if !result.replayed{
                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
                effects.retain(c,&command_actor(current)?,&request).await?;
                authority.verify_command(c,&command_actor(current)?,&request,CommandDisposition::Execute).await?;
            }
            Ok(result.result)
        })
    })).await
}
