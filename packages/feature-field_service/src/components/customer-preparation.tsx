'use client';
import {useCallback,useEffect,useLayoutEffect,useRef,useState} from 'react';
import {Button,Card,Field,InlineNotice,Select,TextInput} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
import {useAccountPreferences} from '@erpx/feature-identity_access/components/account-preferences-context';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {contextIsSwitching,registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import {createServiceCustomerCommands,type ServiceCustomerSource} from '@erpx/feature-field_service/lib/customer-preparation-command';
import type {ServiceCustomerInput,ServiceCustomerPreparation as ServiceCustomerPreparationView,ServiceCustomerConfirmation} from '@erpx/server-finance/public/service-customer-preparation';

const initialFields={code:'',name:'',currency:'',taxRegistration:'',paymentTermsDays:'30',creditLimit:'0'};
export function ServiceCustomerPreparation({base,workspaceKey,initial}:{base:string;workspaceKey:string;initial:ServiceCustomerPreparationView}){
 const th=useAccountPreferences()?.preference?.language==='th',t=useCallback((thai:string,english:string)=>th?thai:english,[th]),router=useRouter();
 const [owner]=useState(()=>{let committed:ServiceCustomerSource|null=null;return{read:()=>committed,commit:(source:ServiceCustomerSource)=>{committed=source;},clear:()=>{committed=null;},commands:createServiceCustomerCommands(()=>committed)};});
 useLayoutEffect(()=>{owner.commit({base,workspaceKey,companyId:initial.companyId,contextKey:initial.contextKey});return()=>owner.clear();},[base,workspaceKey,initial,owner]);
 const commands=owner.commands;
 const [open,setOpen]=useState(false),[fields,setFields]=useState({...initialFields}),[dirty,setDirty]=useState(false),[busy,setBusy]=useState(false),[pending,setPending]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState<ServiceCustomerConfirmation|null>(null),[notice,setNotice]=useState('');
 const form=useRef<HTMLFormElement>(null),turn=useRef(false),held=useRef(false),trigger=useRef<HTMLButtonElement>(null);
 useLayoutEffect(()=>{held.current=dirty||busy||pending;},[dirty,busy,pending]);
 useEffect(()=>{
  const unregister=registerUnsavedWork(()=>held.current),unload=(event:BeforeUnloadEvent)=>{if(held.current&&!contextIsSwitching()){event.preventDefault();event.returnValue='';}},allow=(href:string)=>{const from=new URL(window.location.href),to=new URL(href,from);return from.origin===to.origin&&from.pathname===to.pathname&&from.search===to.search||!held.current||window.confirm(t('ออกจากร่างลูกค้าที่ยังไม่ยืนยันหรือไม่? คำขอเดิมเก็บไว้เฉพาะหน้านี้','Leave this unconfirmed customer draft? The original request is retained only while this page remains open.'));},click=(event:MouseEvent)=>{if(event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey)return;const anchor=event.target instanceof Element?event.target.closest<HTMLAnchorElement>('a[href]'):null;if(!anchor||anchor.hasAttribute('data-product-switch')||anchor.hasAttribute('download')||anchor.target&&anchor.target!=='_self')return;const next=new URL(anchor.href,window.location.href);if(next.origin===window.location.origin&&!allow(next.href)){event.preventDefault();event.stopImmediatePropagation();}},traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string;sameDocument:boolean}};if(next.navigationType==='traverse'&&next.cancelable&&next.destination?.sameDocument&&!allow(next.destination.url))event.preventDefault();},navigation=(window as Window&{navigation?:EventTarget}).navigation;
  window.addEventListener('beforeunload',unload);document.addEventListener('click',click,true);navigation?.addEventListener('navigate',traverse);return()=>{unregister();window.removeEventListener('beforeunload',unload);document.removeEventListener('click',click,true);navigation?.removeEventListener('navigate',traverse);};
 },[t]);
 function change(name:keyof typeof fields,value:string){setFields(previous=>({...previous,[name]:value}));setDirty(true);}
 async function submit(retry=false){
  const source=owner.read();if(turn.current||!source)return;const sourceLocation=window.location.pathname+window.location.search,assertWork=captureWorkCommandContext(),current=()=>{assertWork();if(owner.read()!==source||window.location.pathname+window.location.search!==sourceLocation)throw Error('The Service customer workspace changed. Return to the original workspace to confirm.');};turn.current=true;setBusy(true);setError('');setNotice('');
  try{
   let outcome;
   if(retry)outcome=await commands.retry();
   else{if(!initial.canCreate||!source.contextKey)throw Error('A current Company Finance preparer must create the customer.');const input:ServiceCustomerInput={expectedVersion:0,contextKey:source.contextKey,input:{kind:'customer',code:fields.code,name:fields.name,currency:fields.currency,taxRegistration:fields.taxRegistration||undefined,paymentTermsDays:Number(fields.paymentTermsDays),creditLimit:fields.creditLimit}};outcome=await commands.submit(input,form.current?{form:form.current,formEditVersion:form.current.getAttribute('data-erpx-edit-version')}:undefined);}
   current();outcome.assertCurrent();const unchanged=outcome.unchanged();if(!outcome.acknowledge())throw Error('Confirm the original customer request again.');setPending(false);setSaved(outcome.confirmation);setNotice(unchanged?t('บันทึกลูกค้าแล้ว กลับไปเลือกลูกค้าในข้อตกลงบริการ','Customer saved. Choose the customer in the service agreement.'):t('บันทึกคำขอเดิมแล้ว การแก้ไขใหม่ยังเก็บไว้','The original customer was saved. Your newer draft is retained.'));
   if(unchanged){setFields({...initialFields});setDirty(false);setOpen(false);}router.refresh();
  }catch(cause){try{current();setPending(commands.pending());setError(cause instanceof Error?cause.message:t('ยังยืนยันลูกค้าไม่ได้','Customer preparation is not confirmed.'));}catch{/* The private original attempt remains available on return. */}}
  finally{
   turn.current=false;
   // A same-owner SSR refresh replaces the mutable reference image. It cannot
   // publish the old result, but the settled original attempt must be visible
   // without requiring another focus event. This does not ACK or reset fields.
   try{assertWork();const settled=owner.read();if(settled&&settled.base===source.base&&settled.companyId===source.companyId&&settled.workspaceKey===source.workspaceKey&&window.location.pathname+window.location.search===sourceLocation){setBusy(false);setPending(commands.pending());}}catch{/* No late state for a different Work, route or mounted owner. */}
  }
 }
 useEffect(()=>{const restore=()=>{if(owner.read()){setBusy(turn.current);setPending(commands.pending());}};window.addEventListener('focus',restore);window.addEventListener('erpx:work-context-changed',restore);return()=>{window.removeEventListener('focus',restore);window.removeEventListener('erpx:work-context-changed',restore);};},[commands,owner]);
 const field=(name:keyof typeof fields,label:string,options:{type?:string;required?:boolean;maxLength?:number;min?:number;max?:number}={})=><Field appearance="standard">{label}<TextInput appearance="standard" name={name} value={fields[name]} onChange={event=>change(name,event.target.value)} required={options.required!==false} {...options}/></Field>;
 return <Card as="section" appearance="standard" id="service-customer-preparation"><h2>{t('ลูกค้างานบริการ','Service customers')}</h2><p>{t('เตรียมลูกค้าก่อนร่างข้อตกลงบริการ ข้อมูลนี้ใช้สิทธิ์การเงินบริษัทเดิม','Prepare a customer before drafting a service agreement. This uses the original Company Finance permissions.')}</p>
  {!initial.available||!initial.canCreate?<InlineNotice role="status">{t('ให้ผู้มีสิทธิ์เตรียมบัญชีบริษัทสร้างลูกค้า แล้วกลับมาเลือกลูกค้าในข้อตกลงบริการ','Ask a current Company Finance preparer to create the customer, then choose it in the service agreement.')}</InlineNotice>:<Button appearance="standard" type="button" ref={trigger} onClick={()=>{setOpen(true);setError('');}}>{t('เตรียมลูกค้า','Prepare customer')}</Button>}
  {!open&&(dirty||pending)&&<Button appearance="standard" type="button" onClick={()=>setOpen(true)}>{t('เปิดร่างลูกค้าเดิม','Reopen customer draft')}</Button>}
  {notice&&<InlineNotice role="status">{notice}</InlineNotice>}{saved&&<p>{saved.code} · {saved.name} · {saved.currency}{saved.status==='blocked'&&<> · {t('ระงับการใช้งาน จึงเลือกสำหรับข้อตกลงใหม่ไม่ได้','Blocked; unavailable for a new agreement')}</>}</p>}
  <DraftFormDrawer appearance="standard" title={t('เตรียมลูกค้างานบริการ','Prepare Service customer')} open={open} dirty={dirty} retainFormContents={dirty||busy||pending} dismissDisabled={busy||pending} finalFocus={()=>trigger.current} onClose={()=>{if(!busy&&!pending)setOpen(false);}} onDiscard={()=>{if(busy||pending)return;setFields({...initialFields});setDirty(false);setOpen(false);}} confirmationTitle={t('ทิ้งร่างลูกค้าหรือไม่?','Discard customer draft?')} confirmationDescription={t('การแก้ไขยังไม่ได้บันทึก','Your changes have not been saved.')} discardLabel={t('ทิ้งร่าง','Discard draft')} keepEditingLabel={t('แก้ไขต่อ','Keep editing')}>
   {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{pending&&<InlineNotice tone="warning">{t('เก็บคำขอเดิมไว้แล้ว ยืนยันคำขอเดิมก่อนเริ่มคำขอใหม่','The original request is retained. Confirm it before starting another request.')}<Button appearance="standard" type="button" disabled={busy} onClick={()=>void submit(true)}>{t('ลองยืนยันลูกค้าเดิม','Retry original customer')}</Button></InlineNotice>}
   <form ref={form} className="ui-form-grid" onChangeCapture={event=>event.currentTarget.setAttribute('data-erpx-edit-version',crypto.randomUUID())} onSubmit={event=>{event.preventDefault();void submit();}}><fieldset disabled={busy||pending||!initial.canCreate} style={{display:'contents'}}>
    {field('code',t('รหัสลูกค้า','Customer code'),{maxLength:64})}{field('name',t('ชื่อลูกค้า','Customer name'),{maxLength:200})}
    <Field appearance="standard">{t('สกุลเงิน','Currency')}<Select appearance="standard" name="currency" value={fields.currency} required onChange={event=>change('currency',event.target.value)}><option value="">{t('เลือกสกุลเงินจริง','Choose a currency')}</option>{initial.currencies.map(currency=><option key={currency.code} value={currency.code}>{currency.code} · {currency.name}</option>)}</Select></Field>
    {field('taxRegistration',t('เลขทะเบียนภาษี','Tax registration'),{maxLength:100,required:false})}{field('paymentTermsDays',t('เงื่อนไขชำระเงิน (วัน)','Payment terms (days)'),{type:'number',min:0,max:365})}{field('creditLimit',t('วงเงินสินเชื่อ','Credit limit'))}<Button appearance="standard" type="submit" disabled={busy||pending}>{busy?t('กำลังยืนยัน…','Confirming…'):t('สร้างลูกค้า','Create customer')}</Button>
   </fieldset></form>
  </DraftFormDrawer>
 </Card>;
}
