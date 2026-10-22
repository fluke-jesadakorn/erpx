"use client";
import {useState,type ReactNode} from 'react';
import {Button,Field,Select,TextArea,TextInput,InlineNotice,FormLayout} from '@erpx/ui/components/foundation';
import {AttachmentPanel,type AttachmentPanelChange} from '@erpx/feature-files/components/attachment-panel';
import {RecordDocumentPanel} from '@erpx/feature-files/components/record-document-panel';
import type {benefitDetail,benefitsWorkspace} from '@erpx/server-benefits/public/benefits';
import {benefitActionAllowed,type BenefitPresentation} from '@erpx/feature-benefits/lib/presentation';
import type {BenefitKind} from '@erpx/feature-benefits/lib/command';
import {BenefitDraftBoundary} from '@erpx/feature-benefits/components/benefit-draft-boundary';
import {BenefitEditor} from '@erpx/feature-benefits/components/benefit-editor';
import {BenefitCommandFeedback} from '@erpx/feature-benefits/components/benefit-command-feedback';
import {useBenefitCommand} from '@erpx/feature-benefits/components/use-benefit-command';
type Command=ReturnType<typeof useBenefitCommand>;
type Row=Record<string,string|number|null>;
const labels:Record<string,string>={publish:'Publish plan',reject:'Reject',retire:'Retire plan',open:'Open window',close:'Close window',cancel:'Cancel',verify:'Verify dependent',remove:'Remove dependent',submit:'Submit election',acknowledge:'Record provider acknowledgement',release:'Release payroll input'};
function Transition({base,view,command,kind,row,action,canManageDocuments}:{base:string;view:BenefitPresentation;command:Command;kind:BenefitKind;row:Row;action:string;canManageDocuments:boolean}){
 const [files,setFiles]=useState<AttachmentPanelChange|null>(null),input={entity:kind,id:row.id,action,expectedVersion:Number(row.version)};
 if(!benefitActionAllowed({...view,purchase:undefined},input))return null;
 const evidence=kind==='dependent'&&action==='verify'?{target:'benefit_dependent',purpose:'dependent_proof',ref:'verification',title:'Dependent verification proof'}:kind==='election'&&action==='submit'?{target:'benefit_election',purpose:'election_document',ref:'employee-submit',title:'Election documents'}:kind==='election'&&action==='acknowledge'?{target:'benefit_election',purpose:'provider_proof',ref:'provider-acknowledgement',title:'Provider acknowledgement proof'}:null;
 const blocked=command.busy||!!command.retryAttempt||!!command.savedDraft||!benefitActionAllowed(view,input);
 return <BenefitEditor title={`${labels[action]} · ${String(row.name??row.plan_name??row.id)}`} busy={command.busy} recovery={<BenefitCommandFeedback command={command} view={view}/>} error={command.error} onReset={()=>setFiles(null)}><FormLayout data-benefit-draft className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=event.currentTarget,fields=new FormData(form);void command.send({...input,reason:fields.get('reason'),...(action==='acknowledge'?{providerReference:fields.get('providerReference')}:{}),attachmentSetIds:files?.setId&&files.readyCount>0&&files.bindable?[files.setId]:[]},form);}}>
  {action==='acknowledge'&&<Field appearance="standard">Provider reference<TextInput appearance="standard" name="providerReference" minLength={2} maxLength={160} required/></Field>}
  <Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} maxLength={3000} required/></Field>
  {evidence&&(action==='submit'||canManageDocuments)&&<AttachmentPanel appearance="standard" key={`${row.id}:${row.version}:${action}`} base={base} targetType={evidence.target} targetId={String(row.id)} expectedTargetVersion={Number(row.version)} componentKind="action" componentRef={evidence.ref} purposeKey={evidence.purpose} title={evidence.title} description="Attach the evidence reviewed for this action. Clean files are locked with the original source revision." visibility="Restricted to the employee owner and dedicated Benefits document roles under the original file policy." onChange={setFiles}/>}
  <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={blocked}>{labels[action]}</Button>
 </FormLayout></BenefitEditor>;
}
export function BenefitsConsole({recordOnly=false,section,embedded=false,base,data,detail}:{recordOnly?:boolean;section?:'plans'|'enrollments';embedded?:boolean;base:string;data?:Awaited<ReturnType<typeof benefitsWorkspace>>;detail?:Awaited<ReturnType<typeof benefitDetail>>}){
 const source=data??detail,view:BenefitPresentation={actorId:source?.actorId??'',canWrite:source?.canWrite??false,canReview:source?.canReview??false,purchase:source?.purchase,plans:data?.plans,windows:data?.windows,dependents:data?.dependents,election:detail?.election,isOwner:detail?.isOwner},command=useBenefitCommand(base,view,source?.workContext??'unavailable'),newWork=!source?.purchase||source.purchase.newWork,settleExisting=!source?.purchase||source.purchase.settleExisting,blocked=command.busy||!!command.retryAttempt||!!command.savedDraft;
 const editor=(title:string,children:ReactNode)=><BenefitEditor title={title} busy={command.busy} recovery={<BenefitCommandFeedback command={command} view={view}/>} error={command.error}>{children}</BenefitEditor>;
 const rows=(kind:BenefitKind,values:Row[],actions:string[])=>values.map(row=><div key={`${kind}:${row.id}`} className="ui-stack">{actions.map(action=><Transition key={`${row.id}:${action}`} base={base} view={view} command={command} kind={kind} row={row} action={action} canManageDocuments={source?.canManageDocuments??false}/>)}</div>);
 return <BenefitDraftBoundary dirty={!!command.retryAttempt||!!command.savedDraft}><div className="ui-stack" data-owner-embedded={embedded||undefined}>
  {source?.purchase&&!newWork&&<InlineNotice tone="info">Benefits history remains available. {settleExisting?'Existing enrollments and provider work can be completed.':'This package is read only.'}</InlineNotice>}
  {command.error&&<InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
  <BenefitCommandFeedback command={command} view={view}/>
  {!recordOnly&&section!=='enrollments'&&data?.canReview&&data.canWrite&&<>
   {editor('Draft benefit plan',<FormLayout data-benefit-draft className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=event.currentTarget,fields=Object.fromEntries(new FormData(form));void command.send({...fields,entity:'plan',effectiveTo:fields.effectiveTo||null,requiredEligibilityCode:fields.requiredEligibilityCode||null},form);}}>
    <Field appearance="standard">Name<TextInput appearance="standard" name="name" required/></Field><Field appearance="standard">Provider<TextInput appearance="standard" name="provider" required/></Field>
    <Field appearance="standard">Coverage<Select appearance="standard" name="coverageType"><option value="employee">Employee</option><option value="family">Family</option></Select></Field>
    <Field appearance="standard">Currency<TextInput appearance="standard" name="currency" defaultValue="THB" required/></Field>
    <Field appearance="standard">Employee deduction<TextInput appearance="standard" name="employeeDeduction" inputMode="decimal" required/></Field><Field appearance="standard">Employer cost<TextInput appearance="standard" name="employerCost" inputMode="decimal" required/></Field>
    <Field appearance="standard">Effective from<TextInput appearance="standard" name="effectiveFrom" type="date" required/></Field><Field appearance="standard">Effective to<TextInput appearance="standard" name="effectiveTo" type="date"/></Field>
    <Field appearance="standard">Required learning eligibility code, when applicable<TextInput appearance="standard" name="requiredEligibilityCode" maxLength={120}/></Field>
    <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={blocked||!newWork}>Draft plan</Button>
   </FormLayout>)}
   {editor('Draft enrollment window',<FormLayout data-benefit-draft className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=event.currentTarget;void command.send({...Object.fromEntries(new FormData(form)),entity:'window'},form);}}>
    <Field appearance="standard">Name<TextInput appearance="standard" name="name" required/></Field><Field appearance="standard">Opens<TextInput appearance="standard" name="opensOn" type="date" required/></Field><Field appearance="standard">Closes<TextInput appearance="standard" name="closesOn" type="date" required/></Field><Field appearance="standard">Coverage effective<TextInput appearance="standard" name="effectiveOn" type="date" required/></Field>
    <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={blocked||!newWork}>Draft window</Button>
   </FormLayout>)}
  </>}
  {data&&section!=='enrollments'&&<>{rows('plan',data.plans,['publish','reject','retire'])}{rows('window',data.windows,['open','cancel','close'])}</>}
  {!recordOnly&&section!=='plans'&&data?.canWrite&&editor('Add dependent',<FormLayout data-benefit-draft className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=event.currentTarget;void command.send({...Object.fromEntries(new FormData(form)),entity:'dependent'},form);}}>
   <Field appearance="standard">Name<TextInput appearance="standard" name="name" required/></Field><Field appearance="standard">Relationship<TextInput appearance="standard" name="relationship" required/></Field><Field appearance="standard">Birth date<TextInput appearance="standard" name="birthDate" type="date" required/></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={blocked||!newWork}>Submit dependent</Button>
  </FormLayout>)}
  {data?.plans.filter(plan=>['draft','published','retired'].includes(String(plan.status))).map(plan=><RecordDocumentPanel appearance="standard" key={`booklet:${plan.id}`} base={base} targetType="benefit_plan" targetId={String(plan.id)} expectedTargetVersion={Number(plan.version)} editable={data.canManageDocuments&&newWork} title={`${plan.name} · plan documents`} purposes={[{key:'benefit_booklet',label:'Benefit booklet',description:'Attach the reviewed plan booklet or coverage guide for this plan revision.',visibility:'Published booklets are self-visible to active employees; drafts remain restricted to Benefits document managers.'}]}/>)}
  {data?.dependents.map(dependent=><RecordDocumentPanel appearance="standard" key={`dependent-proof:${dependent.id}`} base={base} targetType="benefit_dependent" targetId={String(dependent.id)} expectedTargetVersion={Number(dependent.version)} editable={dependent.status==='pending'&&settleExisting} title={`${dependent.name} · dependent proof`} purposes={[{key:'dependent_proof',label:'Dependent proof',description:'Attach relationship, identity, or provider eligibility evidence.',visibility:'Only the employee owner and dedicated Benefits document roles can access these files.'}]}/>)}
  {data&&rows('dependent',data.dependents,['verify','reject','remove'])}
  {!recordOnly&&section!=='plans'&&data?.canWrite&&editor('Elect coverage',<FormLayout data-benefit-draft className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=event.currentTarget,fields=new FormData(form),plan=data.plans.find(row=>row.id===fields.get('planId')),window=data.windows.find(row=>row.id===fields.get('windowId'));void command.send({entity:'election',planId:fields.get('planId'),planVersion:Number(plan?.version),windowId:fields.get('windowId'),windowVersion:Number(window?.version),dependentIds:fields.getAll('dependentIds')},form);}}>
   <Field appearance="standard">Plan<Select appearance="standard" name="planId" required>{data.plans.filter(row=>row.status==='published').map(row=><option key={String(row.id)} value={String(row.id)}>{row.name}</option>)}</Select></Field>
   <Field appearance="standard">Window<Select appearance="standard" name="windowId" required>{data.windows.filter(row=>row.status==='open').map(row=><option key={String(row.id)} value={String(row.id)}>{row.name}</option>)}</Select></Field>
   <fieldset><legend>Verified dependents</legend>{data.dependents.filter(row=>row.status==='verified').map(row=><Field appearance="standard" orientation="inline" key={String(row.id)}><TextInput appearance="standard" type="checkbox" name="dependentIds" value={String(row.id)}/>{row.name}</Field>)}</fieldset>
   <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={blocked||!newWork||!data.plans.some(row=>row.status==='published')||!data.windows.some(row=>row.status==='open')}>Elect plan</Button>
  </FormLayout>)}
  {detail&&rows('election',[detail.election],['submit','cancel','acknowledge','release'])}
 </div></BenefitDraftBoundary>;
}
