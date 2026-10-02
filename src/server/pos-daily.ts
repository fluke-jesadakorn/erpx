import 'server-only';
import { createHash } from 'node:crypto';
import { z } from 'zod';
import type { WorkContext } from './authorization';
import type { SqlDatabase } from './runtime/types';
import { all, one, run, id, num, str, fail, scopeRow, type PosRow } from './pos-core';
import { samePerson } from '@/lib/iam';
import { businessDate } from '@/lib/pos';
import { requirePermission } from './iam-permissions';

export type PosReceiptSnapshot = {
 schema: 1; saleId: string; number: string; currency: string; businessDate: string;
 issuer: Record<string,unknown>; customer: PosRow|null; lines: PosRow[]; payments: PosRow[];
 subtotalMinor: number; discountMinor: number; taxMinor: number; totalMinor: number;
 roomCharge: PosRow|null;
};

/** Called in the completion transaction. Reprints never consult mutable masters. */
export async function capturePosReceipt(db:SqlDatabase,saleId:string) {
 const sale=(await one(db,'SELECT * FROM pos_sales WHERE id=?',saleId))!;
 if(sale.state!=='completed')fail('POS_RECEIPT_UNPAID','Complete and receive payment before printing');
 const customer=sale.party_id?await one(db,'SELECT id,name,phone,notification_email,address_json,tax_registration FROM finance_parties WHERE id=? AND company_id=?',sale.party_id,sale.company_id):null;
 const lines=await all(db,'SELECT id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note FROM pos_lines WHERE sale_id=? ORDER BY id',saleId);
 const payments=await all(db,"SELECT id,method,amount_minor,tendered_minor,reference,finance_payment_id FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at,id",saleId);
 const roomCharge=await one(db,'SELECT property_id,record_id folio_entry_id FROM pms_sources WHERE source_key=?',`pos:${saleId}:${saleId}`);
 const snapshot:PosReceiptSnapshot={schema:1,saleId,number:str(sale,'number'),currency:str(sale,'currency'),businessDate:str(sale,'business_date'),issuer:JSON.parse(str(sale,'issuer_json')||'{}'),customer,lines,payments,subtotalMinor:num(sale,'subtotal_minor'),discountMinor:num(sale,'discount_minor'),taxMinor:num(sale,'tax_minor'),totalMinor:num(sale,'total_minor'),roomCharge};
 await run(db,'INSERT INTO pos_sale_documents(sale_id,company_id,branch_id,snapshot_json,created_at) VALUES(?,?,?,?::jsonb,?) ON CONFLICT(sale_id) DO NOTHING',saleId,sale.company_id,sale.branch_id,JSON.stringify(snapshot),Date.now());
}

export async function posCustomers(db:SqlDatabase,actor:WorkContext,query='') {
 requirePermission(actor,'pos.read');
 if(actor.departmentId)fail('POS_BRANCH_REQUIRED','Select a Company or Branch',403);
 const q=query.trim().slice(0,100),pattern=`%${q.replace(/[\\%_]/g,'\\$&')}%`;
 return all(db,`SELECT id,name,phone,notification_email,address_json,version FROM finance_parties
  WHERE company_id=? AND kind='customer' AND status='active'
  AND (?='' OR name ILIKE ? OR phone ILIKE ? OR notification_email ILIKE ?)
  ORDER BY name,id LIMIT 100`,actor.companyId,q,pattern,pattern,pattern);
}

