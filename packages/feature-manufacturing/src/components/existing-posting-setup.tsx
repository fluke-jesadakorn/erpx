'use client';
import {useCallback,useEffect,useLayoutEffect,useRef,useState} from 'react';
import {Button,Card,Field,InlineNotice,TextArea} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
import {ManufacturingForm} from '@erpx/feature-manufacturing/components/draft-boundary';
import {registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import {useAccountPreferences} from '@erpx/feature-identity_access/components/account-preferences-context';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import {manufacturingPostingAccountFields} from '@erpx/feature-manufacturing/lib/posting-setup-command';
import {createManufacturingExistingPostingCommands,readManufacturingExistingPostingSetup,type ManufacturingExistingPostingSource} from '@erpx/feature-manufacturing/lib/existing-posting-setup-command';
import type {ManufacturingExistingPostingSetupView} from '@erpx/server-commerce/public/manufacturing-existing-posting-setup';
const owner=(source:ManufacturingExistingPostingSource)=>JSON.stringify([source.base,source.companyId]);
const scope=(source:ManufacturingExistingPostingSource)=>JSON.stringify([source.base,source.companyId,source.contextKey]);
const inactive=()=>{};
const labels={receivableAccountId:['ลูกหนี้การค้า','Receivables'],payableAccountId:['เจ้าหนี้การค้า','Payables'],revenueAccountId:['รายได้','Revenue'],inventoryAccountId:['สินค้าคงเหลือ','Inventory'],clearingAccountId:['บัญชีพักรับสินค้า','Receipt clearing'],cogsAccountId:['ต้นทุนขาย','Cost of goods sold'],inputTaxAccountId:['ภาษีซื้อ','Input tax'],outputTaxAccountId:['ภาษีขาย','Output tax'],cashAccountId:['เงินสด / ธนาคาร','Cash / bank']} as const;
export function ManufacturingExistingPostingSetup({base,initial,canPrepareInitialStock=false,disabled=false,onActiveChange=inactive,onChanged=inactive}:{base:string;initial:ManufacturingExistingPostingSetupView;canPrepareInitialStock?:boolean;disabled?:boolean;onActiveChange?:(active:boolean)=>void;onChanged?:()=>void}){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,english:string)=>th?thai:english;
 const [refreshed,setRefreshed]=useState<{base:string;initial:ManufacturingExistingPostingSetupView;view:ManufacturingExistingPostingSetupView}|null>(null),view=refreshed?.base===base&&refreshed.initial===initial&&refreshed.view.companyId===initial.companyId?refreshed.view:initial;
 const current={base,companyId:view.companyId,contextKey:view.contextKey},source=useRef<ManufacturingExistingPostingSource|null>(current);useLayoutEffect(()=>{source.current={base,companyId:view.companyId,contextKey:view.contextKey};return()=>{source.current=null;};},[base,view.companyId,view.contextKey]);
 const commands=useRef<ReturnType<typeof createManufacturingExistingPostingCommands>|null>(null);commands.current??=createManufacturingExistingPostingCommands(()=>source.current);
 const [reason,setReason]=useState(''),[seed,setSeed]=useState(current),[open,setOpen]=useState(false),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[applied,setApplied]=useState(false),[error,setErrorValue]=useState(''),[errorScope,setErrorScope]=useState(''),[notice,setNotice]=useState<{owner:string;text:string}|null>(null);
 const formRef=useRef<HTMLFormElement>(null),trigger=useRef<HTMLButtonElement>(null),busyRef=useRef(false),reads=useRef(0),revision=useRef(0),dirty=reason.length>0||uncertain,effectiveSeed=!dirty&&!busy?current:seed,changed=scope(effectiveSeed)!==scope(current),sameOwner=owner(effectiveSeed)===owner(current);
 useEffect(()=>registerUnsavedWork(()=>dirty||busy),[dirty,busy]);useLayoutEffect(()=>onActiveChange(open||dirty||busy||uncertain),[onActiveChange,open,dirty,busy,uncertain]);useEffect(()=>()=>onActiveChange(false),[onActiveChange]);
 const setError=useCallback((message:string)=>{setErrorValue(message);setErrorScope(scope({base,companyId:view.companyId,contextKey:view.contextKey}));},[base,view.companyId,view.contextKey]),visibleError=errorScope===scope(current)?error:'';
 const reload=useCallback(async()=>{const request=++reads.current;try{const fresh=await readManufacturingExistingPostingSetup(()=>source.current);if(request===reads.current){setRefreshed({base,initial,view:fresh});setError('');}}catch(cause){if(request===reads.current)setError(cause instanceof Error?cause.message:'Unable to preview the existing mapping.');}},[base,initial,setError]);
 const canStart=view.available&&view.schemaAvailable&&view.canConfirm&&!view.confirmed&&!disabled&&!changed&&!uncertain;
 function edit(value:string){if(!dirty&&!busy)setSeed(current);revision.current++;setReason(value);}
 function discard(){if(commands.current!.pending()){setError(t('กรุณายืนยันผลคำขอเดิมก่อนละทิ้งร่าง','Confirm the original request before discarding its draft.'));return;}setApplied(false);setReason('');revision.current++;if(formRef.current)window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:formRef.current}}));setOpen(false);setError('');setSeed(current);}
 async function submit(form:HTMLFormElement,retry=false){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');setNotice(null);const capturedRevision=revision.current;
  try{
   if(!retry&&!canStart)throw Error(t('เก็บร่างไว้และตรวจสอบการตั้งค่าปัจจุบันก่อน','Keep the draft and preview the current mapping first.'));
   const outcome=await(retry?commands.current!.retry():commands.current!.submit({expectedVersion:0,contextKey:effectiveSeed.contextKey??'',reason},{form,formEditVersion:form.getAttribute('data-erpx-edit-version')})),unchanged=outcome.unchanged()&&capturedRevision===revision.current;
   if(!outcome.acknowledge())throw Error(t('ยังยืนยันผลคำขอเดิมไม่ได้','The original request is still unresolved.'));
   setRefreshed({base,initial,view:outcome.view});setUncertain(false);setApplied(false);setNotice({owner:owner(current),text:unchanged?t('ยืนยันการใช้บัญชีเดิมสำหรับการผลิตแล้ว','Existing posting accounts confirmed for Manufacturing.'):t('ยืนยันคำขอเดิมแล้ว เหตุผลที่แก้ใหม่ยังอยู่','The original request is confirmed. Your newer reason is retained.')});
   if(unchanged){setReason('');revision.current++;setOpen(false);setSeed({base,companyId:outcome.view.companyId,contextKey:outcome.view.contextKey});}onChanged();
  }catch(cause){setUncertain(commands.current!.pending());setApplied(commands.current!.applied());setError(cause instanceof Error?cause.message:'The existing mapping confirmation is unresolved.');}finally{busyRef.current=false;setBusy(false);}
 }
 if(!view.available||!view.configured||view.unavailableReason==='already-manufacturing-configured')return null;
 const configuration=view.configuration;
 return <Card appearance="standard"><h2>{t('ตรวจสอบบัญชีเดิมสำหรับการผลิต','Review existing accounts for Manufacturing')}</h2>
  <p>{t('ตรวจสอบการตั้งค่าที่บริษัทใช้อยู่ แล้วระบุเหตุผลเพื่อยืนยันการใช้บัญชีเดิมสำหรับการผลิต','Review the Company’s existing posting accounts and provide a reason to confirm them for Manufacturing.')}</p>
  {configuration&&<dl><div><dt>{t('ผังบัญชี','Chart')}</dt><dd>{view.previewChart?`${view.previewChart.name} · ${view.previewChart.id}`:configuration.chartId}</dd></div><div><dt>{t('สกุลเงินเอกสาร','Document currency')}</dt><dd>{configuration.currency}</dd></div>{manufacturingPostingAccountFields.map(field=><div key={field}><dt>{labels[field][th?0:1]}</dt><dd>{(()=>{const account=view.previewAccounts.find(item=>item.id===configuration[field]);return account?`${account.code} · ${account.name} · ${account.currency} · ${account.id}`:configuration[field];})()}</dd></div>)}</dl>}
  {view.effectiveSalesConfiguration&&<div><h3>{t('บัญชีการขายที่ใช้จริงตามฉบับอนุมัติ','Effective accounts from approved Sales defaults')}</h3><dl>{(['receivableAccountId','revenueAccountId','outputTaxAccountId'] as const).map(field=><div key={field}><dt>{labels[field][th?0:1]}</dt><dd>{view.effectiveSalesConfiguration?.[field]??t('ไม่มีภาษีขายตามฉบับอนุมัติ','No output VAT in the approved defaults')}</dd></div>)}</dl></div>}
  {view.expectedSalesDefaults&&<p>{t('บัญชีการขายยังใช้ฉบับที่อนุมัติแล้วตามเดิม','Sales continues to use its existing approved defaults.')} · {view.expectedSalesDefaults.id} · {view.expectedSalesDefaults.version}</p>}
  {view.unavailableReason==='schema-required'&&<InlineNotice role="status">{t('ยังไม่พร้อมยืนยันการตั้งค่าที่มีอยู่','Existing mapping confirmation is not yet available.')}</InlineNotice>}
  {view.unavailableReason==='invalid-existing-mapping'&&<InlineNotice role="status">{t('ผู้ดูแลบัญชีต้องตรวจสอบความถูกต้องของการตั้งค่าเดิมก่อน','The Finance owner must reconcile the existing mapping before confirmation.')}</InlineNotice>}
  {view.confirmed&&<InlineNotice role="status">{t('มีหลักฐานยืนยันบัญชีเดิมสำหรับการผลิตแล้ว','The existing accounts have been confirmed for Manufacturing.')}</InlineNotice>}
  {view.confirmed&&canPrepareInitialStock&&<p><WorkspaceLink href={`${base}/production/manufacturing/settings/initial-stock`}>{t('เตรียมผลนับสินค้าจริงครั้งแรก','Prepare the initial physical stock count')}</WorkspaceLink></p>}
  {notice?.owner===owner(current)&&<InlineNotice role="status">{notice.text}</InlineNotice>}{visibleError&&!open&&<InlineNotice role="alert" tone="danger">{visibleError}</InlineNotice>}
  <div className="ui-form-actions">{(view.canConfirm||dirty)&&<Button appearance="standard" ref={trigger} type="button" disabled={busy||disabled} onClick={()=>setOpen(true)}>{t('ตรวจสอบและยืนยันบัญชีเดิม','Review and confirm existing accounts')}</Button>}<Button appearance="standard" type="button" disabled={busy} onClick={()=>void reload()}>{t('โหลดข้อมูลปัจจุบันและเก็บร่างไว้','Reload current mapping and keep draft')}</Button></div>
  <DraftFormDrawer appearance="standard" title={t('ยืนยันบัญชีเดิมสำหรับการผลิต','Confirm existing Manufacturing accounts')} open={open} dirty={dirty} retainFormContents={dirty} dismissDisabled={busy||uncertain} onClose={()=>setOpen(false)} onDiscard={discard} finalFocus={()=>trigger.current}>
   {visibleError&&<InlineNotice role="alert" tone="danger">{visibleError}</InlineNotice>}{changed&&<InlineNotice role="status">{t('ร่างยังผูกกับข้อมูลเดิม กรุณาตรวจสอบข้อมูลปัจจุบันก่อน','This draft retains its original references. Review the current mapping before continuing.')}</InlineNotice>}
   {applied&&<InlineNotice role="status">{t('ได้รับหลักฐานยืนยันแล้ว ใช้คำขอเดิมเพื่อตรวจสอบผล','A confirmation receipt was received. Use the original request to check its outcome.')}</InlineNotice>}
   <ManufacturingForm ref={formRef} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}><fieldset disabled={busy||!canStart} style={{display:'contents'}}><Field appearance="standard">{t('เหตุผลที่ตรวจสอบแล้ว','Reviewed reason')}<TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000} value={reason} onChange={event=>edit(event.target.value)}/></Field></fieldset><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!canStart||reason.trim().length<3}>{t('ยืนยันการใช้บัญชีเดิมสำหรับการผลิต','Confirm existing accounts for Manufacturing')}</Button></ManufacturingForm>
   {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>{t('ลองยืนยันคำขอเดิม','Retry original confirmation')}</Button>}
   {dirty&&!uncertain&&!busy&&sameOwner&&changed&&view.canConfirm&&<Button appearance="standard" type="button" onClick={()=>setSeed(current)}>{t('ตรวจสอบข้อมูลปัจจุบันและเก็บร่างไว้','Review current mapping and keep draft')}</Button>}
  </DraftFormDrawer>
 </Card>;
}
