import 'server-only';
import { createHash, randomUUID } from 'node:crypto';
import { z } from 'zod';
import { type PosAccounts, settingsSchema, catalogSchema, money, qty, saleAmounts, recipeQuantities, portion, businessDate, accountKeys } from '@/lib/pos';
import { currencyDefinition } from '@/lib/company-money';
import { scaledAmount } from '@/lib/commerce-math';
import { samePerson } from '@/lib/iam';
import { hasPermission } from '@/lib/permissions';
import type { WorkContext } from './authorization';
import type { SqlDatabase } from './runtime/types';
import { PostgresDatabase } from './runtime/postgres';
import { requirePosImageSource } from './pos-image-source';
import { requirePermission } from './iam-permissions';
import { workContextGuards } from './commands';
import { iamMutationGuards } from './iam-store';
import { ApiError } from './http';
import { applyPosRoomCharge, reversePosRoomCharge } from './pms';
import { assertProductionModuleEnabled } from './hospitality-rollout';
import { posCommerceAccountDefaults, posGeneratedAccountSpecs } from '@/lib/pos-bootstrap';
import { prepareAttachmentBinding } from './attachments/service';
import { snapshotMoney, snapshotMoneyCarry, snapshotMoneyReversal } from './company-money';
import { capturePosReceipt,createPosCustomer,posDayCommand,assertPosDayWritable } from './pos-daily';
export type PosRow=Record<string,string|number|boolean|null>;
export const id=()=>randomUUID();
export const num=(row:PosRow,key:string)=>Number(row[key]??0);
export const str=(row:PosRow,key:string)=>String(row[key]??'');
export function json<T>(value:unknown):T {return JSON.parse(String(value)) as T;}
export function fail(code:string,message:string,status=409):never {throw new ApiError(status,code,message);}
export async function one(db:SqlDatabase,sql:string,...args:unknown[]){return db.prepare(sql).bind(...args).first<PosRow>();}
export async function all(db:SqlDatabase,sql:string,...args:unknown[]){return (await db.prepare(sql).bind(...args).all<PosRow>()).results;}
export async function run(db:SqlDatabase,sql:string,...args:unknown[]){return db.prepare(sql).bind(...args).run();}
export function scopeRow(actor:WorkContext,row:PosRow|null):PosRow {
 if(!row||row.company_id!==actor.companyId||(actor.branchId&&row.branch_id!==actor.branchId)||actor.departmentId)fail('POS_NOT_FOUND','Record not found in this workspace',404);
 return row;
}
export async function saleRow(db:SqlDatabase,actor:WorkContext,saleId:string){return scopeRow(actor,await one(db,'SELECT * FROM pos_sales WHERE id=? FOR UPDATE',saleId));}
function version(row:PosRow,input:{expectedVersion?:number}){if(num(row,'version')!==input.expectedVersion)fail('POS_VERSION_CONFLICT','Record changed. Load the latest data');}
export async function config(db:SqlDatabase,companyId:string){const row=await one(db,'SELECT * FROM pos_settings WHERE company_id=?',companyId);if(!row)fail('POS_SETUP_REQUIRED','Configure accounts and sales points before use');return row;}
export async function openPeriod(db:SqlDatabase,companyId:string,date:string){const periods=await all(db,"SELECT id FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=? AND state='open' FOR SHARE",companyId,date,date);if(periods.length!==1)fail('POS_PERIOD_CLOSED','An open accounting period is required for this date');return str(periods[0],'id');}
export async function posting(db:SqlDatabase,companyId:string,branchId:string|null,accounts:PosAccounts,date:string,currency:string,sourceKey:string,sourceId:string,actorId:string,entries:Array<{account:string;debit:number;credit:number}>){
 const existing=await one(db,'SELECT journal_id FROM pos_postings WHERE source_key=?',sourceKey);if(existing)return str(existing,'journal_id');
 const lines=entries.filter(e=>e.debit||e.credit);if(!lines.length)return null;
 if(lines.some(e=>!Number.isSafeInteger(e.debit)||!Number.isSafeInteger(e.credit)||e.debit<0||e.credit<0||Boolean(e.debit)===Boolean(e.credit))||lines.reduce((n,e)=>n+e.debit-e.credit,0)!==0)fail('POS_UNBALANCED','Journal is unbalanced',422);
 const period=await openPeriod(db,companyId,date),journal=id(),now=Date.now();
 const chart=await one(db,"SELECT id FROM finance_charts WHERE id=? AND company_id=?",accounts.chartId,companyId);if(!chart)fail('POS_CHART_INVALID','Company chart of accounts not found');
 for(const line of lines)if(!await one(db,"SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active'",line.account,companyId,accounts.chartId))fail('POS_ACCOUNT_INVALID','Configured account is not available');
 await run(db,"INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'pos',?,?,'posted',1,?,?,?,?,?)",journal,companyId,accounts.chartId,period,`POS-GL-${journal}`,date,currency,sourceId,sourceKey,actorId,actorId,now,now,now);
 for(const [i,line] of lines.entries())await run(db,'INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?)',id(),journal,line.account,line.debit,line.credit,line.debit,line.credit,JSON.stringify({branchId,sourceId}),i);
 await run(db,'INSERT INTO pos_postings VALUES(?,?,?)',sourceKey,companyId,journal);return journal;
}
export const pair=(debit:string,credit:string,amount:number)=>[{account:debit,debit:amount,credit:0},{account:credit,debit:0,credit:amount}];
export async function audit(db:SqlDatabase,companyId:string,branchId:string|null,actorId:string,operation:string,recordId:string,details:unknown){await run(db,'INSERT INTO pos_audit VALUES(?,?,?,?,?,?,?,?)',id(),companyId,branchId,actorId,operation,recordId,JSON.stringify(details),Date.now());}
export async function enqueue(db:SqlDatabase,companyId:string,kind:string,recordId:string,payload:unknown={}){await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,payload_json,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT(kind,record_id) DO NOTHING",id(),companyId,kind,recordId,JSON.stringify(payload),Date.now());}
async function editable(db:SqlDatabase,sale:PosRow){if(!['open','held'].includes(str(sale,'state')))fail('POS_SALE_LOCKED','This bill cannot be edited');if(await one(db,"SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor) LIMIT 1",sale.id))fail('POS_PAYMENT_STARTED','This bill already has a payment');}
async function refreshSale(db:SqlDatabase,saleId:string,clearApproval=true){await run(db,`UPDATE pos_sales SET subtotal_minor=(SELECT COALESCE(SUM(subtotal_minor),0) FROM pos_lines WHERE sale_id=?), tax_minor=(SELECT COALESCE(SUM(tax_minor),0) FROM pos_lines WHERE sale_id=?),discount_minor=(SELECT COALESCE(SUM(discount_minor),0) FROM pos_lines WHERE sale_id=?),total_minor=(SELECT COALESCE(SUM(subtotal_minor+tax_minor),0) FROM pos_lines WHERE sale_id=?),version=version+1,updated_at=?${clearApproval?',discount_approved_by=NULL,discount_reason=NULL':''} WHERE id=?`,saleId,saleId,saleId,saleId,Date.now(),saleId);}
async function ingredients(db:SqlDatabase,f:PosRow){return recipeQuantities(json<Array<{productId:string;quantityMilli:number}>>(f.recipe_json),num(f,'quantity_milli'));}
export async function reserve(db:SqlDatabase,f:PosRow){
 if(f.reserved||['preparing','ready','delivered'].includes(str(f,'state')))return;
 for(const part of await ingredients(db,f)){
  const stock=await one(db,'SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE',f.warehouse_id,part.productId);
  const held=await one(db,'SELECT COALESCE(SUM(quantity_milli),0) n FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?',f.warehouse_id,part.productId);
  if(!stock||num(stock,'quantity_milli')-num(held!,'n')<part.quantityMilli)fail('POS_STOCK_UNAVAILABLE','Insufficient product/ingredients for this item');
  await run(db,'INSERT INTO pos_reservations VALUES(?,?,?,?)',f.id,f.warehouse_id,part.productId,part.quantityMilli);
 }
 await run(db,'UPDATE pos_fulfillments SET reserved=true WHERE id=?',f.id);
}
async function consume(db:SqlDatabase,f:PosRow,actorId:string,kitchen:boolean){
 await reserve(db,f);let cost=0;
 for(const part of await ingredients(db,f)){
  const stock=await one(db,'SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE',f.warehouse_id,part.productId);
  if(!stock||num(stock,'quantity_milli')<part.quantityMilli)fail('POS_STOCK_UNAVAILABLE','Stock changed. Review it');
  if(await one(db,'SELECT id FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1',f.warehouse_id,part.productId,f.business_date))fail('POS_BACKDATED_STOCK','Stock movement exists after this transaction date');
  const value=part.quantityMilli===num(stock,'quantity_milli')?num(stock,'value_minor'):scaledAmount(num(stock,'value_minor'),part.quantityMilli,num(stock,'quantity_milli'));cost+=value;
  await run(db,'DELETE FROM pos_reservations WHERE fulfillment_id=? AND product_id=?',f.id,part.productId);
  await run(db,'UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?',part.quantityMilli,value,f.warehouse_id,part.productId);
  await run(db,'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',id(),f.company_id,f.warehouse_id,part.productId,f.id,f.id,`POS-${f.id}`,kitchen?'kitchen':'sale',-part.quantityMilli,-value,f.business_date,Date.now());
 }
 const a=json<PosAccounts>(f.accounts_json);
 await posting(db,str(f,'company_id'),str(f,'branch_id'),a,str(f,'business_date'),str(f,'currency'),`consume:${f.id}`,str(f,'id'),actorId,pair(kitchen?a.wip:a.cogs,a.inventory,cost));
 await run(db,'UPDATE pos_fulfillments SET reserved=false,cost_minor=? WHERE id=?',cost,f.id);return cost;
}
export async function receivePayment(db:SqlDatabase,payment:PosRow,actorId:string,date?:string){
 if(payment.state==='succeeded')return;
 const sale=(await one(db,'SELECT * FROM pos_sales WHERE id=? FOR UPDATE',payment.sale_id))!;
 if(['cancelled','merged'].includes(str(sale,'state')))fail('POS_PAYMENT_EXCEPTION','Payment for a cancelled bill requires review');
 const a=json<PosAccounts>(sale.accounts_json),when=date??str(sale,'business_date');
 const account=payment.method==='cash'?a.cash:payment.method==='transfer'?a.bank:payment.method==='edc'?a.edc:a.stripe;
 const financeId=id();const journal=await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,when,str(sale,'currency'),`receipt:${payment.id}`,str(sale,'id'),actorId,pair(account,sale.invoice_id?a.receivable:a.advance,num(payment,'amount_minor')));
 await run(db,"INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,?,?,0,?,'settled',?,?,?,?,?)",financeId,sale.company_id,sale.party_id,`POS-R-${payment.id}`,when,sale.currency,payment.amount_minor,account,journal,actorId,actorId,Date.now(),Date.now());
 const received=await snapshotMoneyCarry(db,{companyId:str(sale,'company_id')},{sourceType:'pos_payment',sourceId:str(payment,'id'),amountKind:'received',transactionDate:when,amountUnits:String(payment.amount_minor),original:{sourceType:'pos_payment',sourceId:str(payment,'id'),amountKind:'requested'}});
 await received.statement.run();
 await run(db,"UPDATE pos_payments SET state='succeeded',finance_payment_id=?,updated_at=?,version=version+1 WHERE id=?",financeId,Date.now(),payment.id);
 await run(db,"UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?",Date.now(),sale.id);
}
export async function completeSale(db:SqlDatabase,sale:PosRow,actorId:string,date?:string){
 if(sale.state==='completed')return {id:sale.id,state:'completed',invoiceId:sale.invoice_id};
 if(!['open','held','payment_pending'].includes(str(sale,'state')))fail('POS_SALE_LOCKED','This bill cannot be closed');
 const lines=await all(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale.id);if(!lines.length)fail('POS_EMPTY_SALE','Add items before closing the bill',422);
 const paid=await one(db,"SELECT COALESCE(SUM(amount_minor-refunded_minor),0) amount FROM pos_payments WHERE sale_id=? AND state='succeeded'",sale.id);
 if(num(paid!,'amount')!==num(sale,'total_minor'))fail('POS_NOT_PAID','Payment is incomplete');
 if(await one(db,"SELECT id FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception')",sale.id))fail('POS_PAYMENT_PENDING','Wait for the payment result before closing the bill');
 const a=json<PosAccounts>(sale.accounts_json),when=date??str(sale,'business_date');
 await openPeriod(db,str(sale,'company_id'),when);
 for(const itemId of new Set(lines.map(l=>str(l,'fulfillment_id')))){
  const f=(await one(db,'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',itemId))!;
  if(f.state==='cancelled')fail('POS_ITEM_CANCELLED','Remove cancelled items before closing the bill');
  if(f.kind==='recipe'&&f.state==='draft')fail('POS_SEND_KITCHEN','Send items to the kitchen before closing the bill');
  if(f.kind!=='recipe'&&f.state!=='delivered'){await consume(db,{...f,business_date:when},actorId,false);await run(db,"UPDATE pos_fulfillments SET state='delivered',version=version+1 WHERE id=?",f.id);}
 }
 let party=sale.party_id;
 if(!party){party=`pos-walkin-${sale.company_id}`;await run(db,"INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer','POS-WALKIN','POS walk-in customer',?,'active',?,?) ON CONFLICT(id) DO NOTHING",party,sale.company_id,sale.currency,Date.now(),Date.now());}
 const invoice=id();
 const journal=await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,when,str(sale,'currency'),`sale:${sale.id}`,str(sale,'id'),actorId,[{account:a.receivable,debit:num(sale,'total_minor'),credit:0},{account:a.revenue,debit:0,credit:num(sale,'subtotal_minor')},{account:a.outputTax,debit:0,credit:num(sale,'tax_minor')}]);
 await run(db,"INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,0,?,?,?,'pos',?,'paid',?,?,?,?,?,?)",invoice,sale.company_id,party,sale.number,when,when,sale.currency,sale.subtotal_minor,sale.tax_minor,sale.total_minor,a.receivable,a.revenue,a.outputTax,sale.id,journal,actorId,actorId,Date.now(),Date.now(),Date.now());
 await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,when,str(sale,'currency'),`allocate:${sale.id}`,str(sale,'id'),actorId,pair(a.advance,a.receivable,num(sale,'total_minor')));
 for(const payment of await all(db,"SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' AND amount_minor>refunded_minor",sale.id)){
  await run(db,'UPDATE finance_payments SET party_id=? WHERE id=?',party,payment.finance_payment_id);
  await run(db,'INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)',id(),sale.company_id,payment.finance_payment_id,invoice,num(payment,'amount_minor')-num(payment,'refunded_minor'),actorId,Date.now());
 }
 await capturePosMoney(db,sale,'pos_sale',str(sale,'id'),'total',str(sale,'business_date'),String(sale.total_minor));
 await run(db,"UPDATE pos_sales SET state='completed',invoice_id=?,party_id=?,table_id=NULL,version=version+1,updated_at=? WHERE id=?",invoice,party,Date.now(),sale.id);
 await capturePosReceipt(db,str(sale,'id'));
 return {id:sale.id,state:'completed',invoiceId:invoice};
}
async function capturePosMoney(db:SqlDatabase,sale:PosRow,sourceType:string,sourceId:string,amountKind:string,transactionDate:string,amountUnits:string){
  const evidence=await snapshotMoney(db,{companyId:str(sale,'company_id')},{sourceType,sourceId,amountKind,transactionDate,currency:str(sale,'currency'),currencyScale:2,amountUnits});
  await evidence.statement.run();
}
async function capturePosReversal(db:SqlDatabase,sale:PosRow,sourceType:string,sourceId:string,amountKind:string,transactionDate:string,amountUnits:string,original:{sourceType:string;sourceId:string;amountKind:string}){
 const evidence=await snapshotMoneyReversal(db,{companyId:str(sale,'company_id')},{sourceType,sourceId,amountKind,transactionDate,amountUnits,original});
 await evidence.statement.run();
}
const keySchema=z.string().regex(/^[a-zA-Z0-9:_.-]{8,180}$/);
const base=z.object({id:z.string().min(1),expectedVersion:z.number().int().positive()});
const permissionFor=(op:string)=>op==='day.confirm'||op==='payment.verify_transfer'||op==='sale.reverse_room_charge'||op==='settlement.refresh'?'finance.approve':op==='day.prepare'?'pos.approve':op.startsWith('settings')||op.startsWith('register')||op.startsWith('catalog')||op.startsWith('table.')?'pos.configure':op.startsWith('kitchen.')?'pos.kitchen':op==='refund.approve'||op==='refund.reject'||op==='sale.approve_discount'||op==='shift.review'||op==='cash.approve'||op.startsWith('exception.')||op.startsWith('settlement.')?'pos.approve':op.startsWith('connection.')?'pos.payments.configure':'pos.sell';
export async function posCommand(db:PostgresDatabase,actor:WorkContext,operation:string,input:unknown,key:string){
 keySchema.parse(key);if(operation.startsWith('connection.')){if(!hasPermission(actor,'pos.payments.configure')&&!hasPermission(actor,'finance.approve'))fail('POS_CONNECTION_FORBIDDEN','Company Finance access is required',403);}else requirePermission(actor,permissionFor(operation));if(operation==='settings.bootstrap')requirePermission(actor,'finance.write');if(actor.departmentId)fail('POS_BRANCH_REQUIRED','Select a company or branch for POS',403);
 if(process.env.NODE_ENV==='production'&&operation.startsWith('connection.')&&process.env.POS_STRIPE_LIVE_ENABLED!=='true')fail('POS_PROVIDER_COMING_SOON','Enable the Stripe production gate before connecting live accounts',403);
 const hash=createHash('sha256').update(JSON.stringify({user:actor.userId,assignment:actor.assignmentId,company:actor.companyId,branch:actor.branchId,operation,input})).digest('hex');
 try{return await db.transaction(async tx=>{
  if(operation!=='sale.reverse_room_charge'&&!operation.startsWith('connection.'))await assertProductionModuleEnabled(tx,actor.companyId,'pos');
  const grant=await one(tx,"SELECT id FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?)) FOR SHARE",actor.assignmentId,actor.userId,actor.companyId,actor.role,Date.now(),Date.now(),actor.branchId);
  if(!grant)fail('POS_ACCESS_CHANGED','Permissions changed. Select the role again',403);
  await tx.batch([...workContextGuards(tx,actor,key),...iamMutationGuards(tx,actor,key)]);
  // All user commands in one Company serialize with preparation/confirmation.
  // Stock reservations retain their own locks against other modules and jobs.
  await run(tx,"SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))",actor.companyId);
  const old=await one(tx,'SELECT * FROM pos_receipts WHERE id=?',key);if(old){if(old.request_hash!==hash)fail('IDEMPOTENCY_CONFLICT','Command number is already used for other data');await run(tx,'DELETE FROM command_assertions WHERE command_id=?',key);return json<Record<string,unknown>>(old.result_json);}
  const attachmentInput=z.object({id:z.string().min(1),expectedVersion:z.number().int().positive(),attachmentSetIds:z.array(z.string().min(1)).max(20).default([])}).passthrough().safeParse(input);
  const attachmentDescriptor=operation==='sale.cancel'?{targetType:'pos_sale',purposeKey:'void_evidence',bindingType:'pos_void',componentRef:'void'}
    :operation==='refund.approve'||operation==='refund.reject'?{targetType:'pos_refund',purposeKey:'refund_evidence',bindingType:'pos_refund_decision',componentRef:'refund-decision'}
    :operation==='shift.review'?{targetType:'pos_shift',purposeKey:'cash_variance',bindingType:'pos_cash_variance_review',componentRef:'variance-review'}:null;
  const attachmentData=attachmentInput.success?attachmentInput.data:null,attachmentSetIds=attachmentData?.attachmentSetIds??[];
  if(attachmentSetIds.length&&!attachmentDescriptor)fail('POS_ATTACHMENT_ACTION','Files can be bound only to a void, refund decision, or cash-variance review.',422);
  if(attachmentSetIds.length&&!attachmentData)fail('POS_ATTACHMENT_INPUT','Choose evidence for the current record revision.',422);
  if(attachmentSetIds.length){
   const placeholders=attachmentSetIds.map(()=>'?').join(','),sets=(await tx.prepare(`SELECT id,purpose_key,component_kind FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(actor.companyId,...attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string}>()).results;
   if(sets.length!==new Set(attachmentSetIds).size||sets.some(set=>set.component_kind!=='action'||set.purpose_key!==attachmentDescriptor!.purposeKey))fail('POS_ATTACHMENT_PURPOSE','Choose only the evidence purpose for this POS action.',422);
  }
  const attachmentBinding=attachmentSetIds.length?await prepareAttachmentBinding(tx,actor,{attachmentSetIds,bindingContract:{targetType:attachmentDescriptor!.targetType,bindingType:attachmentDescriptor!.bindingType,contexts:[{purposeKey:attachmentDescriptor!.purposeKey,componentKind:'action',componentRef:attachmentDescriptor!.componentRef}]},targetId:attachmentData!.id,expectedTargetVersion:attachmentData!.expectedVersion,commandId:key,bindingId:`${attachmentData!.id}:${operation}:${attachmentData!.expectedVersion+1}`}):null;
  if(attachmentBinding)await tx.batch(attachmentBinding.guards);
  const result=await perform(tx,actor,operation,input,key);
  if(attachmentBinding)await tx.batch(attachmentBinding.steps);
  await run(tx,'INSERT INTO pos_receipts VALUES(?,?,?,?,?)',key,actor.companyId,hash,JSON.stringify(result),Date.now());
  await audit(tx,actor.companyId,actor.branchId,actor.userId,operation,String(result.id??key),{input,result,assignmentId:actor.assignmentId});
  await run(tx,'DELETE FROM command_assertions WHERE command_id=?',key);
  return result;
 });}catch(error){if(error instanceof Error&&error.message==='QUALITY_HOLD')fail('QUALITY_HOLD','This product is quarantined. Quality inspection and independent release are required');if(['23505','23514','40001','40P01'].includes((error as {code?:string}).code??''))fail('POS_CONFLICT','Data or reserved amounts changed. Load the latest data and try again');throw error;}
}
async function bootstrapRetail(db:SqlDatabase,actor:WorkContext,input:unknown){
 const p=z.object({registerId:z.string().min(1).optional(),warehouseId:z.string().min(1).optional(),bankAccountId:z.string().min(1).optional(),mode:z.enum(['retail','restaurant']).default('retail'),registerName:z.string().trim().min(1).max(200).optional()}).strict().parse(input);
 const company=actor.companyId;
 if(actor.branchId)fail('POS_COMPANY_SETTINGS','Run retail setup from the Company workspace',403);
 if(!await one(db,"SELECT id FROM organization_nodes WHERE id=? AND kind='company' FOR UPDATE",company))fail('POS_COMPANY_SETTINGS','Company workspace not found',404);
 if(await one(db,'SELECT company_id FROM pos_settings WHERE company_id=? FOR UPDATE',company))fail('POS_ALREADY_CONFIGURED','POS is already configured. Review the detailed POS settings instead.');
 const existingRetailRegisters=await all(db,"SELECT * FROM pos_registers WHERE company_id=? AND mode=? ORDER BY name,id FOR UPDATE",company,p.mode);

 const commerce=await one(db,'SELECT * FROM commerce_settings WHERE company_id=? FOR UPDATE',company);
 if(!commerce)fail('POS_COMMERCE_SETUP_REQUIRED','Set up Commerce and Finance before configuring POS',409);
 const chartId=str(commerce,'chart_id'),currency=str(commerce,'currency');
 const chart=await one(db,"SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active' FOR SHARE",chartId,company);
 if(!chart)fail('POS_COMMERCE_CHART_REQUIRED','Select an active Commerce chart of accounts in Finance first',409);
 if(str(chart,'base_currency')!==currency)fail('POS_CURRENCY_MISMATCH','Commerce currency must match the chart base currency',422);

 const accounts:Record<string,string>={chartId};
 for(const source of posCommerceAccountDefaults){
  const accountId=str(commerce,source.field);
  const account=accountId?await one(db,"SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category=? AND status='active' AND (currency IS NULL OR currency=?) FOR SHARE",accountId,company,chartId,source.category,currency):null;
  if(!account)fail('POS_COMMERCE_ACCOUNT_INVALID',`Review the Commerce ${source.label} account in Finance before setting up POS`,409);
  accounts[source.key]=accountId;
 }

 let registerId='',registerName='',warehouseId='',branchId='',reuseRegister=false;
 if(existingRetailRegisters.length){
  const activeRegisters=existingRetailRegisters.filter(row=>Boolean(row.active));
  if(!activeRegisters.length)fail('POS_RETAIL_REGISTER_EXISTS','An existing retail register is inactive. Reactivate it in Advanced POS settings first.');
  if(activeRegisters.length>1&&!p.registerId)fail('POS_REGISTER_SELECTION_REQUIRED','Choose which active retail register will use the new POS settings',422);
  const existingRegister=p.registerId?activeRegisters.find(row=>str(row,'id')===p.registerId):activeRegisters.length===1?activeRegisters[0]:null;
  if(!existingRegister)fail('POS_REGISTER_INVALID','Choose an active retail register in this Company',422);
  registerId=str(existingRegister,'id');registerName=str(existingRegister,'name');warehouseId=str(existingRegister,'warehouse_id');branchId=str(existingRegister,'branch_id');reuseRegister=true;
  if(p.warehouseId&&p.warehouseId!==warehouseId)fail('POS_WAREHOUSE_INVALID','The selected register already uses a different warehouse',422);
  const warehouse=await one(db,"SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active' FOR SHARE",warehouseId,company,branchId);
  if(!warehouse)fail('POS_WAREHOUSE_INVALID','The selected register must use an active warehouse in its branch',409);
 }else{
  if(p.registerId)fail('POS_REGISTER_INVALID','Choose an active retail register in this Company',422);
  const warehouses=await all(db,"SELECT id,branch_id,name FROM commerce_warehouses WHERE company_id=? AND status='active' ORDER BY name,id FOR SHARE",company);
  if(!warehouses.length)fail('POS_WAREHOUSE_REQUIRED','Add an active warehouse in Commerce before setting up POS',409);
  warehouseId=p.warehouseId??(warehouses.length===1?str(warehouses[0],'id'):'');
  const warehouse=warehouses.find(row=>str(row,'id')===warehouseId);
  if(warehouses.length>1&&!p.warehouseId)fail('POS_WAREHOUSE_SELECTION_REQUIRED','Choose the warehouse this retail register will use',422);
  if(!warehouse)fail('POS_WAREHOUSE_INVALID','Choose an active warehouse in this Company',422);
  branchId=str(warehouse,'branch_id');
 }

 const bankAccounts=await all(db,"SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND category='asset' AND control_kind='bank' AND status='active' AND (currency IS NULL OR currency=?) ORDER BY code,id FOR UPDATE",company,chartId,currency);
 let bankAccountId='';
 if(bankAccounts.length>1&&!p.bankAccountId)fail('POS_BANK_ACCOUNT_SELECTION_REQUIRED','Choose which bank account POS transfers will use',422);
 if(bankAccounts.length){
  bankAccountId=p.bankAccountId??str(bankAccounts[0],'id');
  if(!bankAccounts.some(row=>str(row,'id')===bankAccountId))fail('POS_BANK_ACCOUNT_INVALID','Choose an active bank account in the Commerce chart and currency',422);
 }else if(p.bankAccountId){
  fail('POS_BANK_ACCOUNT_INVALID','No eligible Finance bank account is available for POS transfers',422);
 }

 const now=Date.now(),createdAccounts:Array<{id:string;code:string;name:string;category:string}>=[];
 for(const spec of posGeneratedAccountSpecs){
  if(spec.key==='bank'&&bankAccountId){accounts.bank=bankAccountId;continue;}
  let code:string=spec.code,suffix=2;
  while(await one(db,'SELECT id FROM finance_accounts WHERE chart_id=? AND code=?',chartId,code))code=`${spec.code}-${suffix++}`;
  const accountId=id();
  await run(db,'INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)',accountId,chartId,company,code,spec.name,spec.category,spec.key==='bank'?currency:null,spec.controlKind==='bank'?1:0,spec.controlKind,now,now);
  accounts[spec.key]=accountId;
  createdAccounts.push({id:accountId,code,name:spec.name,category:spec.category});
 }

 if(!reuseRegister){
  registerId=id();registerName=p.registerName??`Main ${p.mode} register`;let nameSuffix=2;
  while(await one(db,'SELECT id FROM pos_registers WHERE company_id=? AND branch_id=? AND name=?',company,branchId,registerName))registerName=`${p.registerName??`Main ${p.mode} register`} ${nameSuffix++}`;
 }
 await run(db,"INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,true,?,'Asia/Bangkok',?,0)",company,currency,JSON.stringify(accounts));
 if(!reuseRegister)await run(db,"INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,true)",registerId,company,branchId,warehouseId,registerName,p.mode);
 return {id:company,mode:p.mode,registerId,registerName,warehouseId,bankAccountId:accounts.bank,registerReused:reuseRegister,accountsCreated:createdAccounts};
}
async function perform(db:SqlDatabase,actor:WorkContext,op:string,input:unknown,commandKey:string):Promise<Record<string,unknown>> {
 const company=actor.companyId,user=actor.userId;
 if(op.startsWith('day.'))return posDayCommand(db,actor,op,input);
 if(op==='customer.create')return createPosCustomer(db,actor,input);
 if(op==='connection.create'||op==='connection.refresh'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Connect Stripe from the company area',403);
  const p=z.object({livemode:z.boolean().default(false)}).parse(input),job=id();
  await enqueue(db,company,op==='connection.create'?'connect':'connect_refresh',job,{livemode:p.livemode,userId:user,holdingSlug:actor.holdingSlug,assignmentId:actor.assignmentId});return {id:job};
 }
 if(op==='connection.oauth'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Connect Stripe from the company area',403);
  const p=z.object({livemode:z.boolean().default(false)}).parse(input),state=id();
  await run(db,'INSERT INTO pos_oauth_states(id,company_id,user_id,expires_at,livemode) VALUES(?,?,?,?,?)',state,company,user,Date.now()+600000,p.livemode);
  return {id:state,livemode:p.livemode};
 }
 if(op==='settings.bootstrap')return bootstrapRetail(db,actor,input);
 if(op==='settings.save'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Configure accounts from the company area',403);
  const p=settingsSchema.parse(input),old=await one(db,'SELECT * FROM pos_settings WHERE company_id=? FOR UPDATE',company);
  if((old?num(old,'version'):0)!==p.expectedVersion)fail('POS_VERSION_CONFLICT','Settings changed');
  for(const k of accountKeys)if(!await one(db,"SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active'",p.accounts[k],company,p.accounts.chartId))fail('POS_ACCOUNT_INVALID',`Review account ${k}`,422);
  const chart=await one(db,'SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?',p.accounts.chartId,company);
  if(chart?.base_currency!==p.currency)fail('POS_CURRENCY_MISMATCH',"Currency must match the chart's base currency",422);
  const commerce=await one(db,'SELECT currency FROM commerce_settings WHERE company_id=?',company);
  if(commerce?.currency!==p.currency)fail('POS_CURRENCY_MISMATCH','POS currency must match Commerce and the company accounts',422);
  await run(db,'INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,accounts_json=excluded.accounts_json,cashier_discount_bps=excluded.cashier_discount_bps,version=pos_settings.version+1',company,p.enabled,p.currency,p.timezone,JSON.stringify(p.accounts),p.cashierDiscountBps);return {id:company};
 }
 if(op==='register.save'){
  const p=z.object({id:z.string().optional(),warehouseId:z.string(),name:z.string().trim().min(1).max(100),mode:z.enum(['retail','restaurant']),active:z.boolean().default(true),expectedVersion:z.number().int().min(0).default(0)}).parse(input);
  const warehouse=scopeRow(actor,await one(db,"SELECT * FROM commerce_warehouses WHERE id=? AND status='active'",p.warehouseId));
  const rid=p.id??id();if(p.id){const previous=scopeRow(actor,await one(db,'SELECT * FROM pos_registers WHERE id=? FOR UPDATE',p.id));version(previous,p);if(await one(db,"SELECT id FROM pos_shifts WHERE register_id=? AND state='open'",p.id))fail('POS_REGISTER_OPEN','Close the shift before editing the sales point');await run(db,'UPDATE pos_registers SET warehouse_id=?,branch_id=?,name=?,mode=?,active=?,version=version+1 WHERE id=?',p.warehouseId,warehouse.branch_id,p.name,p.mode,p.active,rid);}else await run(db,'INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,?)',rid,company,warehouse.branch_id,p.warehouseId,p.name,p.mode,p.active);
  return {id:rid};
 }
 if(op==='table.create'){
  const p=z.object({registerId:z.string(),name:z.string().trim().min(1).max(40),seats:z.number().int().min(1).max(100)}).parse(input);
  scopeRow(actor,await one(db,'SELECT * FROM pos_registers WHERE id=? FOR UPDATE',p.registerId));if(await one(db,'SELECT id FROM pos_tables WHERE register_id=? AND name=?',p.registerId,p.name))fail('POS_TABLE_NAME_EXISTS','A table with this name already exists at this register',422);const tid=id();await run(db,'INSERT INTO pos_tables VALUES(?,?,?,?)',tid,p.registerId,p.name,p.seats);return {id:tid};
 }
 if(op==='catalog.image'){
  const p=z.object({productId:z.string().min(1),imageId:z.string().min(1),expectedVersion:z.number().int().positive()}).parse(input);
  const c=await one(db,'SELECT * FROM pos_catalog WHERE product_id=? AND company_id=? FOR UPDATE',p.productId,company);
  if(!c)fail('POS_PRODUCT_INVALID','Catalog not found',404);version(c,p);
  if(actor.branchId&&!await one(db,'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?',p.productId,actor.branchId,company))fail('POS_PRODUCT_INVALID','Catalog is outside this Branch',404);
  await requirePosImageSource(db,actor,p.imageId,p.productId,commandKey);
  await run(db,'UPDATE pos_catalog SET image_id=?,version=version+1 WHERE product_id=?',p.imageId,p.productId);return {id:p.productId,imageId:p.imageId};
 }
 if(op==='catalog.save'){
  const p=catalogSchema.parse(input);if(!await one(db,"SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'",p.productId,company))fail('POS_PRODUCT_INVALID','Select a company product',422);
  if(p.kind!=='recipe'&&p.modifiers.some(m=>m.ingredients.length))fail('POS_RECIPE_REQUIRED','Ingredient modifiers require a recipe menu',422);
  if(p.kind==='recipe'&&!p.recipe.length)fail('POS_RECIPE_REQUIRED','Enter recipe ingredients',422);
  if(new Set(p.modifiers.map(m=>m.id)).size!==p.modifiers.length)fail('POS_MODIFIER_DUPLICATE','Duplicate modifier ID',422);
  for(const part of [...p.recipe,...p.modifiers.flatMap(m=>m.ingredients)])if(!await one(db,"SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'",part.productId,company))fail('POS_INGREDIENT_INVALID','Ingredients must be company products',422);
  const old=await one(db,'SELECT * FROM pos_catalog WHERE product_id=? FOR UPDATE',p.productId);if((old?num(old,'version'):0)!==p.expectedVersion)fail('POS_VERSION_CONFLICT','Product data changed');
  if(p.imageId)await requirePosImageSource(db,actor,p.imageId,p.productId,commandKey);
  const registerIds=p.registerIds??(old?undefined:(await all(db,"SELECT id FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?<>'recipe' OR mode='restaurant')",company,actor.branchId,actor.branchId,p.kind)).map(r=>str(r,'id')));
  if(registerIds){for(const rid of registerIds){const register=scopeRow(actor,await one(db,'SELECT * FROM pos_registers WHERE id=?',rid));if(p.kind==='recipe'&&register.mode!=='restaurant')fail('POS_RESTAURANT_REQUIRED','Recipes require restaurant registers',422);}}
  await run(db,'INSERT INTO pos_catalog(product_id,company_id,barcode,category,kind,station,recipe_json,modifiers_json,tax_inclusive,enabled,image_id) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(product_id) DO UPDATE SET barcode=excluded.barcode,category=excluded.category,kind=excluded.kind,station=excluded.station,recipe_json=excluded.recipe_json,modifiers_json=excluded.modifiers_json,tax_inclusive=excluded.tax_inclusive,enabled=excluded.enabled,image_id=excluded.image_id,version=pos_catalog.version+1',p.productId,company,p.barcode||null,p.category,p.kind,p.station,JSON.stringify(p.recipe),JSON.stringify(p.modifiers),p.taxInclusive,p.enabled,p.imageId===undefined?old?.image_id??null:p.imageId);
  if(registerIds){if(actor.branchId)await run(db,'DELETE FROM pos_register_catalog WHERE product_id=? AND register_id IN (SELECT id FROM pos_registers WHERE company_id=? AND branch_id=?)',p.productId,company,actor.branchId);else await run(db,'DELETE FROM pos_register_catalog WHERE product_id=?',p.productId);for(const rid of new Set(registerIds))await run(db,'INSERT INTO pos_register_catalog(register_id,product_id) VALUES(?,?)',rid,p.productId);}
  return {id:p.productId};
 }
 if(op==='shift.open'){
  const p=z.object({registerId:z.string(),openingMinor:money}).parse(input),register=scopeRow(actor,await one(db,'SELECT * FROM pos_registers WHERE id=? AND active=true FOR UPDATE',p.registerId)),settings=await config(db,company);if(!settings.enabled)fail('POS_DISABLED','POS transactions are not enabled');
  const hotel=await one(db,'SELECT state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? FOR SHARE',company,register.branch_id);
  if(hotel&&(hotel.state!=='open'||hotel.business_date!==businessDate()))fail('POS_HOTEL_DAY_CLOSED','Finish or recover the hotel day close before opening a restaurant cash session');
  await assertPosDayWritable(db,company,str(register,'branch_id'),businessDate());
  await openPeriod(db,company,businessDate());const sid=id();await run(db,'INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,business_date) VALUES(?,?,?,?,?,?,?,?)',sid,register.id,company,register.branch_id,user,Date.now(),p.openingMinor,businessDate());return {id:sid};
 }
 if(op.startsWith('shift.')||op.startsWith('cash.')){
  const p=base.extend({amountMinor:z.number().int().min(-99999999999).max(99999999999).optional(),countedMinor:money.optional(),denominations:z.record(z.string(),z.number().int().nonnegative().max(100000)).optional(),reason:z.string().trim().min(3).max(1000).optional(),movementId:z.string().optional()}).parse(input);
  const shift=scopeRow(actor,await one(db,'SELECT * FROM pos_shifts WHERE id=? FOR UPDATE',p.id));version(shift,p);
  await assertPosDayWritable(db,company,str(shift,'branch_id'),str(shift,'business_date'));
  const settings=await config(db,company),a=json<PosAccounts>(settings.accounts_json);
  if(op==='cash.create'){
   if(shift.state!=='open'||!p.amountMinor||!p.reason)fail('POS_CASH_INVALID','Enter the amount and reason in an open cash session',422);
   const mid=id();await run(db,'INSERT INTO pos_cash_movements VALUES(?,?,?,?,?,?,?)',mid,shift.id,p.amountMinor,p.reason,user,null,Date.now());await run(db,'UPDATE pos_shifts SET version=version+1 WHERE id=?',shift.id);return {id:mid};
  }
  if(op==='cash.approve'){
   if(shift.state!=='open')fail('POS_SHIFT_CLOSED','Cash session is closed');
   const m=await one(db,'SELECT * FROM pos_cash_movements WHERE id=? AND shift_id=? FOR UPDATE',p.movementId,shift.id);if(!m||m.approved_by||samePerson(actor,m.created_by))fail('POS_INDEPENDENT_REVIEW','A different approver must review the record');
   await posting(db,company,str(shift,'branch_id'),a,businessDate(),str(settings,'currency'),`cash:${m.id}`,str(shift,'id'),user,pair(num(m,'amount_minor')>0?a.cash:a.cashTransfer,num(m,'amount_minor')>0?a.cashTransfer:a.cash,Math.abs(num(m,'amount_minor'))));await run(db,'UPDATE pos_cash_movements SET approved_by=? WHERE id=?',user,m.id);
  }else if(op==='shift.close'){
   if(shift.state!=='open'||p.countedMinor===undefined)fail('POS_SHIFT_INVALID','Enter the actual counted amount',422);
   if(await one(db,"SELECT id FROM pos_sales WHERE shift_id=? AND state IN ('open','held','payment_pending')",shift.id))fail('POS_OPEN_SALES','Close or cancel open bills before closing the cash session');
   if(await one(db,'SELECT id FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NULL',shift.id))fail('POS_CASH_REVIEW_PENDING','There are unapproved cash movements');
   const cash=await one(db,"SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=? AND p.method='cash' AND p.state='succeeded'",shift.id);
   const refunds=await one(db,"SELECT COALESCE(SUM(rp.amount_minor),0) n FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id JOIN pos_refunds r ON r.id=rp.refund_id WHERE r.payout_shift_id=? AND p.method='cash' AND rp.state='succeeded'",shift.id);
   const movements=await one(db,'SELECT COALESCE(SUM(amount_minor),0) n FROM pos_cash_movements WHERE shift_id=? AND approved_by IS NOT NULL',shift.id);
   const expected=num(shift,'opening_minor')+num(cash!,'n')-num(refunds!,'n')+num(movements!,'n');
   if(p.denominations){const denominations=[100000,50000,10000,5000,2000,1000,500,200,100,50,25];if(Object.keys(p.denominations).some(k=>!denominations.includes(Number(k)))||Object.entries(p.denominations).reduce((n,[k,v])=>n+Number(k)*v,0)!==p.countedMinor)fail('POS_CASH_COUNT','Denomination counts must match the counted amount',422);}
   await run(db,"UPDATE pos_shifts SET state='closed',counted_minor=?,expected_minor=?,variance_minor=?,closed_at=?,closed_by=?,denominations_json=?::jsonb WHERE id=?",p.countedMinor,expected,p.countedMinor-expected,Date.now(),user,p.denominations?JSON.stringify(p.denominations):null,shift.id);
  }else if(op==='shift.review'){
   if(shift.state!=='closed'||samePerson(actor,shift.opened_by)||samePerson(actor,shift.closed_by)||!p.reason)fail('POS_INDEPENDENT_REVIEW','A different manager must review the total with a reason');
   const variance=num(shift,'variance_minor');await posting(db,company,str(shift,'branch_id'),a,businessDate(),str(settings,'currency'),`shift:${shift.id}`,str(shift,'id'),user,pair(variance>0?a.cash:a.cashDifference,variance>0?a.cashDifference:a.cash,Math.abs(variance)));
   await run(db,"UPDATE pos_shifts SET state='reviewed',reviewed_by=?,review_reason=? WHERE id=?",user,p.reason,shift.id);
  }else fail('POS_ACTION_UNKNOWN','Command not found',404);
  await run(db,'UPDATE pos_shifts SET version=version+1 WHERE id=?',shift.id);return {id:shift.id};
 }
 if(op==='sale.create'){
  const p=z.object({shiftId:z.string(),tableId:z.string().nullable().optional(),guests:z.number().int().min(1).max(100).default(1),partyId:z.string().nullable().optional(),serviceMode:z.enum(['retail','dine_in','takeaway']),currency:z.string().regex(/^[A-Z]{3}$/).refine(value=>Boolean(currencyDefinition(value))).optional()}).parse(input);
  const shift=scopeRow(actor,await one(db,"SELECT * FROM pos_shifts WHERE id=? AND state='open' FOR UPDATE",p.shiftId)),settings=await config(db,company);if(!settings.enabled)fail('POS_DISABLED','New transactions are temporarily paused');
  if(p.currency&&p.currency!==settings.currency)fail('POS_CURRENCY_SETUP_REQUIRED',`The selected ${p.currency} currency needs a POS register, supported payment accounts and pricing setup before a new sale can start.`,422);
  if(p.currency&&p.currency!=='USD'&&!await one(db,'SELECT id FROM company_fx_rates WHERE company_id=? AND currency=? ORDER BY effective_date DESC,confirmed_at DESC LIMIT 1',company,p.currency))fail('MANUAL_RATE_REQUIRED',`Confirm a manual USD/${p.currency} rate before starting this sale.`,422);
  const register=(await one(db,'SELECT * FROM pos_registers WHERE id=? AND active=true',shift.register_id))!;
  await assertPosDayWritable(db,company,str(shift,'branch_id'),businessDate());
  if(shift.business_date!==businessDate())fail('POS_SHIFT_DATE','Close the previous business day cash session before selling');
  if(!register)fail('POS_REGISTER_DISABLED','Sales point is disabled');
  if(p.serviceMode==='retail'&&register.mode!=='retail'||p.serviceMode!=='retail'&&register.mode!=='restaurant')fail('POS_MODE_INVALID','Mode does not match the sales point',422);
  if(p.tableId&&!await one(db,'SELECT id FROM pos_tables WHERE id=? AND register_id=?',p.tableId,register.id))fail('POS_TABLE_INVALID','Table is not at this sales point',422);
  if(p.partyId&&!await one(db,"SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active'",p.partyId,company))fail('POS_CUSTOMER_INVALID','Customer is not available',422);
  const sid=id();await run(db,'INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,table_id,guests,service_mode,created_by,created_at,updated_at,accounts_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',sid,company,shift.branch_id,register.id,shift.id,`POS-${businessDate().replaceAll('-','')}-${sid.slice(0,8).toUpperCase()}`,settings.currency,businessDate(),p.partyId??null,p.tableId??null,p.guests,p.serviceMode,user,Date.now(),Date.now(),settings.accounts_json);const issuer=await one(db,'SELECT display_name FROM organization_nodes WHERE id=?',company);await run(db,'UPDATE pos_sales SET issuer_json=? WHERE id=?',JSON.stringify({companyName:issuer?.display_name??company,registerName:register.name,branchId:shift.branch_id}),sid);return {id:sid};
 }
 if(op.startsWith('sale.')){
  const p=base.passthrough().parse(input),sale=await saleRow(db,actor,p.id);version(sale,p);
  await assertPosDayWritable(db,company,str(sale,'branch_id'),op==='sale.reverse_room_charge'?businessDate():str(sale,'business_date'));
  if(op==='sale.reverse_room_charge'){
   if(await one(db,"SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'",sale.company_id,sale.branch_id,sale.business_date))fail('POS_DAY_REVIEW_PENDING','Finance must confirm the original day close before a room-charge correction can change its source records');
   requirePermission(actor,'pos.approve');requirePermission(actor,'pms.approve');
   const q=base.extend({propertyId:z.string().min(1),expectedPropertyVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000),restockLineIds:z.array(z.string()).max(100).default([])}).parse(input);
   const chargeAudit=await one(db,"SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=? ORDER BY created_at DESC LIMIT 1",sale.id);
   if(samePerson(actor,sale.created_by)||chargeAudit&&samePerson(actor,chargeAudit.actor_id))fail('POS_INDEPENDENT_REVIEW','A different Finance approver must reverse this room charge');
   if(sale.state!=='completed'||await one(db,'SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?',sale.id))fail('POS_ROOM_CHARGE_REVERSED','This room charge is already reversed or the bill is not complete');
   if(await one(db,'SELECT id FROM pos_payments WHERE sale_id=?',sale.id))fail('POS_ROOM_CHARGE_PAYMENT','This bill has POS payments and cannot use room-charge reversal');
   const source=await one(db,'SELECT ps.property_id,ps.record_id FROM pms_sources ps JOIN pms_properties p ON p.id=ps.property_id WHERE ps.source_key=? AND p.company_id=? AND p.branch_id=?',`pos:${sale.id}:${sale.id}`,company,sale.branch_id);
   if(!source||source.property_id!==q.propertyId)fail('POS_ROOM_CHARGE_MISSING','The linked hotel charge was not found');
   if(new Set(q.restockLineIds).size!==q.restockLineIds.length)fail('POS_DUPLICATE_LINE','Select each restocked line once',422);
   const lines=await all(db,'SELECT l.*,f.kind,f.state fulfillment_state,f.cost_minor fulfillment_cost,f.quantity_milli fulfillment_quantity,f.warehouse_id,f.product_id FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id',sale.id);
   if(q.restockLineIds.some(lineId=>!lines.some(line=>line.id===lineId&&line.kind==='stock'&&line.fulfillment_state==='delivered')))fail('POS_RETURN_NOT_STOCK','Only delivered stock items can be restocked',422);
   const reversal=await reversePosRoomCharge(db,actor,{propertyId:q.propertyId,sourceKey:`pos:${sale.id}:${sale.id}`,expectedPropertyVersion:q.expectedPropertyVersion,reason:q.reason,total:num(sale,'total_minor')});
   const accounts=json<PosAccounts>(sale.accounts_json),date=reversal.businessDate;let restored=0,wasted=0;
   await assertPosDayWritable(db,company,str(sale,'branch_id'),date);
   await openPeriod(db,company,date);
   for(const line of lines){
    const prior=await one(db,'SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?',line.fulfillment_id,line.id);
    const cost=portion(num(line,'fulfillment_cost'),num(prior!,'n'),num(line,'quantity_milli'),num(line,'fulfillment_quantity'));
    if(q.restockLineIds.includes(str(line,'id'))){
     const stock=await one(db,'SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE',line.warehouse_id,line.product_id);
     if(!stock)fail('POS_STOCK_MISSING','The original warehouse stock is unavailable');
     await run(db,'UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?',line.quantity_milli,cost,line.warehouse_id,line.product_id);
     await run(db,'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',id(),company,line.warehouse_id,line.product_id,line.fulfillment_id,sale.id,sale.number,'return',line.quantity_milli,cost,date,Date.now());
     restored+=cost;
    }else wasted+=cost;
   }
   await posting(db,company,str(sale,'branch_id'),accounts,date,str(sale,'currency'),`room-reversal:${sale.id}`,str(sale,'id'),user,[...pair(accounts.inventory,accounts.cogs,restored),...pair(accounts.waste,accounts.cogs,wasted)]);
   await run(db,'INSERT INTO pos_room_charge_reversals(sale_id,company_id,property_id,original_entry_id,reversal_entry_id,reason,restocked_lines_json,restored_cost_minor,wasted_cost_minor,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)',sale.id,company,q.propertyId,reversal.entryId,reversal.reversalEntryId,q.reason,JSON.stringify(q.restockLineIds),restored,wasted,user,Date.now());
   await capturePosReversal(db,sale,'pos_room_charge_reversal',str(sale,'id'),'total',date,String(sale.total_minor),{sourceType:'pos_sale',sourceId:str(sale,'id'),amountKind:'total'});
   await run(db,'UPDATE pos_sales SET version=version+1,updated_at=? WHERE id=?',Date.now(),sale.id);
   return {id:sale.id,state:'room_charge_reversed',reversalEntryId:reversal.reversalEntryId,restoredCostMinor:restored,wastedCostMinor:wasted};
  }
 if(op==='sale.complete')return completeSale(db,sale,user);
  if(op==='sale.charge_to_room'){
   if(process.env.NODE_ENV==='production'&&process.env.ERPX_POS_ROOM_CHARGE_PILOT_ENABLED!=='true')fail('POS_ROOM_CHARGE_PILOT_GATE','Room charges require pilot acceptance before production activation',403);
   const p=base.extend({propertyId:z.string().min(1),stayId:z.string().min(1),expectedPropertyVersion:z.number().int().positive()}).parse(input);
   if(sale.state!=='open'&&sale.state!=='held')fail('POS_SALE_LOCKED','Only an unpaid bill can be charged to a room');
   if(!await one(db,"SELECT id FROM pos_registers WHERE id=? AND mode='restaurant'",sale.register_id))fail('POS_ROOM_CHARGE_REGISTER','Only restaurant bills can be charged to a room');
   if(!await one(db,"SELECT id FROM pos_shifts WHERE id=? AND state='open'",sale.shift_id))fail('POS_SHIFT_CLOSED','Cash session is closed');
   const settings=await config(db,company);if(!settings.enabled)fail('POS_DISABLED','New transactions are temporarily paused');
   if(await one(db,'SELECT id FROM pos_payments WHERE sale_id=? LIMIT 1',sale.id))fail('POS_PAYMENT_STARTED','Room charge requires the full unpaid bill');
   const lines=await all(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale.id);if(!lines.length)fail('POS_EMPTY_SALE','Add items before closing the bill',422);
   const gross=await one(db,'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?',sale.id);
   if(num(sale,'discount_minor')>scaledAmount(num(gross!,'n'),num(settings,'cashier_discount_bps'),10000)&&!sale.discount_approved_by)fail('POS_DISCOUNT_APPROVAL','This discount must be approved before room charge');
   await openPeriod(db,company,str(sale,'business_date'));
   const property=await one(db,'SELECT branch_id,business_date FROM pms_properties WHERE id=? AND company_id=?',p.propertyId,company);
   if(!property||property.branch_id!==sale.branch_id)fail('POS_PROPERTY_SCOPE','Select a hotel in this restaurant branch',404);
   if(property.business_date!==sale.business_date)fail('POS_BUSINESS_DATE','Restaurant and hotel business dates must match');
   for(const itemId of new Set(lines.map(l=>str(l,'fulfillment_id')))){
    const f=(await one(db,'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',itemId))!;
    if(f.state==='cancelled')fail('POS_ITEM_CANCELLED','Remove cancelled items before closing the bill');
    if(f.kind==='recipe'&&f.state!=='delivered')fail('POS_SEND_KITCHEN','Deliver kitchen items before charging the room');
    if(f.kind!=='recipe'&&f.state!=='delivered'){
     await consume(db,f,user,false);
     await run(db,"UPDATE pos_fulfillments SET state='delivered',version=version+1 WHERE id=?",f.id);
    }
   }
   const result=await applyPosRoomCharge(db,actor,{
    source:'pos',sourceId:str(sale,'id'),sourceLineId:str(sale,'id'),companyId:company,
    branchId:str(sale,'branch_id'),propertyId:p.propertyId,stayId:p.stayId,
    currency:str(sale,'currency'),revenueOwner:'pms',sourceJournalId:null,
    description:`Restaurant bill ${str(sale,'number')}`,net:num(sale,'subtotal_minor'),
    service:0,tax:num(sale,'tax_minor'),total:num(sale,'total_minor'),
   },p.expectedPropertyVersion);
   await capturePosMoney(db,sale,'pos_sale',str(sale,'id'),'total',str(sale,'business_date'),String(sale.total_minor));
   await run(db,"UPDATE pos_sales SET state='completed',table_id=NULL,version=version+1,updated_at=? WHERE id=?",Date.now(),sale.id);
   await capturePosReceipt(db,str(sale,'id'));
   return {id:sale.id,state:'completed',tender:'room_charge',propertyId:p.propertyId,stayId:p.stayId,folioEntryId:result.entryId};
  }
  if(op==='sale.approve_discount'){
   const q=z.object({reason:z.string().trim().min(3).max(1000)}).parse(p);if(samePerson(actor,sale.discount_requested_by??sale.created_by))fail('POS_INDEPENDENT_REVIEW','A different manager must approve the discount');
   await run(db,'UPDATE pos_sales SET discount_approved_by=?,discount_reason=?,version=version+1 WHERE id=?',user,q.reason,sale.id);return {id:sale.id};
  }
  await editable(db,sale);
  if(op==='sale.add'){
   await run(db,'UPDATE pos_sales SET discount_requested_by=? WHERE id=?',user,sale.id);
   const q=z.object({productId:z.string(),quantityMilli:qty,modifierIds:z.array(z.string()).max(30).default([]),discountMinor:money.default(0),note:z.string().max(500).default('')}).parse(p);
   const product=await one(db,"SELECT p.*,c.kind,c.station,c.recipe_json,c.modifiers_json,c.tax_inclusive,c.version catalog_version FROM commerce_products p JOIN pos_catalog c ON c.product_id=p.id AND c.company_id=p.company_id AND c.enabled=true JOIN pos_register_catalog rc ON rc.product_id=c.product_id AND rc.register_id=? AND rc.enabled=true WHERE p.id=? AND p.company_id=? AND p.status='active'",sale.register_id,q.productId,company);if(!product)fail('POS_PRODUCT_INVALID','สินค้านี้ไม่ได้เปิดขายใน catalog ของจุดขาย กรุณาให้ผู้ดูแลเปิดขายก่อน',422);
   const validation=z.object({expectedCatalogVersion:z.number().int().positive().optional(),expectedPriceMinor:money.optional()}).parse(p);
   if(validation.expectedCatalogVersion!==undefined&&validation.expectedCatalogVersion!==num(product,'catalog_version')||validation.expectedPriceMinor!==undefined&&validation.expectedPriceMinor!==num(product,'price_minor'))fail('POS_CATALOG_CHANGED','ราคา/ตัวเลือกเปลี่ยน กรุณาตรวจตะกร้าก่อนส่งใหม่');
   const modifiers=json<z.infer<typeof catalogSchema>['modifiers']>(product.modifiers_json??'[]').filter(m=>q.modifierIds.includes(m.id));if(modifiers.length!==q.modifierIds.length)fail('POS_MODIFIER_INVALID','Invalid modifiers',422);
   const kind=str(product,'kind')||'stock',price=num(product,'price_minor')+modifiers.reduce((n,m)=>n+m.priceMinor,0),amounts=saleAmounts(q.quantityMilli,price,num(product,'tax_bps'),Boolean(product.tax_inclusive),q.discountMinor);
   if(kind==='recipe'&&sale.service_mode==='retail')fail('POS_RESTAURANT_REQUIRED','Prepared menus require a restaurant sales point',422);
   const recipe=kind==='stock'?[{productId:q.productId,quantityMilli:1000}]:kind==='recipe'?json<Array<{productId:string;quantityMilli:number}>>(product.recipe_json):[];
   recipe.push(...modifiers.flatMap(m=>m.ingredients));
   const register=(await one(db,'SELECT * FROM pos_registers WHERE id=?',sale.register_id))!,fid=id(),lid=id();
   await run(db,'INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',fid,company,sale.branch_id,register.warehouse_id,q.productId,q.quantityMilli,kind,str(product,'station')||'Kitchen',q.note,JSON.stringify(recipe),sale.accounts_json,sale.currency,sale.business_date,Date.now());
   await run(db,'INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',lid,sale.id,fid,q.productId,product.name,q.quantityMilli,price,product.tax_bps,Boolean(product.tax_inclusive),q.discountMinor,amounts.subtotal,amounts.tax,JSON.stringify(modifiers),q.note);
  }else if(op==='sale.customer'){
   const q=z.object({partyId:z.string().nullable()}).parse(p);
   if(q.partyId&&!await one(db,"SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active'",q.partyId,company))fail('POS_CUSTOMER_INVALID','Customer is not available',422);
   await run(db,'UPDATE pos_sales SET party_id=? WHERE id=?',q.partyId,sale.id);
  }else if(op==='sale.update_line'){
   const q=z.object({lineId:z.string(),quantityMilli:qty,discountMinor:money,note:z.string().max(500).default('')}).parse(p);
   const line=await one(db,'SELECT l.*,f.state fulfillment_state,f.quantity_milli fulfillment_quantity FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.id=? AND l.sale_id=? FOR UPDATE OF l,f',q.lineId,sale.id);
   if(!line)fail('POS_LINE_INVALID','Line is not on this bill',422);
   if(q.quantityMilli!==num(line,'quantity_milli')&&line.fulfillment_state!=='draft')fail('POS_ITEM_IN_KITCHEN','รายการที่ส่งครัวแล้วต้องแยกบิล หรือให้ผู้อนุมัติยกเลิกพร้อมบันทึกของเสีย');
   if(line.fulfillment_state==='draft')await run(db,'UPDATE pos_fulfillments SET quantity_milli=?,note=?,version=version+1 WHERE id=?',q.quantityMilli,q.note,line.fulfillment_id);
   const amounts=saleAmounts(q.quantityMilli,num(line,'price_minor'),num(line,'tax_bps'),Boolean(line.tax_inclusive),q.discountMinor+num(line,'bill_discount_minor'));
   await run(db,'UPDATE pos_lines SET quantity_milli=?,discount_minor=?,subtotal_minor=?,tax_minor=?,note=? WHERE id=?',q.quantityMilli,amounts.discount,amounts.subtotal,amounts.tax,q.note,line.id);
   await run(db,'UPDATE pos_sales SET discount_requested_by=? WHERE id=?',user,sale.id);
  }else if(op==='sale.discount'){
   const q=z.object({amountMinor:money}).parse(p),lines=await all(db,'SELECT * FROM pos_lines WHERE sale_id=? ORDER BY id FOR UPDATE',sale.id);
   const weights=lines.map(l=>scaledAmount(num(l,'price_minor'),num(l,'quantity_milli'),1000)-num(l,'discount_minor')+num(l,'bill_discount_minor'));
   const total=weights.reduce((n,w)=>n+w,0);if(q.amountMinor>total)fail('POS_DISCOUNT_AMOUNT','Discount exceeds the bill amount',422);
   let previous=0;
   for(const [i,l] of lines.entries()){
    const extra=weights[i]&&q.amountMinor?portion(q.amountMinor,previous,weights[i],total):0;previous+=weights[i];
    const discount=num(l,'discount_minor')-num(l,'bill_discount_minor')+extra;
    const amounts=saleAmounts(num(l,'quantity_milli'),num(l,'price_minor'),num(l,'tax_bps'),Boolean(l.tax_inclusive),discount);
    await run(db,'UPDATE pos_lines SET discount_minor=?,bill_discount_minor=?,subtotal_minor=?,tax_minor=? WHERE id=?',discount,extra,amounts.subtotal,amounts.tax,l.id);
   }
   await run(db,'UPDATE pos_sales SET discount_requested_by=? WHERE id=?',user,sale.id);
  }else if(op==='sale.remove'){
   const q=z.object({lineId:z.string()}).parse(p),line=await one(db,'SELECT l.*,f.state fulfillment_state FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.id=? AND l.sale_id=? FOR UPDATE OF l,f',q.lineId,sale.id);if(!line||!['draft','cancelled'].includes(str(line,'fulfillment_state')))fail('POS_ITEM_IN_KITCHEN','Cancel the kitchen item before removing it from the bill');
   await run(db,'DELETE FROM pos_reservations WHERE fulfillment_id=?',line.fulfillment_id);await run(db,'DELETE FROM pos_lines WHERE id=?',line.id);
  }else if(op==='sale.hold'||op==='sale.resume'){await run(db,'UPDATE pos_sales SET state=? WHERE id=?',op==='sale.hold'?'held':'open',sale.id);
  }else if(op==='sale.move_table'){
   const q=z.object({tableId:z.string()}).parse(p);if(!await one(db,'SELECT id FROM pos_tables WHERE id=? AND register_id=?',q.tableId,sale.register_id))fail('POS_TABLE_INVALID','Table is not at this sales point',422);await run(db,'UPDATE pos_sales SET table_id=? WHERE id=?',q.tableId,sale.id);
  }else if(op==='sale.merge'){
   const q=z.object({targetId:z.string(),targetVersion:z.number().int().positive()}).parse(p);if(q.targetId===sale.id)fail('POS_MERGE_INVALID','Select a different bill',422);
   const target=await saleRow(db,actor,q.targetId);version(target,{expectedVersion:q.targetVersion});await editable(db,target);
   if(target.register_id!==sale.register_id||target.shift_id!==sale.shift_id||target.party_id!==sale.party_id)fail('POS_MERGE_INVALID','Bills must use the same sales point, cash session, and customer');
   await run(db,'UPDATE pos_lines SET sale_id=? WHERE sale_id=?',target.id,sale.id);await run(db,"UPDATE pos_sales SET state='merged',table_id=NULL WHERE id=?",sale.id);await refreshSale(db,str(target,'id'));
  }else if(op==='sale.split'){
   const q=z.object({lineId:z.string(),quantityMilli:qty}).parse(p),line=await one(db,'SELECT * FROM pos_lines WHERE id=? AND sale_id=? FOR UPDATE',q.lineId,sale.id);if(!line||q.quantityMilli>num(line,'quantity_milli'))fail('POS_SPLIT_INVALID','Invalid split quantity',422);
   const target=id(),fid=str(line,'fulfillment_id'),f=(await one(db,'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',fid))!;let newFid=fid;
   await run(db,"INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,party_id,guests,service_mode,created_by,created_at,updated_at,accounts_json) SELECT ?,company_id,branch_id,register_id,shift_id,?,currency,business_date,party_id,guests,service_mode,?,?,?,accounts_json FROM pos_sales WHERE id=?",target,`POS-${target.slice(0,8).toUpperCase()}`,user,Date.now(),Date.now(),sale.id);
   if(q.quantityMilli===num(line,'quantity_milli'))await run(db,'UPDATE pos_lines SET sale_id=? WHERE id=?',target,line.id);
   else {
    if(f.kind!=='recipe'||f.state==='draft'){
     if(f.reserved)fail('POS_SPLIT_RESERVED','Reserved items cannot be split yet');newFid=id();
     await run(db,'INSERT INTO pos_fulfillments(id,company_id,branch_id,warehouse_id,product_id,quantity_milli,kind,station,note,recipe_json,accounts_json,currency,business_date,created_at) SELECT ?,company_id,branch_id,warehouse_id,product_id,?,kind,station,note,recipe_json,accounts_json,currency,business_date,? FROM pos_fulfillments WHERE id=?',newFid,q.quantityMilli,Date.now(),fid);
     await run(db,'UPDATE pos_fulfillments SET quantity_milli=quantity_milli-?,version=version+1 WHERE id=?',q.quantityMilli,fid);
    }
    const amount=portion(num(line,'subtotal_minor'),0,q.quantityMilli,num(line,'quantity_milli')),tax=portion(num(line,'tax_minor'),0,q.quantityMilli,num(line,'quantity_milli')),discount=portion(num(line,'discount_minor'),0,q.quantityMilli,num(line,'quantity_milli'));
    const billDiscount=portion(num(line,'bill_discount_minor'),0,q.quantityMilli,num(line,'quantity_milli'));
    await run(db,'INSERT INTO pos_lines(id,sale_id,fulfillment_id,product_id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,bill_discount_minor,subtotal_minor,tax_minor,modifiers_json,note) SELECT ?,?,?,product_id,name,?,price_minor,tax_bps,tax_inclusive,?,?,?,?,modifiers_json,note FROM pos_lines WHERE id=?',id(),target,newFid,q.quantityMilli,discount,billDiscount,amount,tax,line.id);
    await run(db,'UPDATE pos_lines SET quantity_milli=quantity_milli-?,subtotal_minor=subtotal_minor-?,tax_minor=tax_minor-?,discount_minor=discount_minor-?,bill_discount_minor=bill_discount_minor-? WHERE id=?',q.quantityMilli,amount,tax,discount,billDiscount,line.id);
   }
   await run(db,'UPDATE pos_sales SET issuer_json=?,discount_requested_by=? WHERE id=?',sale.issuer_json,user,target);await refreshSale(db,target);await refreshSale(db,str(sale,'id'));return {id:target};
  }else if(op==='sale.cancel'){
   for(const f of await all(db,'SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?',sale.id)){
    if(!['draft','queued','cancelled'].includes(str(f,'state')))fail('POS_KITCHEN_ACTIVE','Cancel the kitchen task and record waste before cancelling the bill');
    if(await one(db,'SELECT id FROM pos_lines WHERE fulfillment_id=? AND sale_id<>?',f.id,sale.id))fail('POS_SHARED_KITCHEN','This kitchen item is on a separate bill. Handle the item first');
    await run(db,'DELETE FROM pos_reservations WHERE fulfillment_id=?',f.id);await run(db,"UPDATE pos_fulfillments SET state='cancelled',reserved=false,version=version+1 WHERE id=?",f.id);
   }
   await run(db,"UPDATE pos_sales SET state='cancelled',table_id=NULL WHERE id=?",sale.id);
  }else fail('POS_ACTION_UNKNOWN','Command not found',404);
  await refreshSale(db,str(sale,'id'));return {id:sale.id,version:(await one(db,'SELECT version FROM pos_sales WHERE id=?',sale.id))!.version};
 }
 if(op.startsWith('kitchen.')){
  const p=base.extend({reason:z.string().min(3).max(500).optional()}).parse(input),f=scopeRow(actor,await one(db,'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',p.id));version(f,p);
  await assertPosDayWritable(db,company,str(f,'branch_id'),str(f,'business_date'));
  if(f.kind!=='recipe')fail('POS_KITCHEN_ITEM','This item is not a prepared food item',422);
  let state='';const a=json<PosAccounts>(f.accounts_json);
  if(op==='kitchen.send'&&f.state==='draft'){await reserve(db,f);state='queued';}
  else if(op==='kitchen.start'&&f.state==='queued'){await consume(db,f,user,true);state='preparing';}
  else if(op==='kitchen.ready'&&f.state==='preparing')state='ready';
  else if(op==='kitchen.deliver'&&f.state==='ready'){await posting(db,company,str(f,'branch_id'),a,businessDate(),str(f,'currency'),`deliver:${f.id}`,str(f,'id'),user,pair(a.cogs,a.wip,num(f,'cost_minor')));state='delivered';}
  else if(op==='kitchen.cancel'&&['draft','queued','preparing','ready','delivered'].includes(str(f,'state'))){
   requirePermission(actor,'pos.approve');if(!p.reason)fail('POS_REASON_REQUIRED','Enter the cancellation reason',422);
   if(['preparing','ready','delivered'].includes(str(f,'state'))){const creator=await one(db,'SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1',f.id,user);const starter=await one(db,"SELECT actor_id FROM pos_audit WHERE operation='kitchen.start' AND record_id=? ORDER BY created_at DESC LIMIT 1",f.id);if(creator||starter&&samePerson(actor,starter.actor_id))fail('POS_INDEPENDENT_REVIEW','ผู้อนุมัติอีกคนต้องยกเลิกรายการที่เริ่มทำแล้ว');}
   if(await one(db,"SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli",f.id))fail('POS_REFUND_REQUIRED','The bill is paid. Refund it through a refund request first');
   await posting(db,company,str(f,'branch_id'),a,businessDate(),str(f,'currency'),`waste:${f.id}`,str(f,'id'),user,pair(a.waste,f.state==='delivered'?a.cogs:a.wip,num(f,'cost_minor')));await run(db,'DELETE FROM pos_reservations WHERE fulfillment_id=?',f.id);state='cancelled';
  }else fail('POS_KITCHEN_TRANSITION','Kitchen status changed');
  await run(db,'UPDATE pos_fulfillments SET state=?,reserved=false,version=version+1 WHERE id=?',state,f.id);
  if(state==='queued')await run(db,'UPDATE pos_fulfillments SET reserved=true WHERE id=?',f.id);
  return {id:f.id,state};
 }
 if(op==='payment.create'){
  const p=base.extend({method:z.enum(['cash','transfer','edc','stripe_card','stripe_promptpay']),amountMinor:money.refine(n=>n>0),tenderedMinor:money,reference:z.string().max(150).default(''),payerEmail:z.union([z.email(),z.literal('')]).default('')}).parse(input),sale=await saleRow(db,actor,p.id);version(sale,p);
  await assertPosDayWritable(db,company,str(sale,'branch_id'),str(sale,'business_date'));
  if(process.env.NODE_ENV==='production'&&!['cash','transfer'].includes(p.method)&&process.env.POS_STRIPE_LIVE_ENABLED!=='true')fail('POS_TENDER_COMING_SOON','The pilot accepts cash and verified bank transfers only',403);
  if(!['open','held','payment_pending'].includes(str(sale,'state')))fail('POS_SALE_LOCKED','Bill is not ready for payment');
  const settings=await config(db,company);if(!settings.enabled)fail('POS_DISABLED','New transactions are temporarily paused');await openPeriod(db,company,str(sale,'business_date'));
  if(!await one(db,"SELECT id FROM pos_shifts WHERE id=? AND state='open'",sale.shift_id))fail('POS_SHIFT_CLOSED','Cash session is closed');
  const gross=await one(db,'SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?',sale.id);
  if(num(sale,'discount_minor')>scaledAmount(num(gross!,'n'),num(settings,'cashier_discount_bps'),10000)&&!sale.discount_approved_by)fail('POS_DISCOUNT_APPROVAL','This discount must be approved before payment');
  const reserved=await one(db,"SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','succeeded','exception')",sale.id);
  if(p.amountMinor>num(sale,'total_minor')-num(reserved!,'n')||p.tenderedMinor<p.amountMinor||p.method!=='cash'&&p.tenderedMinor!==p.amountMinor)fail('POS_PAYMENT_AMOUNT','Invalid payment or tendered amount',422);
  if(['transfer','edc'].includes(p.method)&&p.reference.trim().length<2)fail('POS_REFERENCE_REQUIRED','Enter the payment reference',422);
  if(p.method==='stripe_promptpay'&&!p.payerEmail)fail('POS_PROMPTPAY_EMAIL_REQUIRED','กรอกอีเมลผู้จ่ายสำหรับ PromptPay และการคืนเงินก่อนสร้าง QR',422);
  for(const f of await all(db,'SELECT DISTINCT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?',sale.id)){
   if(f.state==='cancelled')fail('POS_ITEM_CANCELLED','Remove cancelled items before taking payment');await reserve(db,f);
  }
  let connection:PosRow|null=null;if(p.method.startsWith('stripe_')){if(sale.currency!=='THB')fail('POS_STRIPE_CURRENCY','This Stripe POS version accepts THB only');connection=await one(db,"SELECT * FROM pos_connections WHERE company_id=? AND state='ready'",company);if(connection?.livemode&&process.env.POS_STRIPE_LIVE_ENABLED!=='true')fail('POS_LIVE_NOT_ENABLED','Complete sandbox testing before accepting live payments');if(!connection?.charges_enabled||!connection.payouts_enabled||!(p.method==='stripe_card'?connection.card_enabled:connection.promptpay_enabled))fail('POS_STRIPE_NOT_READY','The Stripe account is not ready for this payment method');}
  const pid=id();await run(db,"INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,stripe_account,livemode,created_by,created_at,updated_at,payer_email) VALUES(?,?,?,?,?,?,?,'requested',?,?,?,?,?,?)",pid,sale.id,company,p.method,p.amountMinor,p.tenderedMinor,p.reference,connection?.account_id??null,connection?.livemode??false,user,Date.now(),Date.now(),p.payerEmail||null);
  await capturePosMoney(db,sale,'pos_payment',pid,'requested',str(sale,'business_date'),String(p.amountMinor));
  if(connection)await enqueue(db,company,'checkout',pid);
  else if(p.method==='transfer')await run(db,"UPDATE pos_payments SET state='pending',version=version+1,updated_at=? WHERE id=?",Date.now(),pid);
  else await receivePayment(db,(await one(db,'SELECT * FROM pos_payments WHERE id=?',pid))!,user);
  await run(db,"UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?",Date.now(),sale.id);return {id:pid,saleId:sale.id};
 }
 if(op==='payment.verify_transfer'){
  const p=base.extend({bankTransactionId:z.string().min(1),bankVersion:z.number().int().positive()}).parse(input);
  const payment=await one(db,'SELECT * FROM pos_payments WHERE id=? AND company_id=? FOR UPDATE',p.id,company);
  if(!payment)fail('POS_NOT_FOUND','Payment not found',404);version(payment,p);
  if(payment.method!=='transfer'||payment.state!=='pending')fail('POS_TRANSFER_STATE','Only a pending bank transfer can be verified');
  if(samePerson(actor,str(payment,'created_by')))fail('POS_INDEPENDENT_VERIFICATION','A different Finance user must verify the transfer',403);
  const sale=await saleRow(db,actor,str(payment,'sale_id'));
  const accounts=json<PosAccounts>(sale.accounts_json);
  const bank=await one(db,'SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? FOR UPDATE OF t',p.bankTransactionId,company);
  if(!bank||num(bank,'version')!==p.bankVersion||!['unmatched','proposed'].includes(str(bank,'match_status'))||bank.bank_account!==accounts.bank||bank.currency!==sale.currency||num(bank,'amount_minor')!==num(payment,'amount_minor')||str(bank,'external_id').trim()!==str(payment,'reference').trim()||str(bank,'transaction_date')<str(sale,'business_date'))fail('POS_BANK_EVIDENCE_MISMATCH','Match the exact imported bank account, reference, date, currency, and amount');
  await receivePayment(db,payment,user,str(bank,'transaction_date'));
  const settled=await one(db,'SELECT finance_payment_id FROM pos_payments WHERE id=?',payment.id);
  await run(db,"UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?",settled!.finance_payment_id,bank.id);
  return {id:payment.id,saleId:sale.id,state:'succeeded',bankTransactionId:bank.id};
 }
 if(op==='payment.cancel_transfer'){
  const p=base.parse(input),payment=await one(db,'SELECT * FROM pos_payments WHERE id=? AND company_id=? FOR UPDATE',p.id,company);
  if(!payment)fail('POS_NOT_FOUND','Payment not found',404);version(payment,p);
  await saleRow(db,actor,str(payment,'sale_id'));
  if(payment.method!=='transfer'||payment.state!=='pending')fail('POS_TRANSFER_STATE','Only a pending bank transfer can be cancelled');
  await run(db,"UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?",Date.now(),payment.id);
  await run(db,"UPDATE pos_sales SET state='open',version=version+1,updated_at=? WHERE id=? AND NOT EXISTS(SELECT 1 FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception','succeeded'))",Date.now(),payment.sale_id,payment.sale_id);
  return {id:payment.id,saleId:payment.sale_id,state:'cancelled'};
 }
 if(op==='payment.retry'||op==='payment.cancel'){
  const p=base.extend({payerEmail:z.email().optional()}).parse(input),payment=await one(db,'SELECT * FROM pos_payments WHERE id=? FOR UPDATE',p.id);if(!payment)fail('POS_NOT_FOUND','Payment not found',404);await saleRow(db,actor,str(payment,'sale_id'));version(payment,p);
  if(!str(payment,'method').startsWith('stripe_')||['succeeded','cancelled'].includes(str(payment,'state')))fail('POS_PAYMENT_LOCKED','This record cannot change state');
  if(p.payerEmail){
   const checkout=await one(db,"SELECT state FROM pos_jobs WHERE record_id=? AND kind='checkout' FOR UPDATE",payment.id);
   if(op!=='payment.retry'||payment.method!=='stripe_promptpay'||payment.provider_id||payment.payer_email||checkout?.state!=='exception')fail('POS_PAYMENT_LOCKED','ตรวจผลรายการเดิมก่อนแก้ข้อมูลผู้จ่าย');
   await run(db,'UPDATE pos_payments SET payer_email=?,version=version+1,updated_at=? WHERE id=?',p.payerEmail,Date.now(),payment.id);
  }
  if(op==='payment.cancel')await enqueue(db,company,'cancel_checkout',str(payment,'id'));
  else await run(db,"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL WHERE record_id=? AND kind IN ('checkout','check_payment') AND state='exception'",payment.id);
  return {id:payment.id};
 }
 if(op.startsWith('refund.'))return refundCommand(db,actor,op,input);
 if(op==='settlement.refresh'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Reconcile from the company area',403);
  const p=z.object({expectedVersion:z.number().int().positive(),accountingDate:z.iso.date(),reason:z.string().trim().min(3).max(1000)}).parse(input);
  const connection=await one(db,"SELECT * FROM pos_connections WHERE company_id=? AND state='ready' FOR UPDATE",company);
  if(!connection)fail('POS_CONNECTION_MISSING','Connect the company Stripe account first',422);version(connection,p);
  await openPeriod(db,company,p.accountingDate);
  const syncId=id();await enqueue(db,company,'balance_sync',syncId,{accountId:connection.account_id,livemode:connection.livemode,accountingDate:p.accountingDate,reason:p.reason,userId:user,assignmentId:actor.assignmentId});return {id:syncId};
 }
 if(op==='settlement.map_external'){
  requirePermission(actor,'finance.approve');if(actor.branchId)fail('POS_COMPANY_SETTINGS','Reconcile from the company area',403);
  const p=base.extend({journalId:z.string(),reason:z.string().trim().min(3).max(1000)}).parse(input);
  const item=await one(db,"SELECT * FROM pos_settlements WHERE id=? AND company_id=? AND state='unmapped' FOR UPDATE",p.id,company);if(!item)fail('POS_NOT_FOUND','Unmatched amount not found',404);version(item,p);
  const settings=await config(db,company),a=json<PosAccounts>(settings.accounts_json),details=json<{net:number;manualPayout?:boolean;status?:string}>(item.details_json),net=details.net;
  const journal=await one(db,"SELECT j.id,SUM(l.debit_minor-l.credit_minor) net FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id=? AND j.company_id=? AND j.currency=? AND j.status='posted' AND j.source_type='manual' AND l.account_id=? GROUP BY j.id",p.journalId,company,item.currency,a.stripe);
  if(!journal||num(journal,'net')!==net||await one(db,'SELECT id FROM pos_settlements WHERE journal_id=?',p.journalId))fail('POS_EXTERNAL_JOURNAL_MISMATCH','Select an approved journal with the matching net amount in the Stripe receivables account, and one that has not already been matched');
  const manualPayout=item.kind==='payout'&&details.manualPayout===true;
  const bankNet=manualPayout?await one(db,'SELECT COALESCE(SUM(debit_minor-credit_minor),0) net FROM finance_journal_lines WHERE journal_id=? AND account_id=?',p.journalId,a.bank):null;
  if(manualPayout&&(details.status!=='paid'||net!==-num(item,'amount_minor')||num(bankNet!,'net')!==num(item,'amount_minor')))fail('POS_EXTERNAL_JOURNAL_MISMATCH','The approved manual payout journal must debit the configured bank account for the verified net payout amount');
  await run(db,"UPDATE pos_settlements SET state=?,journal_id=?,version=version+1 WHERE id=?",manualPayout?'paid':'mapped',p.journalId,item.id);return {id:item.id};
 }
 if(op==='settlement.match'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Reconcile from the company area',403);
  requirePermission(actor,'finance.approve');
  const p=base.extend({bankTransactionId:z.string(),bankVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)}).parse(input);
  const settlement=await one(db,"SELECT * FROM pos_settlements WHERE id=? AND company_id=? AND kind='payout' AND state='paid' FOR UPDATE",p.id,company);
  if(!settlement||settlement.bank_transaction_id)fail('POS_PAYOUT_MATCHED','Unreconciled payout not found');version(settlement,p);
  const bank=await one(db,'SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? FOR UPDATE OF t',p.bankTransactionId,company);
  if(!bank||bank.version!==p.bankVersion||!['unmatched','proposed'].includes(str(bank,'match_status'))||bank.currency!==settlement.currency||bank.amount_minor!==settlement.amount_minor)fail('POS_BANK_MISMATCH','Bank amount, currency, or status does not match the payout');
  const bankNet=await one(db,'SELECT COALESCE(SUM(debit_minor-credit_minor),0) net FROM finance_journal_lines WHERE journal_id=? AND account_id=?',settlement.journal_id,bank.bank_account);
  if(num(bankNet!,'net')!==num(settlement,'amount_minor'))fail('POS_BANK_ACCOUNT','Bank account net amount must match the account receiving the payout');
  await run(db,"UPDATE finance_bank_transactions SET match_status='matched',matched_type='pos_payout',matched_id=?,version=version+1 WHERE id=?",settlement.id,bank.id);
  await run(db,'UPDATE pos_settlements SET bank_transaction_id=?,version=version+1 WHERE id=?',bank.id,settlement.id);return {id:settlement.id};
 }
 if(op==='exception.attach_provider'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Review records from the company area',403);
  const p=z.object({id:z.string(),providerId:z.string().regex(/^(re|pyr|cs)_[a-zA-Z0-9_]+$/),accountingDate:z.iso.date(),reason:z.string().trim().min(3).max(1000)}).parse(input);
  const job=await one(db,"SELECT * FROM pos_jobs WHERE id=? AND company_id=? AND state='exception' FOR UPDATE",p.id,company);if(!job)fail('POS_NOT_FOUND','Exception record not found',404);await openPeriod(db,company,p.accountingDate);
  if(job.kind==='refund'&&(p.providerId.startsWith('re_')||p.providerId.startsWith('pyr_'))){
   const changed=await one(db,"UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id",p.providerId,job.record_id);if(!changed)fail('POS_PROVIDER_ALREADY_BOUND','Record is already linked to Stripe');
  }else if(job.kind==='checkout'&&p.providerId.startsWith('cs_')){
   const changed=await one(db,"UPDATE pos_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id",p.providerId,job.record_id);if(!changed)fail('POS_PROVIDER_ALREADY_BOUND','Record is already linked to Stripe');
   await enqueue(db,company,'check_payment',str(job,'record_id'),{accountingDate:p.accountingDate});
  }else fail('POS_PROVIDER_TYPE','Reference does not match the record type',422);
  await run(db,"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,payload_json=? WHERE id=?",JSON.stringify({...json<Record<string,unknown>>(job.payload_json),accountingDate:p.accountingDate,resolutionReason:p.reason,resolvedBy:user}),job.id);return {id:job.id};
 }
 if(op==='exception.retry'){
  if(actor.branchId)fail('POS_COMPANY_SETTINGS','Edit Stripe records from the company area',403);
  requirePermission(actor,'pos.approve');const p=z.object({id:z.string(),accountingDate:z.iso.date(),reason:z.string().trim().min(3).max(1000),refundInstructionsEmail:z.email().max(254).optional(),expectedRefundVersion:z.number().int().positive().optional()}).parse(input),job=await one(db,"SELECT * FROM pos_jobs WHERE id=? AND company_id=? AND state='exception' FOR UPDATE",p.id,company);if(!job)fail('POS_NOT_FOUND','Exception record not found',404);
  await openPeriod(db,company,p.accountingDate);
  const payload=json<Record<string,unknown>>(job.payload_json);
  if(p.refundInstructionsEmail){
   requirePermission(actor,'finance.approve');if(actor.departmentId)fail('POS_COMPANY_SETTINGS','Correct refund instructions from the company area',403);
   const refund=await one(db,"SELECT r.id,r.version,r.state refund_state,rp.state payout_state,rp.provider_id,p.method,p.payer_email FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND r.company_id=? AND p.company_id=? FOR UPDATE OF rp,r",job.record_id,company,company);
   if(job.kind!=='refund'||job.last_error!=='POS_PROVIDER_REFUND_EMAIL_RESTRICTED'||!refund||refund.method!=='stripe_promptpay'||refund.provider_id||refund.payout_state!=='requested'||refund.refund_state!=='processing')fail('POS_REFUND_EMAIL_LOCKED','Only an approved PromptPay refund with a rejected instruction email and no provider refund can be corrected');
   if(p.expectedRefundVersion!==num(refund,'version'))fail('POS_VERSION_CONFLICT','Reload the refund before correcting its instruction email',409);
   const attempts=z.array(z.object({email:z.email().max(254)})).max(10).parse(payload.refundInstructionsAttempts??[]);
   if(attempts.length>=10)fail('POS_PROVIDER_LOOKUP_REQUIRED','Review the refund with Stripe before another email correction');
   if(p.refundInstructionsEmail===(attempts.at(-1)?.email??refund.payer_email))fail('POS_REFUND_EMAIL_UNCHANGED','The instruction email is unchanged. Retry without a replacement after Stripe permits that recipient');
   payload.refundInstructionsAttempts=[...attempts,{email:p.refundInstructionsEmail}];
   await run(db,'UPDATE pos_refunds SET version=version+1 WHERE id=?',refund.id);
  }
  let providerAttemptStartedAt=typeof payload.providerAttemptStartedAt==='number'?payload.providerAttemptStartedAt:undefined;
  if(job.kind==='refund'){const failed=await one(db,"UPDATE pos_refund_payments SET provider_id=NULL,provider_attempt=provider_attempt+1,state='requested' WHERE id=? AND state='failed' RETURNING id",job.record_id);if(failed)providerAttemptStartedAt=Date.now();}
  await run(db,"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL,payload_json=? WHERE id=?",JSON.stringify({...payload,accountingDate:p.accountingDate,providerAttemptStartedAt,resolutionReason:p.reason,resolvedBy:user}),job.id);return {id:job.id};
 }
 fail('POS_ACTION_UNKNOWN','POS command not found',404);
}
async function refundCommand(db:SqlDatabase,actor:WorkContext,op:string,input:unknown):Promise<Record<string,unknown>>{
 const user=actor.userId;
 if(op==='refund.request'){
  const p=base.extend({reason:z.string().trim().min(3).max(1000),lines:z.array(z.object({lineId:z.string(),quantityMilli:qty,restock:z.boolean()})).max(100).default([]),amountMinor:money.optional(),payoutShiftId:z.string().optional()}).parse(input),sale=await saleRow(db,actor,p.id);version(sale,p);
  await assertPosDayWritable(db,str(sale,'company_id'),str(sale,'branch_id'),businessDate());
  if(await one(db,"SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'",sale.company_id,sale.branch_id,sale.business_date))fail('POS_DAY_REVIEW_PENDING','Finance must confirm the original day close before a refund can change its source records');
  if(await one(db,'SELECT source_key FROM pms_sources WHERE source_key=?',`pos:${sale.id}:${sale.id}`))fail('POS_ROOM_CHARGE_REVERSAL','Reverse this charge on the hotel folio; it has no POS payment to refund');
  if(!['completed','payment_pending'].includes(str(sale,'state')))fail('POS_REFUND_SALE','Bill has no payment');
  if(new Set(p.lines.map(l=>l.lineId)).size!==p.lines.length)fail('POS_DUPLICATE_LINE','Duplicate refund item',422);
  const advance=sale.state!=='completed',snapshots:Array<Record<string,unknown>>=[];let amount=0,tax=0;
  if(advance){if(!p.amountMinor||p.lines.length)fail('POS_ADVANCE_REFUND','Enter an advance payment refund amount without selecting items',422);amount=p.amountMinor;}
  else for(const part of p.lines){
   const l=await one(db,'SELECT * FROM pos_lines WHERE id=? AND sale_id=?',part.lineId,sale.id);if(!l||num(l,'returned_milli')+part.quantityMilli>num(l,'quantity_milli'))fail('POS_REFUND_QUANTITY','Refund quantity exceeds the original item',422);
   const subtotal=portion(num(l,'subtotal_minor'),num(l,'returned_milli'),part.quantityMilli,num(l,'quantity_milli')),vat=portion(num(l,'tax_minor'),num(l,'returned_milli'),part.quantityMilli,num(l,'quantity_milli'));
   amount+=subtotal+vat;tax+=vat;snapshots.push({...part,subtotalMinor:subtotal,taxMinor:vat,previousReturned:num(l,'returned_milli')});
  }
  if(amount<=0)fail('POS_REFUND_EMPTY','Select an item with a refund amount',422);
  const rid=id();await run(db,'INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,payout_shift_id,kind,reason,lines_json,amount_minor,requested_by,created_at,accounting_date,tax_minor) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',rid,sale.id,sale.company_id,sale.branch_id,p.payoutShiftId??null,advance?'advance':'sale',p.reason,JSON.stringify(snapshots),amount,user,Date.now(),businessDate(),tax);return {id:rid};
 }
 const p=base.extend({reason:z.string().trim().min(3).max(1000).optional()}).parse(input),refund=scopeRow(actor,await one(db,'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE',p.id));version(refund,p);
 if(refund.state!=='requested'||samePerson(actor,refund.requested_by))fail('POS_INDEPENDENT_REVIEW','A different manager must approve the pending request');
 if(op==='refund.reject'){if(!p.reason)fail('POS_REASON_REQUIRED','Enter a reason',422);await run(db,"UPDATE pos_refunds SET state='rejected',approved_by=?,version=version+1 WHERE id=?",user,refund.id);return {id:refund.id};}
 if(op!=='refund.approve')fail('POS_ACTION_UNKNOWN','Refund command not found',404);
 const sale=await saleRow(db,actor,str(refund,'sale_id')),a=json<PosAccounts>(sale.accounts_json),date=str(refund,'accounting_date');
 const payments=await all(db,"SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at DESC,id FOR UPDATE",sale.id);
 if(payments.reduce((n,r)=>n+num(r,'amount_minor')-num(r,'refunded_minor'),0)<num(refund,'amount_minor'))fail('POS_REFUND_EXCEEDED','Refund exceeds the remaining payment amount');
 if(refund.kind==='advance'&&sale.state==='completed')fail('POS_REFUND_CHANGED','Bill is closed. Request a refund from the item');
 if(refund.kind==='sale'&&sale.state!=='completed')fail('POS_REFUND_CHANGED','Bill status changed');
 for(const part of json<Array<{lineId:string;quantityMilli:number;restock:boolean;subtotalMinor:number;taxMinor:number;previousReturned:number}>>(refund.lines_json)){
  const line=(await one(db,'SELECT * FROM pos_lines WHERE id=? AND sale_id=? FOR UPDATE',part.lineId,sale.id))!;
  if(!line||num(line,'returned_milli')!==part.previousReturned||part.quantityMilli+num(line,'returned_milli')>num(line,'quantity_milli'))fail('POS_REFUND_CHANGED','This item has already been refunded. Create a new request');
  const f=(await one(db,'SELECT * FROM pos_fulfillments WHERE id=? FOR UPDATE',line.fulfillment_id))!;let restoredCost=0;
  if(part.restock){
   if(f.kind!=='stock'||f.state!=='delivered')fail('POS_RETURN_NOT_STOCK','Only delivered items can be returned to inventory',422);
   const returned=await one(db,"SELECT COALESCE(SUM(quantity_milli),0) qty,COALESCE(SUM(value_minor),0) cost FROM pos_stock_moves WHERE fulfillment_id=? AND kind='return'",f.id);
   restoredCost=portion(num(f,'cost_minor'),num(returned!,'qty'),part.quantityMilli,num(f,'quantity_milli'));
   await run(db,'SELECT product_id FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE',f.warehouse_id,f.product_id);
   await run(db,'UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?',part.quantityMilli,restoredCost,f.warehouse_id,f.product_id);
   await run(db,'INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',id(),sale.company_id,f.warehouse_id,f.product_id,f.id,refund.id,sale.number,'return',part.quantityMilli,restoredCost,date,Date.now());
   await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,date,str(sale,'currency'),`return:${refund.id}:${line.id}`,str(refund,'id'),user,pair(a.inventory,a.cogs,restoredCost));
  }
  await run(db,'UPDATE pos_lines SET returned_milli=returned_milli+?,refunded_minor=refunded_minor+?,refunded_tax_minor=refunded_tax_minor+?,returned_cost_minor=returned_cost_minor+? WHERE id=?',part.quantityMilli,part.subtotalMinor+part.taxMinor,part.taxMinor,restoredCost,line.id);
 }
 await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,date,str(sale,'currency'),`refund:${refund.id}`,str(refund,'id'),user,[{account:refund.kind==='advance'?a.advance:a.revenue,debit:num(refund,'amount_minor')-num(refund,'tax_minor'),credit:0},{account:a.outputTax,debit:num(refund,'tax_minor'),credit:0},{account:a.refund,debit:0,credit:num(refund,'amount_minor')}]);
 if(refund.kind==='sale')await capturePosReversal(db,sale,'pos_refund',str(refund,'id'),'total',date,String(refund.amount_minor),{sourceType:'pos_sale',sourceId:str(sale,'id'),amountKind:'total'});
 let remaining=num(refund,'amount_minor');
 for(const payment of payments){const amount=Math.min(remaining,num(payment,'amount_minor')-num(payment,'refunded_minor'));if(!amount)continue;
  const manualPromptPay=payment.method==='stripe_promptpay'&&!payment.payer_email;
  const rpid=id();await run(db,'INSERT INTO pos_refund_payments(id,refund_id,payment_id,amount_minor,manual_method) VALUES(?,?,?,?,?)',rpid,refund.id,payment.id,amount,manualPromptPay?(refund.payout_shift_id?'cash':'transfer'):null);
  await run(db,'UPDATE pos_payments SET refunded_minor=refunded_minor+?,version=version+1 WHERE id=?',amount,payment.id);
  if(str(payment,'method').startsWith('stripe_')&&!manualPromptPay)await enqueue(db,str(sale,'company_id'),'refund',rpid);
  else {
   if(payment.method==='cash'||(manualPromptPay&&refund.payout_shift_id)){
    const shift=await one(db,"SELECT * FROM pos_shifts WHERE id=? AND company_id=? AND branch_id=? AND state='open'",refund.payout_shift_id,actor.companyId,sale.branch_id);if(!shift)fail('POS_REFUND_SHIFT_REQUIRED','Specify an open cash session for cash refund payout');
   }
   // Manual methods require the approver to confirm money was actually returned.
   if(!p.reason)fail('POS_REFUND_PROOF_REQUIRED','Enter proof/reference for the cash, transfer, or EDC refund',422);
   await completeRefundPayment(db,rpid,user);
  }
  remaining-=amount;
 }
 await run(db,"UPDATE pos_refunds SET approved_by=?,state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?",user,refund.id,refund.id);
 await releaseUnpaidReservations(db,str(sale,'id'));
 return {id:refund.id};
}
export async function completeRefundPayment(db:SqlDatabase,refundPaymentId:string,actorId:string,date?:string){
 const rp=await one(db,'SELECT * FROM pos_refund_payments WHERE id=? FOR UPDATE',refundPaymentId);if(!rp||rp.state==='succeeded')return;
 const refund=(await one(db,'SELECT * FROM pos_refunds WHERE id=? FOR UPDATE',rp.refund_id))!,p=(await one(db,'SELECT * FROM pos_payments WHERE id=?',rp.payment_id))!,sale=(await one(db,'SELECT * FROM pos_sales WHERE id=?',refund.sale_id))!,a=json<PosAccounts>(sale.accounts_json);
 const account=rp.manual_method==='cash'?a.cash:rp.manual_method==='transfer'?a.bank:p.method==='cash'?a.cash:p.method==='transfer'?a.bank:p.method==='edc'?a.edc:a.stripe;
 const journal=await posting(db,str(sale,'company_id'),str(sale,'branch_id'),a,date??str(refund,'accounting_date'),str(sale,'currency'),`refund-payment:${rp.id}`,str(refund,'id'),actorId,pair(a.refund,account,num(rp,'amount_minor')));
 await run(db,"INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'refund',?,?,?,?,0,?,'settled',?,?,?,?,?)",id(),sale.company_id,sale.party_id,`POS-REF-${rp.id}`,date??refund.accounting_date,sale.currency,rp.amount_minor,account,journal,actorId,actorId,Date.now(),Date.now());
 await capturePosReversal(db,sale,'pos_refund_payment',str(rp,'id'),'settled',date??str(refund,'accounting_date'),String(rp.amount_minor),{sourceType:'pos_payment',sourceId:str(p,'id'),amountKind:'received'});
 await run(db,"UPDATE pos_refund_payments SET state='succeeded' WHERE id=?",rp.id);
 await run(db,"UPDATE pos_refunds SET state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?",refund.id,refund.id);
}
export async function releaseUnpaidReservations(db:SqlDatabase,saleId:string){
 if(await one(db,"SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor)",saleId))return;
 if(await one(db,"SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'",saleId))return;
 await run(db,"DELETE FROM pos_reservations WHERE fulfillment_id IN (SELECT f.id FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=? AND f.state='draft')",saleId);
 await run(db,"UPDATE pos_fulfillments SET reserved=false WHERE state='draft' AND id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=?)",saleId);
 await run(db,"UPDATE pos_sales SET state='open',version=version+1 WHERE id=? AND state='payment_pending'",saleId);
}
export const posPermissions=(actor:WorkContext)=>Object.fromEntries(['pos.read','pos.sell','pos.kitchen','pos.approve','pos.configure','pos.payments.configure','pos.reports','finance.read','finance.write','finance.approve','pms.approve'].map(p=>[p,hasPermission(actor,p)]));