export async function createPosCustomer(db:SqlDatabase,actor:WorkContext,input:unknown) {
 const p=z.object({name:z.string().trim().min(1).max(200),phone:z.string().trim().max(50).default(''),email:z.union([z.email(),z.literal('')]).default(''),address:z.object({line1:z.string().trim().max(200),line2:z.string().trim().max(200).default(''),city:z.string().trim().max(100),region:z.string().trim().max(100).default(''),postalCode:z.string().trim().max(30),countryCode:z.literal('TH').default('TH')}).nullable().default(null)}).parse(input);
 const duplicate=await one(db,`SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active'
  AND (lower(trim(name))=lower(?) OR (?<>'' AND regexp_replace(COALESCE(phone,''),'[^0-9]','','g')=?)
   OR (?<>'' AND lower(notification_email)=lower(?))) LIMIT 1`,actor.companyId,p.name,p.phone.replace(/\D/g,''),p.phone.replace(/\D/g,''),p.email,p.email);
 if(duplicate)fail('POS_CUSTOMER_DUPLICATE',`พบลูกค้าที่อาจซ้ำ: ${duplicate.name} กรุณาค้นหาและเลือกลูกค้าเดิม`,422);
 const customer=id(),settings=await one(db,'SELECT currency FROM pos_settings WHERE company_id=?',actor.companyId);
 if(!settings)fail('POS_SETUP_REQUIRED','Configure POS before creating customers');
 await run(db,`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,phone,notification_email,address_json,created_at,updated_at)
  VALUES(?,?,'customer',?,?,?,'active',?,?,?::jsonb,?,?)`,customer,actor.companyId,`POS-${customer.slice(0,8)}`,p.name,settings.currency,p.phone||null,p.email||null,JSON.stringify(p.address??{}),Date.now(),Date.now());
 return {id:customer,name:p.name};
}

export async function assertPosDayWritable(db:SqlDatabase,company:string,branch:string,date:string) {
 if(await one(db,'SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?',company,branch,date))
  fail('POS_DAY_CLOSED','วันธุรกิจนี้เตรียมปิดแล้ว ใช้รายการปรับปรุงในวันปัจจุบันและให้ Finance ตรวจสอบ');
}

