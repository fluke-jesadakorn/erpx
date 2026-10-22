"use client";
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import Link from '@erpx/workplace/components/workspace-link';
import {Button,Card,Field,InlineNotice,TextInput} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
import {CommerceDraftBoundary,CommerceForm} from '@erpx/feature-commerce/components/commerce-draft-boundary';
import {useAccountPreferences} from '@erpx/feature-identity_access/components/account-preferences-context';
import {createSalesServiceProductCommands,emptySalesServiceProductDraft,salesServiceProductInput,type SalesServiceProductSource,type SalesServiceProductDraft} from '@erpx/feature-sales/lib/service-product-command';
import type {SalesServiceProductsView} from '@erpx/server-commerce/public/sales-service-products';
const identity=(source:SalesServiceProductSource)=>JSON.stringify([source.base,source.companyId]);
/** Server seed is authoritative. Opening or mounting never loads a collection. */
export function SalesServiceProducts({base,initial}:{base:string;initial:SalesServiceProductsView}){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,english:string)=>th?thai:english,router=useRouter();
 const source={base,companyId:initial.companyId,contextKey:initial.contextKey},live=useRef<SalesServiceProductSource|null>(source),commands=useRef<ReturnType<typeof createSalesServiceProductCommands>|null>(null),formRef=useRef<HTMLFormElement>(null),trigger=useRef<HTMLButtonElement>(null),revision=useRef(0),busyRef=useRef(false);
 useLayoutEffect(()=>{live.current=source;if(commands.current===null)commands.current=createSalesServiceProductCommands(()=>live.current);return()=>{live.current=null;};});
 const [draft,setDraft]=useState(emptySalesServiceProductDraft),[seed,setSeed]=useState<SalesServiceProductSource|null>(null),[open,setOpen]=useState(false),[busy,setBusy]=useState(false),[dirty,setDirty]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[saved,setSaved]=useState<{owner:string;id:string}|null>(null);
 const sameOwner=!seed||identity(seed)===identity(source),current=!!seed&&sameOwner&&seed.contextKey===source.contextKey,allowed=initial.canCreate&&current&&!uncertain;
 const change=(name:keyof SalesServiceProductDraft,value:string)=>{revision.current++;setDirty(true);setDraft(previous=>({...previous,[name]:value}));};
 function discard(){if(commands.current!.pending())return;setDraft(emptySalesServiceProductDraft());setDirty(false);setOpen(false);setSeed(null);setError('');if(formRef.current)window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:formRef.current}}));}
 async function submit(form:HTMLFormElement,retry=false){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');const submittedRevision=revision.current;
  try{
   if(!retry&&!allowed)throw Error(t('ตรวจสอบบริบทการขายปัจจุบันก่อนบันทึก','Review the current Sales context before saving.'));
   const result=await(retry?commands.current!.retry():commands.current!.submit(salesServiceProductInput(draft),{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}));
   if(!result.acknowledge())return;setSaved({owner:identity(source),id:result.result.id});setUncertain(false);
   if(submittedRevision===revision.current&&result.unchanged()){setDraft(emptySalesServiceProductDraft());setDirty(false);setOpen(false);setSeed(null);}router.refresh();
  }catch(failure){setError(failure instanceof Error?failure.message:String(failure));setUncertain(commands.current!.pending());}finally{busyRef.current=false;setBusy(false);}
 }
 const savedNotice=saved?.owner===identity(source)&&<InlineNotice role="status">{t('บันทึกรายการบริการแล้ว','Service item saved')}: {saved.id}. <Link href={`${base}/customers-sales/sales/settings/services#${encodeURIComponent(saved.id)}`}>{t('ดูรายการที่บันทึก','View saved service item')}</Link></InlineNotice>;
 return <CommerceDraftBoundary><Card appearance="standard"><h1>{t('รายการบริการสำหรับการขาย','Sales service catalogue')}</h1><p><Link href={`${base}/customers-sales/sales/documents`}>{t('กลับไปใบเสนอราคาและใบสั่งขาย','Return to quotations and sales orders')}</Link></p>
  {!initial.available&&<InlineNotice role="status">{initial.unavailableReason==='purchase-required'?t('ต้องมีแพ็กเกจการขายที่มีสิทธิ์ดูประวัติ','Current Sales purchase history access is required.'):t('ใช้บทบาทการขายที่มีสิทธิ์ปัจจุบัน','Use a current authorized Sales work role.')}</InlineNotice>}
  {initial.unavailableReason==='sales-posting-required'&&<InlineNotice role="status">{t('เตรียมและตรวจสอบการลงบัญชีการขายก่อนเพิ่มบริการ','Prepare and independently review Sales posting before adding services.')} <Link href={`${base}/customers-sales/sales/settings/posting`}>{t('เตรียมการลงบัญชีการขาย','Prepare Sales posting')}</Link></InlineNotice>}
  {initial.available&&<><p>{t('สกุลเงินเอกสาร','Document currency')}: {initial.currency??t('ยังไม่พร้อม','Not ready')}</p>{!initial.products.length&&<p>{t('ยังไม่มีรายการบริการ','No service items yet.')}</p>}<ul>{initial.products.map(product=><li id={product.id} key={product.id}>{product.sku} · {product.name} · {product.unit} · {t(product.status==='active'?'ใช้งาน':'เก็บประวัติ',product.status==='active'?'Active':'Archived')}</li>)}</ul></>}
  {initial.available&&!initial.canCreate&&initial.unavailableReason===null&&<p>{t('ผู้จัดเตรียมสินค้าที่มีสิทธิ์ปัจจุบันสามารถเพิ่มบริการได้ ติดต่อผู้ดูแลบริษัทหากต้องการรายการใหม่','An authorized product preparer can add services. Contact your Company administrator for a new service item.')}</p>}
  {savedNotice}{error&&!open&&<InlineNotice role="alert" tone="danger">{error}</InlineNotice>}
  {(initial.canCreate||dirty)&&<Button ref={trigger} appearance="standard" type="button" disabled={busy} onClick={()=>{if(!dirty){setSeed(source);setDraft(emptySalesServiceProductDraft());}setOpen(true);}}>{dirty?t('เปิดร่างที่เก็บไว้','Reopen retained draft'):t('เพิ่มบริการ','Add service item')}</Button>}
  <DraftFormDrawer appearance="standard" open={open} title={t('เพิ่มบริการ','Add service item')} dirty={dirty||uncertain} retainFormContents={dirty||uncertain} dismissDisabled={busy||uncertain} finalFocus={()=>trigger.current} onClose={()=>setOpen(false)} onDiscard={discard} confirmationTitle={t('ยกเลิกร่างหรือไม่','Discard changes?')} confirmationDescription={t('ร่างนี้ยังไม่ได้บันทึก','Your draft has unsaved changes.')} discardLabel={t('ทิ้งร่าง','Discard draft')} keepEditingLabel={t('แก้ไขต่อ','Keep editing')}>
   {savedNotice}{error&&<InlineNotice role="alert" tone="danger">{error}</InlineNotice>}{!current&&dirty&&<InlineNotice role="status">{t('ร่างนี้เก็บบริบทเดิมไว้ กลับไปบริบทเดิมหรือโหลดข้อมูลปัจจุบันโดยเก็บร่างไว้','This draft retains its original context. Return to it or reload current references and keep the draft.')}</InlineNotice>}
   <CommerceForm ref={formRef} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}><fieldset disabled={busy||!allowed} style={{display:'contents'}}>
    {([['sku','รหัสบริการ','Service code',40],['name','ชื่อบริการ','Service name',160],['unit','หน่วย','Unit',30],['price','ราคาต่อหน่วย','Unit price',undefined],['taxPercent','ภาษี (%)','Tax (%)',undefined]] as const).map(([name,thai,english,max])=><Field appearance="standard" key={name}>{t(thai,english)}<TextInput appearance="standard" name={name} required value={draft[name]} onChange={event=>change(name,event.target.value)} maxLength={max} minLength={name==='sku'||name==='name'?2:1} inputMode={name==='price'||name==='taxPercent'?'decimal':undefined} pattern={name==='price'||name==='taxPercent'?'[0-9]+(\\.[0-9]{1,2})?':undefined}/></Field>)}
   </fieldset><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!allowed}>{busy?t('กำลังบันทึก…','Saving…'):t('บันทึกบริการ','Save service item')}</Button></CommerceForm>
   {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>{t('ลองยืนยันคำขอเดิม','Retry original request')}</Button>}
   {!uncertain&&!busy&&dirty&&sameOwner&&!current&&initial.canCreate&&<Button appearance="standard" type="button" onClick={()=>setSeed(source)}>{t('ตรวจสอบข้อมูลปัจจุบันและเก็บร่างไว้','Review current references and keep draft')}</Button>}
   {!busy&&error&&<Button appearance="standard" type="button" onClick={()=>router.refresh()}>{t('โหลดสถานะโดยเก็บร่างไว้','Reload status and keep draft')}</Button>}
  </DraftFormDrawer>
 </Card></CommerceDraftBoundary>;
}
