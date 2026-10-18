'use client';
import {useLayoutEffect,useRef,useState} from 'react';
import {Button,Card,Field,InlineNotice,Select,TextArea,TextInput} from '@/components/ui/foundation';
import {useAccountPreferences} from '@/components/identity_access/account-preferences-context';
import {PayrollForm} from './payroll-form';
import {PayrollFormDrawer} from './payroll-form-drawer';
import {usePayrollPurchase} from './payroll-purchase-context';
import {payrollActionAllowed} from '@/lib/people_payroll/payroll-purchases';
import {minorToDecimal,decimalToMinor} from '@/lib/finance/finance-money';
import {createPayrollOtherRemittanceCommands,readPayrollOtherLiabilities,validPayrollOtherLiabilities,type PayrollOtherSource,type PayrollOtherRequest} from '@/lib/people_payroll/payroll-other-remittance-command';
import type {PayrollOtherLiabilitiesView,PayrollOtherLiability,PayrollOtherLiabilityKind} from '@/server/modules/people_payroll/public/payroll-other-remittance';

type Fields=Record<string,string>;
type Selection={mode:'remit'|'bank-evidence'|'ignore-bank-evidence';obligation:PayrollOtherLiability;view:PayrollOtherLiabilitiesView;evidenceId?:string};
/** IDs, currencies and versions come exclusively from the selected owner rows. */
export function payrollOtherRemittanceBody(selection:Selection,fields:Fields,periodEnd:string):PayrollOtherRequest{
 const {view,obligation}=selection;if(!view.contextKey||!view.schemaAvailable)throw Error('Review the current Payroll liability before submitting.');
 const common={kind:obligation.kind,liabilityAccountId:obligation.accountId,expectedRunVersion:view.runVersion,contextKey:view.contextKey,reason:fields.reason??''};
 if(selection.mode==='ignore-bank-evidence'){
  const evidence=view.bankEvidence.find(e=>e.id===selection.evidenceId&&e.kind===obligation.kind&&e.liabilityAccountId===obligation.accountId&&e.currency===view.currency&&e.canIgnore===true&&['unmatched','proposed'].includes(e.matchStatus));
  if(!evidence)throw Error('Choose current unused bank evidence for this liability account.');
  return {action:'ignore-bank-evidence',input:{...common,bankTransactionId:evidence.id,bankVersion:evidence.version}};
 }
 if(obligation.outstandingMinor<=0)throw Error('Review the current Payroll liability before submitting.');
 if(selection.mode==='remit'){
  const evidence=view.bankEvidence.find(e=>e.id===fields.evidenceId&&e.kind===obligation.kind&&e.liabilityAccountId===obligation.accountId&&e.currency===view.currency&&e.canRemit);
  if(!view.canRemit||!evidence||!Number.isSafeInteger(evidence.amountMinor)||evidence.amountMinor>=0||evidence.transactionDate<periodEnd||Math.abs(evidence.amountMinor)>obligation.outstandingMinor)throw Error('Choose current bank evidence for this liability account.');
  return {action:'remit',input:{...common,amountMinor:Math.abs(evidence.amountMinor),bankAccountId:evidence.bankAccountId,bankTransactionId:evidence.id,bankVersion:evidence.version,bankReference:evidence.reference}};
 }
 const bank=view.bankAccounts.find(a=>a.id===fields.bankAccountId&&a.currency===view.currency),amountMinor=decimalToMinor(fields.amount??'');
 if(!view.canImportBankEvidence||!bank||amountMinor<=0||amountMinor>obligation.outstandingMinor)throw Error('Choose an available bank account and an amount within the outstanding liability.');
 if(!/^\d{4}-\d{2}-\d{2}$/.test(fields.transactionDate??'')||fields.transactionDate<periodEnd||fields.transactionDate>fields.statementDate)throw Error('Choose a bank date from the Payroll period end through the statement date.');
 return {action:'bank-evidence',input:{...common,bankAccount:bank.id,statementNumber:fields.statementNumber??'',statementDate:fields.statementDate??'',opening:fields.opening??'',closing:fields.closing??'',...(fields.closingFxRate?{closingFxRate:fields.closingFxRate}:{}),sourceHash:fields.sourceHash??'',rows:[{externalId:fields.externalId??'',transactionDate:fields.transactionDate,description:fields.description??'',amount:`-${minorToDecimal(amountMinor)}`,currency:view.currency}]}};
}
const labels:Record<PayrollOtherLiabilityKind,readonly[string,string]>={benefit:['สวัสดิการ','Benefits'],deduction_other:['รายการหักอื่น','Other deductions'],employer_other:['ภาระนายจ้างอื่น','Other employer liabilities']};
const recovery:Readonly<Record<string,string>>={
 'The Payroll source or work role changed. The original request and newer draft are retained.':'ข้อมูลเงินเดือนหรือบทบาทเปลี่ยนแล้ว คำขอเดิมและร่างที่แก้ไขใหม่ยังเก็บไว้',
 'Current Payroll liabilities could not be loaded.':'ไม่สามารถโหลดภาระเงินเดือนปัจจุบันได้',
 'Wait for the original Payroll request.':'รอผลคำขอเงินเดือนเดิมก่อน',
 'Payroll remittance could not be confirmed.':'ไม่สามารถยืนยันการนำส่งเงินเดือนได้',
 'The Payroll receipt is incomplete. Retry the original request.':'ใบยืนยันเงินเดือนไม่ครบถ้วน ลองคำขอเดิมอีกครั้ง',
 'The request was saved. Confirm its current Payroll status using the original request.':'คำขอถูกบันทึกแล้ว ยืนยันสถานะเงินเดือนปัจจุบันด้วยคำขอเดิม',
 'The ignored bank evidence is not available in the current Payroll view. Retry the original request.':'ไม่พบหลักฐานธนาคารที่ละเว้นในมุมมองเงินเดือนปัจจุบัน ลองคำขอเดิมอีกครั้ง',
 'Choose current unused bank evidence for this liability account.':'เลือกหลักฐานธนาคารที่ยังไม่ได้ใช้สำหรับบัญชีภาระรายการนี้',
 'The saved bank evidence is not available in the current Payroll view. Retry the original request.':'ไม่พบหลักฐานธนาคารที่บันทึกแล้วในมุมมองเงินเดือนปัจจุบัน ลองคำขอเดิมอีกครั้ง',
 'Review the current Payroll liability before submitting.':'ตรวจสอบภาระเงินเดือนปัจจุบันก่อนส่งคำขอ',
 'Confirm the original Payroll request first. Your newer draft is retained.':'ยืนยันคำขอเงินเดือนเดิมก่อน ร่างที่แก้ไขใหม่ยังเก็บไว้',
 'There is no original Payroll request to retry.':'ไม่มีคำขอเงินเดือนเดิมให้ลองอีกครั้ง',
 'Choose current bank evidence for this liability account.':'เลือกหลักฐานธนาคารปัจจุบันสำหรับบัญชีภาระรายการนี้',
 'Choose an available bank account and an amount within the outstanding liability.':'เลือกบัญชีธนาคารที่ใช้ได้และจำนวนเงินไม่เกินภาระคงค้าง',
 'Choose a bank date from the Payroll period end through the statement date.':'เลือกวันที่ธนาคารตั้งแต่วันสิ้นงวดเงินเดือนถึงวันที่รายการเดินบัญชี',
 'Money must be a decimal string.':'ระบุจำนวนเงินเป็นเลขทศนิยม',
 'Money supports at most 2 decimal places.':'จำนวนเงินมีทศนิยมได้ไม่เกิน 2 ตำแหน่ง',
};
export function PayrollOtherRemittancesPanel({base,companyId,runId,version,currency,periodEnd,initial}:{base:string;companyId:string;runId:string;version:number;currency:string;periodEnd:string;initial?:PayrollOtherLiabilitiesView|null}){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,en:string)=>th?thai:en,purchase=usePayrollPurchase(),canSettle=payrollActionAllowed(purchase,'settle-existing');
 const expected={companyId,runId,runVersion:version,currency},seed=validPayrollOtherLiabilities(initial,expected)?initial:null;
 const [refreshed,setRefreshed]=useState<{base:string;seed:typeof initial;view:PayrollOtherLiabilitiesView}|null>(null),view=refreshed?.base===base&&refreshed.seed===initial&&validPayrollOtherLiabilities(refreshed.view,expected)?refreshed.view:seed;
 const current:PayrollOtherSource={base,...expected,contextKey:view?.contextKey??null,purchaseRevision:purchase?.revision??'legacy-disabled',canSettle,history:payrollActionAllowed(purchase,'history')},live=useRef<PayrollOtherSource|null>(current);
 const commands=useRef<ReturnType<typeof createPayrollOtherRemittanceCommands>|null>(null);
 useLayoutEffect(()=>{live.current=current;commands.current??=createPayrollOtherRemittanceCommands(()=>live.current);return()=>{live.current=null;};});
 const [selection,setSelection]=useState<Selection|null>(null),[fields,setFields]=useState<Fields>({}),[busy,setBusy]=useState(false),[pending,setPending]=useState(false),[applied,setApplied]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),form=useRef<HTMLFormElement|null>(null),trigger=useRef<HTMLButtonElement|null>(null);
 const visibleError=th&&Object.hasOwn(recovery,error)?recovery[error]:error;
 const sameSeed=!!selection&&selection.view===view,canStart=canSettle&&!!view?.schemaAvailable&&!!view.contextKey;
 function open(mode:Selection['mode'],obligation:PayrollOtherLiability,button:HTMLButtonElement,evidenceId?:string){if(!view||pending)return;trigger.current=button;setSelection({mode,obligation,view,...(evidenceId?{evidenceId}:{})});setFields(mode==='ignore-bank-evidence'?{}:{amount:minorToDecimal(obligation.outstandingMinor),transactionDate:periodEnd});setError('');setNotice('');setApplied(false);}
 function discard(){if(busy||pending)return;setSelection(null);setFields({});setError('');setApplied(false);}
 async function refresh(){if(busy||pending)return;setBusy(true);setError('');try{const next=await readPayrollOtherLiabilities(()=>live.current);setRefreshed({base,seed:initial,view:next});}catch(cause){setError(cause instanceof Error?cause.message:t('ไม่สามารถโหลดภาระเงินเดือนปัจจุบันได้','Current Payroll liabilities could not be loaded.'));}finally{setBusy(false);}}
 async function submit(target:HTMLFormElement,retry=false){
  const command=commands.current;if(busy||!command)return;setBusy(true);setError('');setNotice('');
  try{
   if(!selection)throw Error('Review the current Payroll liability before submitting.');
   if(!retry&&(!sameSeed||!canStart))throw Error('Review the current Payroll liability before submitting.');
   const result=await (retry?command.retry():command.submit(payrollOtherRemittanceBody(selection,fields,periodEnd),{form:target,formEditVersion:target.getAttribute('data-erpx-edit-version')})),unchanged=result.unchanged();
   if(!result.acknowledge())return;
   setRefreshed({base,seed:initial,view:result.view});setPending(false);setApplied(false);
   setNotice(unchanged?t('บันทึกและตรวจสอบสถานะเงินเดือนปัจจุบันแล้ว','Saved and checked against current Payroll status.'):t('บันทึกคำขอเดิมแล้ว ร่างที่แก้ไขใหม่ยังเก็บไว้ ปิดร่างแล้วตรวจสอบยอดปัจจุบันก่อนส่งรายการถัดไป','The original request was saved. Your newer draft is retained. Close it and review current balances before another submission.'));
   if(unchanged){setSelection(null);setFields({});}
  }catch(cause){setPending(command.pending());setApplied(command.applied());setError(cause instanceof Error?cause.message:t('ไม่สามารถยืนยันการนำส่งเงินเดือนได้','Payroll remittance could not be confirmed.'));}finally{setBusy(false);}
 }
 const edit=(name:string,value:string)=>setFields(old=>({...old,[name]:value}));
 return <Card as="section" appearance="standard" className="ui-panel" aria-labelledby="payroll-other-liabilities-heading">
  <h2 id="payroll-other-liabilities-heading">{t('ภาระเงินเดือนตามบัญชี','Account-specific Payroll liabilities')}</h2>
  <p className="muted">{t('นำส่งสวัสดิการ รายการหักอื่น และภาระนายจ้างอื่นจากยอดที่ผ่านการลงบัญชีแล้ว หลักฐานธนาคารผูกกับงวดและบัญชีรายการนี้เท่านั้น','Remit posted benefits, other deductions and other employer liabilities. Bank evidence is scoped to this run and liability account.')}</p>
  {visibleError&&<InlineNotice tone="danger" role="alert">{visibleError}</InlineNotice>}{notice&&<p role="status">{notice}</p>}
  {!view&&<p>{t('ยังไม่มีข้อมูลภาระเงินเดือนปัจจุบัน โหลดสถานะเพื่อตรวจสอบ','Current Payroll liability status is unavailable. Refresh to review.')}</p>}
  {view?.unavailableReason==='schema-required'&&<InlineNotice tone="info">{t('การนำส่งตามบัญชียังไม่พร้อม ติดต่อผู้ดูแลระบบเพื่อดำเนินการตั้งค่า','Account-specific remittance is not ready. Contact your administrator to finish setup.')}</InlineNotice>}
  {view?.schemaAvailable&&view.unavailableReason===null&&view.obligations.length===0&&<p>{t('ไม่พบภาระประเภทนี้ในงวดเงินเดือน','This run has no liabilities of these kinds.')}</p>}
  {view?.obligations.map(obligation=><article className="ui-record-item" key={`${obligation.kind}:${obligation.accountId}`}><div className="row-main"><strong>{t(...labels[obligation.kind])} · {obligation.accountCode} {obligation.accountName}</strong><span>{t('ตั้งค้าง','Accrued')} {currency} {minorToDecimal(obligation.accruedMinor)} · {t('นำส่งแล้ว','Remitted')} {currency} {minorToDecimal(obligation.remittedMinor)}</span><strong>{t('คงค้าง','Outstanding')} {currency} {minorToDecimal(obligation.outstandingMinor)}</strong></div>
   {canStart&&obligation.outstandingMinor>0&&<div className="ui-form-actions">{view.canImportBankEvidence&&<Button appearance="standard" variant="secondary" disabled={busy||pending} onClick={event=>open('bank-evidence',obligation,event.currentTarget)}>{t('เพิ่มหลักฐานธนาคาร','Add bank evidence')}</Button>}{view.canRemit&&view.bankEvidence.some(e=>e.kind===obligation.kind&&e.liabilityAccountId===obligation.accountId&&e.canRemit)&&<Button appearance="standard" variant="secondary" disabled={busy||pending} onClick={event=>open('remit',obligation,event.currentTarget)}>{t('บันทึกการนำส่ง','Record remittance')}</Button>}</div>}
   {view.bankEvidence.some(e=>e.kind===obligation.kind&&e.liabilityAccountId===obligation.accountId)&&<ul>{view.bankEvidence.filter(e=>e.kind===obligation.kind&&e.liabilityAccountId===obligation.accountId).map(e=><li key={e.id}>{e.transactionDate} · {e.reference} · {e.currency} {minorToDecimal(Math.abs(e.amountMinor))} · {e.matchStatus==='ignored'?t('ละเว้นแล้ว','Ignored'):e.matchStatus==='matched'?t('จับคู่แล้ว','Matched'):e.matchStatus==='proposed'?t('เสนอจับคู่','Proposed'):e.matchStatus==='unmatched'?t('ยังไม่จับคู่','Unmatched'):e.matchStatus} {canStart&&e.canIgnore===true&&<Button appearance="standard" variant="secondary" disabled={busy||pending} onClick={event=>open('ignore-bank-evidence',obligation,event.currentTarget,e.id)}>{t('ละเว้นหลักฐานที่ไม่ได้ใช้','Ignore unused evidence')}</Button>}</li>)}</ul>}
  </article>)}
  <Button appearance="standard" variant="secondary" disabled={busy||pending} onClick={()=>{void refresh();}}>{t('โหลดสถานะปัจจุบัน','Refresh current status')}</Button>
  <PayrollFormDrawer appearance="standard" open={!!selection} title={selection?.mode==='ignore-bank-evidence'?t('ละเว้นหลักฐานธนาคารที่ไม่ได้ใช้','Ignore unused bank evidence'):selection?.mode==='bank-evidence'?t('หลักฐานธนาคารสำหรับภาระเงินเดือน','Payroll liability bank evidence'):t('บันทึกการนำส่งเงินเดือน','Record Payroll remittance')} onClose={discard} onDiscard={discard} dismissDisabled={busy||pending} retainFormContents finalFocus={()=>trigger.current} confirmationTitle={t('ทิ้งการแก้ไขหรือไม่','Discard changes?')} confirmationDescription={t('ร่างนี้ยังไม่ได้บันทึก','Your draft has unsaved changes.')} discardLabel={t('ทิ้งร่าง','Discard draft')} keepEditingLabel={t('แก้ไขต่อ','Keep editing')}>
   {selection&&<PayrollForm ref={form} className="ui-form-grid" onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}>
    <p>{t(...labels[selection.obligation.kind])} · {selection.obligation.accountCode} {selection.obligation.accountName} · {currency}</p>
    {visibleError&&<InlineNotice tone="danger" role="alert">{visibleError}</InlineNotice>}{notice&&<p role="status">{notice}</p>}
    {!sameSeed&&<InlineNotice tone="info">{t('ยอดอ้างอิงเปลี่ยนแล้ว ร่างยังเก็บไว้ ปิดร่างและตรวจสอบยอดปัจจุบันก่อนเริ่มคำขอใหม่','The source balances changed. Your draft is retained. Close it and review current balances before starting a new request.')}</InlineNotice>}
    {selection.mode==='ignore-bank-evidence'?<><InlineNotice tone="info">{t('การละเว้นเก็บหลักฐานไว้ในประวัติ ไม่ลดภาระคงค้างและไม่บันทึกการนำส่ง','Ignoring keeps the evidence in history. It does not reduce the liability or record a remittance.')}</InlineNotice>{selection.view.bankEvidence.filter(e=>e.id===selection.evidenceId).map(e=><p key={e.id}>{e.transactionDate} · {e.reference} · {e.currency} {minorToDecimal(Math.abs(e.amountMinor))}</p>)}</>:selection.mode==='remit'?<Field appearance="standard">{t('หลักฐานธนาคาร','Bank evidence')}<Select appearance="standard" required name="evidenceId" value={fields.evidenceId??''} onChange={event=>edit('evidenceId',event.target.value)}><option value="">{t('เลือกหลักฐานสำหรับบัญชีนี้','Choose evidence for this account')}</option>{selection.view.bankEvidence.filter(e=>e.kind===selection.obligation.kind&&e.liabilityAccountId===selection.obligation.accountId&&e.canRemit).map(e=><option key={e.id} value={e.id}>{e.transactionDate} · {e.reference} · {e.currency} {minorToDecimal(Math.abs(e.amountMinor))} · {selection.view.bankAccounts.find(a=>a.id===e.bankAccountId)?.name??t('บัญชีธนาคารเดิม','Original bank account')}</option>)}</Select></Field>:<>
     <Field appearance="standard">{t('บัญชีธนาคาร','Bank account')}<Select appearance="standard" required name="bankAccountId" value={fields.bankAccountId??''} onChange={event=>edit('bankAccountId',event.target.value)}><option value="">{t('เลือกบัญชีธนาคาร','Choose bank account')}</option>{selection.view.bankAccounts.map(a=><option key={a.id} value={a.id}>{a.code} {a.name} · {a.currency}</option>)}</Select></Field>
     <Field appearance="standard">{t('จำนวนเงิน','Amount')} ({currency})<TextInput appearance="standard" name="amount" type="number" min="0.01" step="0.01" max={minorToDecimal(selection.obligation.outstandingMinor)} value={fields.amount??''} onChange={event=>edit('amount',event.target.value)} required/></Field>
     <Field appearance="standard">{t('เลขที่รายการเดินบัญชี','Statement number')}<TextInput appearance="standard" name="statementNumber" value={fields.statementNumber??''} onChange={event=>edit('statementNumber',event.target.value)} required/></Field>
     <Field appearance="standard">{t('วันที่รายการเดินบัญชี','Statement date')}<TextInput appearance="standard" name="statementDate" type="date" min={periodEnd} value={fields.statementDate??''} onChange={event=>edit('statementDate',event.target.value)} required/></Field>
     <Field appearance="standard">{t('ยอดยกมา','Opening balance')} ({currency})<TextInput appearance="standard" name="opening" inputMode="decimal" value={fields.opening??''} onChange={event=>edit('opening',event.target.value)} required/></Field>
     <Field appearance="standard">{t('ยอดปิด','Closing balance')} ({currency})<TextInput appearance="standard" name="closing" inputMode="decimal" value={fields.closing??''} onChange={event=>edit('closing',event.target.value)} required/></Field>
     <Field appearance="standard">{t('อัตราแลกเปลี่ยนยอดปิด (ถ้าจำเป็น)','Closing FX rate (if required)')}<TextInput appearance="standard" name="closingFxRate" inputMode="decimal" value={fields.closingFxRate??''} onChange={event=>edit('closingFxRate',event.target.value)}/></Field>
     <Field appearance="standard">{t('รหัสตรวจสอบเอกสารต้นทาง','Source fingerprint')}<TextInput appearance="standard" name="sourceHash" minLength={8} value={fields.sourceHash??''} onChange={event=>edit('sourceHash',event.target.value)} required/></Field>
     <Field appearance="standard">{t('เลขอ้างอิงธนาคาร','Bank reference')}<TextInput appearance="standard" name="externalId" maxLength={160} value={fields.externalId??''} onChange={event=>edit('externalId',event.target.value)} required/></Field>
     <Field appearance="standard">{t('วันที่ธนาคาร','Bank date')}<TextInput appearance="standard" name="transactionDate" type="date" min={periodEnd} max={fields.statementDate||undefined} value={fields.transactionDate??''} onChange={event=>edit('transactionDate',event.target.value)} required/></Field>
     <Field appearance="standard">{t('รายละเอียดธนาคาร','Bank description')}<TextInput appearance="standard" name="description" value={fields.description??''} onChange={event=>edit('description',event.target.value)}/></Field>
    </>}
    <Field appearance="standard">{t('เหตุผล','Reason')}<TextArea appearance="standard" name="reason" minLength={3} maxLength={3000} value={fields.reason??''} onChange={event=>edit('reason',event.target.value)} required/></Field>
    {pending&&<InlineNotice tone="info">{applied?t('ได้รับใบยืนยันแล้ว แต่ยังต้องตรวจสอบสถานะปัจจุบันด้วยคำขอเดิม','A receipt was received. Confirm its current status using the original request.'):t('ยังยืนยันผลคำขอไม่ได้ ลองคำขอเดิมด้วยข้อมูลและกุญแจเดิม','The result is uncertain. Retry the original body and key.')}</InlineNotice>}
    <div className="ui-form-actions"><Button appearance="standard" clientManaged type="submit" disabled={busy||pending||!sameSeed||!canStart}>{selection.mode==='ignore-bank-evidence'?t('ละเว้นหลักฐานนี้','Ignore this evidence'):selection.mode==='bank-evidence'?t('บันทึกหลักฐานธนาคาร','Save bank evidence'):t('บันทึกการนำส่ง','Record remittance')}</Button>{pending&&<Button appearance="standard" variant="secondary" disabled={busy} onClick={()=>{if(form.current)void submit(form.current,true);}}>{t('ลองคำขอเดิมอีกครั้ง','Retry original request')}</Button>}</div>
   </PayrollForm>}
  </PayrollFormDrawer>
 </Card>;
}