export async function posDayReport(db:SqlDatabase,actor:WorkContext,branch:string,date:string) {
 requirePermission(actor,'pos.reports');
 if(actor.departmentId||actor.branchId&&actor.branchId!==branch)fail('POS_NOT_FOUND','Branch is outside the current workspace',404);
 if(!await one(db,"SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch'",branch,actor.companyId))fail('POS_NOT_FOUND','Branch not found',404);
 const company=actor.companyId;
 const sales=await all(db,'SELECT s.*,ps.record_id room_charge_id,rr.reversal_entry_id FROM pos_sales s LEFT JOIN pms_sources ps ON ps.source_key=\'pos:\'||s.id||\':\'||s.id LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY s.id',company,branch,date);
 const shifts=await all(db,'SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=? ORDER BY id',company,branch,date);
 const payments=await all(db,'SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY p.id',company,branch,date);
 const refunds=await all(db,'SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY r.id',company,branch,date);
 const refundPayments=await all(db,'SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY rp.id',company,branch,date);
 const stockMoves=await all(db,'SELECT * FROM pos_stock_moves WHERE company_id=? AND movement_date=? AND warehouse_id IN (SELECT id FROM commerce_warehouses WHERE company_id=? AND branch_id=?) ORDER BY id',company,date,company,branch);
 const journals=await all(db,`SELECT j.id,j.status,j.entry_date,j.source_id,j.memo,j.version FROM finance_journals j WHERE j.company_id=? AND j.entry_date=? AND j.source_type='pos'
  AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.dimension_json::jsonb->>'branchId'=?) ORDER BY j.id`,company,date,branch);
 const costs=await one(db,`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  JOIN pos_settings c ON c.company_id=j.company_id WHERE j.company_id=? AND j.entry_date=? AND j.status='posted' AND j.source_type='pos'
  AND l.dimension_json::jsonb->>'branchId'=? AND l.account_id=c.accounts_json::jsonb->>'cogs'`,company,date,branch);
 const waste=await one(db,`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  JOIN pos_settings c ON c.company_id=j.company_id WHERE j.company_id=? AND j.entry_date=? AND j.status='posted' AND j.source_type='pos'
  AND l.dimension_json::jsonb->>'branchId'=? AND l.account_id=c.accounts_json::jsonb->>'waste'`,company,date,branch);
 const settings=(await one(db,'SELECT currency FROM pos_settings WHERE company_id=?',company))!;
 const blockers:Array<{code:string;id:string;message:string}>=[];
 for(const s of sales)if(['open','held','payment_pending'].includes(str(s,'state')))blockers.push({code:'OPEN_SALE',id:str(s,'id'),message:`จัดการบิล ${s.number} ให้เสร็จ`});
 for(const s of sales)if(s.state==='completed'&&!s.room_charge_id&&!s.invoice_id)blockers.push({code:'SALE_POSTING',id:str(s,'id'),message:'บิลขายที่สำเร็จต้องมีใบแจ้งหนี้และ journal ต้นทาง'});
 for(const s of shifts)if(s.state!=='reviewed')blockers.push({code:'SHIFT_REVIEW',id:str(s,'id'),message:'ปิดกะและให้ผู้ตรวจอีกคนตรวจยอดเงินสด'});
 for(const p of payments)if(['requested','pending','exception'].includes(str(p,'state')))blockers.push({code:p.method==='transfer'?'TRANSFER_EVIDENCE':'PROVIDER_POSTING',id:str(p,'id'),message:'ยืนยันหลักฐานรับเงินและลงบัญชีให้สำเร็จ'});
 for(const p of payments)if(p.state==='succeeded'&&!p.finance_payment_id)blockers.push({code:'RECEIPT_POSTING',id:str(p,'id'),message:'รับเงินสำเร็จแต่ยังไม่มีใบรับเงิน Finance'});
 for(const r of refunds)if(['requested','approved','processing','exception'].includes(str(r,'state')))blockers.push({code:'REFUND_PENDING',id:str(r,'id'),message:'จัดการรายการคืนเงินค้าง'});
 const kitchen=await all(db,"SELECT id,state FROM pos_fulfillments WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('queued','preparing','ready') ORDER BY id",company,branch,date);
 for(const f of kitchen)blockers.push({code:'KITCHEN_PENDING',id:str(f,'id'),message:'ส่งมอบหรือยกเลิกรายการครัวพร้อมบันทึกของเสีย'});
 for(const j of journals)if(j.status!=='posted')blockers.push({code:'JOURNAL_PENDING',id:str(j,'id'),message:'Finance ต้องลงบัญชีรายการต้นทาง'});
 const pendingCash=await all(db,'SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND m.approved_by IS NULL ORDER BY m.id',company,branch,date);
 for(const m of pendingCash)blockers.push({code:'CASH_REVIEW',id:str(m,'id'),message:'อนุมัติเงินเข้า/ออกลิ้นชัก'});
 const completed=sales.filter(s=>s.state==='completed'&&!s.room_charge_id),sum=(rows:PosRow[],key:string)=>rows.reduce((n,r)=>n+num(r,key),0);
 const completedRefunds=refunds.filter(r=>r.state==='completed'&&r.kind==='sale');
 const refundMinor=sum(completedRefunds,'amount_minor'),refundTaxMinor=sum(completedRefunds,'tax_minor');
 const received:Record<string,number>={},returned:Record<string,number>={};
 for(const p of payments.filter(p=>p.state==='succeeded'))received[str(p,'method')]=(received[str(p,'method')]??0)+num(p,'amount_minor');
 for(const p of refundPayments.filter(p=>p.state==='succeeded'))returned[str(p,'method')]=(returned[str(p,'method')]??0)+num(p,'amount_minor');
 const totals={bills:completed.length,grossMinor:sum(completed,'subtotal_minor')+sum(completed,'discount_minor'),discountMinor:sum(completed,'discount_minor'),salesMinor:sum(completed,'subtotal_minor'),taxMinor:sum(completed,'tax_minor'),refundMinor,refundTaxMinor,netTaxMinor:sum(completed,'tax_minor')-refundTaxMinor,netMinor:sum(completed,'total_minor')-refundMinor,costMinor:num(costs!,'n'),wasteMinor:num(waste!,'n'),cashVarianceMinor:sum(shifts,'variance_minor'),roomChargeMinor:sum(sales.filter(s=>s.state==='completed'&&s.room_charge_id&&!s.reversal_entry_id),'total_minor'),received,returned};
 const pendingPayouts=await all(db,"SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=? AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id",company);
 const stripeFunds=await one(db,`SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  JOIN pos_settings c ON c.company_id=j.company_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=c.accounts_json::jsonb->>'stripe'`,company);
 const sources={sales,shifts,payments,refunds,refundPayments,stockMoves,journals,kitchen,pendingCash};
 // Keep the original digest contract so already-prepared immutable snapshots
 // remain confirmable when additional presentation totals are introduced.
 const digestTotals=Object.fromEntries(Object.entries(totals).filter(([key])=>!['refundTaxMinor','netTaxMinor'].includes(key)));
 const digest=createHash('sha256').update(JSON.stringify({sources,totals:digestTotals})).digest('hex');
 return {companyId:company,branchId:branch,businessDate:date,currency:str(settings,'currency'),totals,blockers,sources,pendingPayouts,stripeClearingMinor:num(stripeFunds!,'n'),digest};
}

