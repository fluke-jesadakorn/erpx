"use client";
import {useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {FormDisclosure} from '@erpx/ui/components/form-disclosure';
import {Button,Field,FormLayout,InlineNotice,TextInput} from '@erpx/ui/components/foundation';
import {useAccountPreferences} from '@erpx/feature-identity_access/components/account-preferences-context';
import {createRefundBankEvidenceCommands,type RefundBankEvidenceSource,type RefundBankEvidenceFields} from '@erpx/feature-finance/lib/refund-bank-evidence-command';
import {minorToDecimal} from '@erpx/feature-finance/lib/finance-money';
import {useFinanceDraftScope} from '@erpx/feature-finance/components/finance-draft-boundary';
const identity=(source:RefundBankEvidenceSource)=>JSON.stringify([source.base,source.companyId,source.obligationId,source.instructionId]);
export function RefundBankEvidence({source,canImport}:{source:RefundBankEvidenceSource;canImport:boolean}){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,english:string)=>th?thai:english,router=useRouter(),draftScope=useFinanceDraftScope(),live=useRef<RefundBankEvidenceSource|null>(source);
 useLayoutEffect(()=>{live.current=source;return()=>{live.current=null;};},[source]);
 const command=useRef<ReturnType<typeof createRefundBankEvidenceCommands>|null>(null);command.current??=createRefundBankEvidenceCommands(()=>live.current);
 const [offered,setOffered]=useState(canImport);
 const [busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState<{owner:string;text:string}|null>(null),[savedOwner,setSavedOwner]=useState(''),[generation,setGeneration]=useState(0),busyRef=useRef(false),formRef=useRef<HTMLFormElement>(null),saved=savedOwner===identity(source);
 async function submit(form:HTMLFormElement,retry=false){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');const captured={...source};
  try{
   if(!retry&&(!canImport||saved))throw Error(t('ตรวจสอบคำสั่งคืนเงินและสิทธิ์ปัจจุบันก่อนนำเข้าหลักฐาน','Review the current original refund instruction before importing evidence.'));
   const values=new FormData(form),fields:RefundBankEvidenceFields={statementNumber:String(values.get('statementNumber')),statementDate:String(values.get('statementDate')),opening:String(values.get('opening')),closing:String(values.get('closing')),sourceHash:String(values.get('sourceHash')),externalId:String(values.get('externalId')),transactionDate:String(values.get('transactionDate')),closingFxRate:String(values.get('closingFxRate')??'')},completion={form,formEditVersion:form.getAttribute('data-erpx-edit-version')},result=await(retry?command.current!.retry():command.current!.submit(fields,completion)),unchanged=result.unchanged();
   if(!result.acknowledge())throw Error(t('กรุณายืนยันคำขอเดิมอีกครั้ง','Confirm the original bank-evidence request again.'));
   setUncertain(false);setSavedOwner(identity(captured));setNotice({owner:identity(captured),text:unchanged?t('นำเข้าหลักฐานของคำสั่งนี้แล้ว ตรวจสอบและยืนยันผลธนาคารแยกต่างหาก','Evidence for this instruction was imported. Review and confirm the bank result separately.'):t('นำเข้าคำขอเดิมแล้ว ข้อมูลที่แก้ไขภายหลังยังอยู่ กรุณาตรวจสอบหลักฐานที่นำเข้า','The original evidence was imported. Your newer entries remain; review the imported evidence.')});
   if(unchanged){try{router.refresh();}catch{setError(t('นำเข้าแล้ว แต่โหลดข้อมูลใหม่ไม่สำเร็จ กรุณาโหลดหน้านี้ใหม่โดยไม่ส่งคำขอใหม่','Evidence was imported, but refresh failed. Reload this page without creating a replacement request.'));}}
  }catch(cause){setUncertain(command.current!.pending());setError(cause instanceof Error?cause.message:t('ยังยืนยันผลไม่ได้ กรุณาลองคำขอเดิม','The result is uncertain. Retry the original evidence request.'));}
  finally{busyRef.current=false;setBusy(false);}
 }
 if(canImport&&!offered)setOffered(true);
 if(!canImport&&!offered)return null;
 return <><FormDisclosure appearance="standard" drawerTitle={t('นำเข้าหลักฐานธนาคารของคำสั่งคืนเงินนี้','Import exact refund bank evidence')} error={error} dismissDisabled={busy||uncertain} onDiscard={()=>{if(command.current!.pending())return;setGeneration(value=>value+1);setError('');if(!canImport)setOffered(false);}}><summary>{t('นำเข้าหลักฐานธนาคารของคำสั่งนี้','Import bank evidence for this instruction')}</summary>
  <p>{t('นำเข้าเพียงหนึ่งรายการที่ตรงบัญชี สกุลเงิน จำนวนเงิน และวันที่ของคำสั่งเดิม การนำเข้าไม่ใช่การยืนยันว่าคืนเงินแล้ว','Import one actual bank row matching the original instruction account, currency, amount and date. Importing evidence does not confirm the refund.')}</p><p>{source.bankAccount} · {source.currency} {minorToDecimal(source.direction==='outgoing'?-source.amountMinor:source.amountMinor)}</p>
  <FormLayout ref={formRef} key={generation} data-finance-draft={draftScope} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}><fieldset disabled={busy||!canImport||saved} style={{display:'contents'}}>
   <Field appearance="standard">{t('เลขอ้างอิงรายการเดินบัญชี','Statement reference')}<TextInput appearance="standard" name="statementNumber" required/></Field>
   <Field appearance="standard">{t('วันที่รายการเดินบัญชี','Statement date')}<TextInput appearance="standard" name="statementDate" type="date" required/></Field>
   <Field appearance="standard">{t('วันที่รายการธนาคาร','Bank transaction date')}<TextInput appearance="standard" name="transactionDate" type="date" min={source.plannedDate} required/></Field>
   <Field appearance="standard">{t('ยอดต้นรายการ','Opening balance')}<TextInput appearance="standard" name="opening" inputMode="decimal" required/></Field>
   <Field appearance="standard">{t('ยอดท้ายรายการ','Closing balance')}<TextInput appearance="standard" name="closing" inputMode="decimal" required/></Field>
   {source.functionalCurrency&&source.functionalCurrency!==source.currency&&<Field appearance="standard">{t(`อัตราปิดบัญชี ${source.functionalCurrency} ต่อ ${source.currency} (ไม่บังคับ)`,`Closing exchange rate: ${source.functionalCurrency} per ${source.currency} (optional)`)}<TextInput appearance="standard" name="closingFxRate" inputMode="decimal"/><small>{t('เว้นว่างเพื่อใช้อัตราด้วยมือที่ยืนยันแล้วของบริษัท การระบุช่องนี้ไม่ยืนยันหรือสร้างอัตราของบริษัท','Leave blank to use confirmed Company manual rates. This entry does not confirm or create a Company rate.')}</small></Field>}
   <Field appearance="standard">{t('เลขอ้างอิงธุรกรรมธนาคาร','Bank transaction reference')}<TextInput appearance="standard" name="externalId" required/></Field>
   <Field appearance="standard">{t('ค่าแฮชหลักฐานต้นฉบับ','Original evidence hash')}<TextInput appearance="standard" name="sourceHash" minLength={8} required/></Field>
  </fieldset><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!canImport||saved||uncertain}>{t('นำเข้าหลักฐานที่ตรวจสอบแล้ว','Import reviewed evidence')}</Button></FormLayout>
  {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>{t('ยืนยันคำขอเดิมอีกครั้ง','Retry original evidence confirmation')}</Button>}
 </FormDisclosure>{notice?.owner===identity(source)&&<InlineNotice role="status">{notice.text}</InlineNotice>}{error&&<InlineNotice role="alert" tone="danger">{error}</InlineNotice>}</>;
}
