'use client';
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@/lib/shared/destination-router';
import {Button,Card,Field,InlineNotice,Select,TextInput} from '@/components/ui/foundation';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {CommerceForm} from './commerce-draft-boundary';
import {useAccountPreferences} from '@/components/identity_access/account-preferences-context';
import {registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {postingSetupViewState} from '@/lib/commerce/posting-setup-command';
import {createPostingPrerequisiteCommands,type PostingPrerequisiteKind,type PostingPrerequisiteRequest,type PostingPrerequisiteInputs} from '@/lib/commerce/posting-setup-prerequisite-command';
import type {CommercePostingSetupView} from '@/server/modules/commerce/public/posting-setup';
import type {SupplyPostingPrerequisitesView} from '@/server/modules/finance/public/supply-posting-prerequisites';

const categories=['asset','liability','equity','revenue','expense','other_income','other_expense'] as const,controls=['none','receivable','payable','bank'] as const;
const isCategory=(value:string):value is PostingPrerequisiteInputs['accounts']['category']=>categories.some(item=>item===value);
const isControl=(value:string):value is PostingPrerequisiteInputs['accounts']['controlKind']=>controls.some(item=>item===value);
const labels={charts:['สร้างผังบัญชี','Create a chart'],accounts:['เพิ่มบัญชีจริง','Add an actual account'],periods:['เปิดงวดบัญชี','Create an open period'],rates:['ยืนยันอัตราแลกเปลี่ยน','Confirm a manual rate']} as const;
type Props={base:string;initial:SupplyPostingPrerequisitesView;disabled?:boolean;onPrepared:(view:CommercePostingSetupView,prerequisites:SupplyPostingPrerequisitesView)=>void;onActiveChange:(active:boolean)=>void};
export function PostingSetupPrerequisites({base,initial,disabled=false,onPrepared,onActiveChange}:Props){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,english:string)=>th?thai:english,router=useRouter(),formRef=useRef<HTMLFormElement>(null),trigger=useRef<HTMLButtonElement>(null),busyRef=useRef(false);
 const [refreshed,setRefreshed]=useState<{base:string;initial:SupplyPostingPrerequisitesView;value:SupplyPostingPrerequisitesView}|null>(null),view=refreshed?.base===base&&refreshed.initial===initial&&refreshed.value.companyId===initial.companyId?refreshed.value:initial;
 const source=useRef({base,companyId:view.companyId,contextKey:view.contextKey});useLayoutEffect(()=>{source.current={base,companyId:view.companyId,contextKey:view.contextKey};},[base,view.companyId,view.contextKey]);
 const commands=useRef<ReturnType<typeof createPostingPrerequisiteCommands>|null>(null);commands.current??=createPostingPrerequisiteCommands(()=>source.current);
 const [kind,setKind]=useState<PostingPrerequisiteKind|null>(null),[open,setOpen]=useState(false),[fields,setFields]=useState<Record<string,string>>({}),[seed,setSeed]=useState(view.contextKey),[scope,setScope]=useState(()=>JSON.stringify([base,view.companyId,view.contextKey])),[owner,setOwner]=useState(()=>JSON.stringify([base,view.companyId]));
 const [busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState<{owner:string;text:string}|null>(null),[applied,setApplied]=useState(false);
 const dirty=Object.values(fields).some(Boolean)||uncertain,draft=postingSetupViewState({base,companyId:view.companyId,contextKey:view.contextKey},{saved:null,notice:'',seed,draftScope:scope,dirty,pending:busy||uncertain}),changed=draft.draftScope!==draft.currentScope,currentOwner=JSON.stringify([base,view.companyId]);
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 useEffect(()=>()=>onActiveChange(false),[onActiveChange]);
 useLayoutEffect(()=>{onActiveChange(open||dirty||busy);},[onActiveChange,open,dirty,busy]);
 const permissions={charts:view.canCreateChart,accounts:view.canCreateAccount,periods:view.canCreatePeriod,rates:view.canConfirmRate},allowed=!!kind&&permissions[kind]&&view.available&&!disabled&&!changed;
 function change(name:string,value:string){setSeed(draft.seed);setScope(draft.draftScope);if(!dirty&&!busy)setOwner(currentOwner);setFields(previous=>({...previous,[name]:value}));}
 function start(next:PostingPrerequisiteKind){if(dirty||busy){setError(t('เก็บร่างเดิมไว้แล้ว กรุณาปิดหรือยืนยันคำขอเดิมก่อน','Your original draft is retained. Finish or discard it before another preparation.'));setOpen(true);return;}setKind(next);setOpen(true);setFields({});setSeed(view.contextKey);setScope(draft.currentScope);setOwner(currentOwner);setError('');setNotice(null);setApplied(false);}
 function discard(){if(commands.current?.pending()){setError(t('ผลของคำขอเดิมยังไม่แน่นอน กรุณาลองคำขอเดิมก่อน','The submitted request is unresolved. Retry it before discarding.'));return;}setFields({});setOpen(false);setError('');setSeed(view.contextKey);setScope(draft.currentScope);}
 function request():PostingPrerequisiteRequest{
  const envelope={expectedVersion:0 as const,contextKey:draft.seed??''};
  if(kind==='charts')return {kind,body:{...envelope,input:{name:fields.name??'',baseCurrency:fields.baseCurrency??'',accountingBasis:fields.accountingBasis??''}}};
  if(kind==='periods')return {kind,body:{...envelope,input:{name:fields.name??'',startDate:fields.startDate??'',endDate:fields.endDate??''}}};
  if(kind==='rates')return {kind,body:{...envelope,input:{currency:fields.currency??'',effectiveDate:fields.effectiveDate??'',currencyPerUsd:fields.currencyPerUsd??'',note:fields.note??''}}};
  const category=fields.category??'',controlKind=fields.controlKind??'';if(kind!=='accounts'||!isCategory(category)||!isControl(controlKind))throw Error(t('เลือกประเภทและบทบาทบัญชีจริง','Choose the actual account category and control role.'));
  return {kind,body:{...envelope,input:{chartId:fields.chartId??'',code:fields.code??'',name:fields.name??'',category,currency:fields.currency??'',controlKind,isControl:controlKind!=='none'}}};
 }
 async function submit(form:HTMLFormElement,retry=false){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');setNotice(null);
  try{
   if(!retry&&!allowed)throw Error(t('บริบทหรือสิทธิ์เปลี่ยน เก็บร่างเดิมไว้แล้ว','Your preparation context changed. The draft is retained.'));
   const outcome=await(retry?commands.current!.retry():commands.current!.submit(request(),{form,formEditVersion:form.getAttribute('data-erpx-edit-version')})),unchanged=outcome.unchanged();
   if(!outcome.acknowledge())throw Error(t('กรุณาลองยืนยันคำขอเดิม','Retry the original confirmation.'));
   setRefreshed({base,initial,value:outcome.prerequisites});onPrepared(outcome.posting,outcome.prerequisites);setUncertain(false);setApplied(false);
   setNotice({owner:currentOwner,text:unchanged?t('บันทึกและโหลดข้อมูลปัจจุบันแล้ว เลือกบัญชีการลงรายการด้วยตนเองได้','Saved and current choices reloaded. Select posting accounts deliberately.'):t('บันทึกคำขอเดิมแล้ว การแก้ไขใหม่ยังเก็บไว้ กรุณาตรวจสอบข้อมูลปัจจุบันก่อนส่งครั้งถัดไป','The submitted request was saved. Newer edits are retained; review current references before another submission.')});
   if(unchanged){setFields({});setOpen(false);}
  }catch(cause){setUncertain(commands.current!.pending());setApplied(commands.current!.applied());setError(cause instanceof Error?cause.message:t('ยังยืนยันคำขอไม่ได้ กรุณาลองคำขอเดิม','Unable to confirm preparation. Retry the original request.'));}
  finally{busyRef.current=false;setBusy(false);}
 }
 const input=(name:string,thai:string,english:string,options:{max?:number;type?:string;required?:boolean;currency?:boolean}={})=><Field appearance="standard">{t(thai,english)}<TextInput appearance="standard" name={name} value={fields[name]??''} onChange={event=>change(name,options.currency?event.target.value.toUpperCase():event.target.value)} required={options.required!==false} maxLength={options.max} minLength={options.currency?3:undefined} pattern={options.currency?'[A-Z]{3}':undefined} type={options.type??'text'}/></Field>;
 return <Card as="section" appearance="standard"><h2>{t('เตรียมบัญชีสำหรับการรับสินค้า','Prepare accounts for receiving')}</h2><p>{t('สร้างเฉพาะข้อมูลจริงที่ยังขาด ไม่มีการสร้างบัญชีหรือตั้งอัตราให้อัตโนมัติ การยืนยันอัตราต้องใช้สิทธิ์ผู้อนุมัติการเงินระดับบริษัท','Create only the actual missing prerequisites. No accounts or rates are supplied automatically. Rate confirmation requires an authorized Company Finance approver.')}</p>
  {!view.available&&<InlineNotice role="status">{view.unavailableReason==='purchase-required'?t('ต้องมีสิทธิ์ซื้อการรับสินค้าและการประเมินมูลค่าสินค้า','Receiving and inventory valuation purchases are required.'):t('ใช้สิทธิ์อ่านและจัดเตรียมหรืออนุมัติการเงินระดับบริษัท','Use a current Company Finance reader with preparation or approval authority.')}</InlineNotice>}
  {view.unavailableReason==='history-requires-review'&&<InlineNotice role="status">{t('ประวัติเดิมต้องให้เจ้าของตรวจสอบ ไม่สามารถสร้างผังบัญชีหรือเปลี่ยนการผูกบัญชีทดแทนได้ การเปิดงวดและยืนยันอัตรายังคงใช้สิทธิ์เดิม','Existing history needs owner review; charts and posting mappings cannot be reconstructed here. Open-period and rate preparation retain their original authority.')}</InlineNotice>}
  {view.available&&<><p>{t('ผังบัญชี','Charts')}: {view.charts.length} · {t('บัญชีจริง','Accounts')}: {view.accounts.length}</p><ul>{view.periods.map(period=><li key={period.id}>{period.name} · {period.startDate} — {period.endDate} · {period.state}</li>)}{view.rates.map(rate=><li key={rate.id}>{rate.currency} · 1 USD = {rate.currencyPerUsd} · {rate.effectiveDate}</li>)}</ul></>}
  {notice?.owner===currentOwner&&<InlineNotice role="status">{notice.text}</InlineNotice>}{error&&!open&&<InlineNotice role="alert" tone="danger">{error}</InlineNotice>}
  {(disabled&&!open)&&<InlineNotice role="status">{t('ปิดหรือยืนยันร่างการตั้งค่าบัญชีเดิมก่อนเตรียมข้อมูลเพิ่ม','Finish or discard the current posting draft before preparing more references.')}</InlineNotice>}
  <div className="ui-form-actions">{(Object.keys(labels) as PostingPrerequisiteKind[]).filter(item=>permissions[item]).map(item=><Button key={item} appearance="standard" type="button" disabled={disabled||busy||dirty||item==='accounts'&&view.charts.length===0} onClick={event=>{trigger.current=event.currentTarget;start(item);}}>{labels[item][th?0:1]}</Button>)}{dirty&&!open&&<Button appearance="standard" type="button" onClick={()=>setOpen(true)}>{t('เปิดร่างเดิม','Reopen retained draft')}</Button>}</div>
  <DraftFormDrawer appearance="standard" title={kind?labels[kind][th?0:1]:t('เตรียมบัญชี','Prepare accounts')} open={open} dirty={dirty} retainFormContents={dirty} dismissDisabled={busy} finalFocus={()=>trigger.current} onClose={()=>setOpen(false)} onDiscard={discard}>
   {error&&<InlineNotice role="alert" tone="danger">{error}</InlineNotice>}{applied&&<InlineNotice role="status">{t('ได้รับใบยืนยันการบันทึกแล้ว กรุณาโหลดสถานะปัจจุบันด้วยคำขอเดิม ไม่ต้องสร้างรายการใหม่','A saved receipt was received. Reload its current status with the original request; do not create another record.')}</InlineNotice>}
   {changed&&<InlineNotice role="status">{t('เก็บร่างไว้ในบริษัทและบริบทเดิม กรุณากลับไปบริบทเดิมหรือตรวจสอบข้อมูลปัจจุบัน','The draft retains its original Company and context. Return to it or explicitly review current references.')}</InlineNotice>}
   <CommerceForm ref={formRef} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}><fieldset disabled={busy||!allowed} style={{display:'contents'}}>
    {kind==='charts'&&<>{input('name','ชื่อผังบัญชี','Chart name',{max:160})}{input('baseCurrency','สกุลเงินหลัก','Base currency',{max:3,currency:true})}{input('accountingBasis','มาตรฐานการบัญชี','Accounting basis',{max:40})}</>}
    {kind==='accounts'&&<><Field appearance="standard">{t('ผังบัญชีจริง','Actual chart')}<Select appearance="standard" name="chartId" required value={fields.chartId??''} onChange={event=>change('chartId',event.target.value)}><option value="" disabled>{t('เลือกผังบัญชี','Choose a chart')}</option>{view.charts.map(chart=><option key={chart.id} value={chart.id}>{chart.name} · {chart.baseCurrency}</option>)}</Select></Field>{input('code','รหัสบัญชี','Account code',{max:40})}{input('name','ชื่อบัญชี','Account name',{max:160})}<Field appearance="standard">{t('ประเภทบัญชี','Account category')}<Select appearance="standard" name="category" required value={fields.category??''} onChange={event=>change('category',event.target.value)}><option value="" disabled>{t('เลือกประเภทจริง','Choose the actual category')}</option>{categories.map(value=><option key={value} value={value}>{value}</option>)}</Select></Field>{input('currency','สกุลเงินบัญชี','Account currency',{max:3,currency:true})}<Field appearance="standard">{t('บทบาทบัญชีคุม','Control role')}<Select appearance="standard" name="controlKind" required value={fields.controlKind??''} onChange={event=>change('controlKind',event.target.value)}><option value="" disabled>{t('เลือกบทบาทจริง','Choose the actual control role')}</option>{controls.map(value=><option key={value} value={value}>{value}</option>)}</Select></Field></>}
    {kind==='periods'&&<>{input('name','ชื่องวดบัญชี','Period name',{max:100})}{input('startDate','วันเริ่มงวด','Start date',{type:'date'})}{input('endDate','วันสิ้นงวด','End date',{type:'date'})}</>}
    {kind==='rates'&&<>{input('currency','สกุลเงินที่ไม่ใช่ USD','Currency other than USD',{max:3,currency:true})}{input('effectiveDate','วันที่มีผล','Effective date',{type:'date'})}{input('currencyPerUsd','จำนวนเงินต่อหนึ่ง USD ที่ตรวจสอบแล้ว','Reviewed currency amount per USD',{max:39})}{input('note','หมายเหตุ','Note',{max:500,required:false})}</>}
   </fieldset><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!allowed}>{busy?t('กำลังบันทึก…','Saving…'):t('บันทึกข้อมูลที่ตรวจสอบแล้ว','Save reviewed prerequisite')}</Button></CommerceForm>
   {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>{t('ลองยืนยันคำขอเดิม','Retry original confirmation')}</Button>}
   {error&&!uncertain&&!busy&&<Button appearance="standard" type="button" onClick={()=>router.refresh()}>{t('โหลดข้อมูลปัจจุบันโดยเก็บร่างไว้','Reload current references and keep draft')}</Button>}
   {dirty&&!uncertain&&!busy&&owner===currentOwner&&changed&&!!kind&&permissions[kind]&&<Button appearance="standard" type="button" onClick={()=>{setSeed(view.contextKey);setScope(draft.currentScope);}}>{t('ตรวจสอบข้อมูลปัจจุบันและเก็บร่างไว้','Review current references and keep draft')}</Button>}
  </DraftFormDrawer>
 </Card>;
}