export async function posDayCommand(db:SqlDatabase,actor:WorkContext,op:string,input:unknown) {
 if(op==='day.prepare'){
  requirePermission(actor,'pos.approve');requirePermission(actor,'pos.reports');
  const p=z.object({branchId:z.string().min(1),businessDate:z.iso.date()}).parse(input);
  if(p.businessDate>businessDate())fail('POS_FUTURE_DATE','Close only the current or an earlier business date',422);
  await assertPosDayWritable(db,actor.companyId,p.branchId,p.businessDate);
  const report=await posDayReport(db,actor,p.branchId,p.businessDate);
  if(report.blockers.length)fail('POS_DAY_BLOCKED',report.blockers.map(b=>b.message).join(' · '));
  if(!report.sources.shifts.length)fail('POS_DAY_EMPTY','No cash sessions exist for this business date',422);
  const close=id();await run(db,"INSERT INTO pos_day_closes(id,company_id,branch_id,business_date,currency,state,snapshot_json,source_digest,prepared_by,prepared_at) VALUES(?,?,?,?,?,'prepared',?::jsonb,?,?,?)",close,actor.companyId,p.branchId,p.businessDate,report.currency,JSON.stringify(report),report.digest,actor.userId,Date.now());
  return {id:close,state:'prepared'};
 }
 if(op==='day.confirm'){
  requirePermission(actor,'finance.approve');requirePermission(actor,'pos.reports');
  const p=z.object({id:z.string().min(1),expectedVersion:z.number().int().positive()}).parse(input);
  const close=scopeRow(actor,await one(db,'SELECT * FROM pos_day_closes WHERE id=? FOR UPDATE',p.id));
  if(close.version!==p.expectedVersion)fail('POS_VERSION_CONFLICT','Reload the current close');
  if(close.state!=='prepared'||samePerson(actor,close.prepared_by))fail('POS_INDEPENDENT_REVIEW','Finance อีกคนต้องยืนยันรายงานปิดวัน');
  const report=await posDayReport(db,actor,str(close,'branch_id'),str(close,'business_date'));
  if(report.blockers.length||report.digest!==close.source_digest)fail('POS_DAY_CHANGED','รายการต้นทางเปลี่ยนหลังเตรียมปิดวัน ให้ตรวจรายการปรับปรุงกับ Finance');
  await run(db,"UPDATE pos_day_closes SET state='confirmed',confirmed_by=?,confirmed_at=?,version=version+1 WHERE id=?",actor.userId,Date.now(),close.id);
  return {id:close.id,state:'confirmed'};
 }
 fail('POS_ACTION_UNKNOWN','Unknown day close command',404);
}
