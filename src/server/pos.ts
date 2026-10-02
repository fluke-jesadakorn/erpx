import { readListPage } from "./list-page";
import 'server-only';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import { requirePermission } from './iam-permissions';
import { hasPermission } from '@/lib/permissions';
import { all, one, scopeRow, str, posPermissions, type PosRow } from './pos-core';
import type { SqlDatabase } from './runtime/types';
import { ApiError } from './http';
import { businessDate } from '@/lib/pos';
import { recipeQuantities } from '@/lib/pos';
import { posCustomers,type PosReceiptSnapshot } from './pos-daily';
export async function posWorkspace(actor:WorkContext,saleId?:string,historyQuery='',section?:string,cursor?:string,frontMode=false){
 requirePermission(actor,'pos.read');if(actor.departmentId)throw new Error('POS_BRANCH_REQUIRED');
 const {write:db}=await holdingResources(actor.holdingId),company=actor.companyId,branch=actor.branchId;
 const registers=await all(db,'SELECT r.*,w.name warehouse_name FROM pos_registers r JOIN commerce_warehouses w ON w.id=r.warehouse_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY r.name',company,branch,branch);
 const sale=saleId?scopeRow(actor,await one(db,'SELECT * FROM pos_sales WHERE id=?',saleId)):null;
 const wants=(...tabs:string[])=>!section||tabs.includes(section);
 const settings=await one(db,'SELECT * FROM pos_settings WHERE company_id=?',company);
 const frontProductCount=frontMode?await one(db,"SELECT COUNT(DISTINCT p.id)::int count FROM commerce_products p JOIN pos_catalog c ON c.product_id=p.id AND c.enabled=true JOIN pos_register_catalog rc ON rc.product_id=p.id AND rc.enabled=true JOIN pos_registers r ON r.id=rc.register_id WHERE p.company_id=? AND p.status='active' AND r.active=true AND (?::text IS NULL OR r.branch_id=?)",company,branch,branch):null;
 const frontHotels=frontMode?await all(db,`SELECT r.id register_id,p.name hotel_name,p.state hotel_state,p.business_date,
   (p.config_json::jsonb->>'approvedBy' IS NOT NULL) finance_approved
   FROM pos_registers r LEFT JOIN pms_properties p ON p.company_id=r.company_id AND p.branch_id=r.branch_id
   WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND r.active=true`,company,branch,branch):[];
 const frontBlockedRegisters=frontHotels.filter(row=>row.hotel_state&&(row.hotel_state!=='open'||row.business_date!==businessDate()));
 const report=!frontMode&&hasPermission(actor,'pos.reports'),configure=!frontMode&&hasPermission(actor,'pos.configure')&&(wants('settings')||!settings),connect=!frontMode&&hasPermission(actor,'pos.payments.configure')&&wants('settings');
 const salesPage=section==='history'?await readListPage<import('./pos-core').PosRow & {id:string;page_key:string}>(db,{
   sql:"SELECT s.*,r.name register_name,s.created_at::text page_key FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?)",
   args:[company,branch,branch,historyQuery.slice(0,100),`%${historyQuery.slice(0,100)}%`],cursor,descending:true,scope:[company,branch,'pos-history',historyQuery],
 }):null;
 const [commerce,commerceCurrency,products,warehouses,shifts,tables,sales,kitchen,refunds,cashMovements,stock,bankAccounts]=await Promise.all([
  configure?one(db,'SELECT * FROM commerce_settings WHERE company_id=?',company):Promise.resolve(null),
  one(db,'SELECT currency FROM commerce_settings WHERE company_id=?',company),
  wants('sale','settings') ? all(db,`SELECT p.*,c.barcode,c.category,COALESCE(c.kind,'stock') kind,COALESCE(c.station,'Kitchen') station,COALESCE(c.recipe_json,'[]') recipe_json,COALESCE(c.modifiers_json,'[]') modifiers_json,COALESCE(c.tax_inclusive,false) tax_inclusive,COALESCE(c.version,0) catalog_version,c.enabled,c.image_id,
   COALESCE((SELECT json_agg(rc.register_id ORDER BY rc.register_id)::text FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=p.id AND rc.enabled=true AND r.active=true AND (?::text IS NULL OR r.branch_id=?)),'[]') register_ids_json
   FROM commerce_products p LEFT JOIN pos_catalog c ON c.product_id=p.id AND c.company_id=p.company_id WHERE p.company_id=? AND p.status='active'
   AND (?=true OR c.enabled=true AND EXISTS(SELECT 1 FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=p.id AND rc.enabled=true AND r.active=true AND (?::text IS NULL OR r.branch_id=?))) ORDER BY p.name`,branch,branch,company,section==='settings',branch,branch) : Promise.resolve([]),
  wants('sale','settings') ? all(db,"SELECT * FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",company,branch,branch) : Promise.resolve([]),
  wants('sale','history','shifts') ? all(db,`SELECT s.*,r.name register_name,
   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)
   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)
   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor
   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100`,company,branch,branch) : Promise.resolve([]),
  wants('sale','settings') ? all(db,'SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name',company,branch,branch) : Promise.resolve([]),
  salesPage ? Promise.resolve(salesPage.items) : wants('sale') ? all(db,"SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150",company,branch,branch,historyQuery.slice(0,100),`%${historyQuery.slice(0,100)}%`,section==='sale'?1:0) : Promise.resolve([]),
  wants('kitchen') ? all(db,"SELECT f.*,p.name product_name,(SELECT l.modifiers_json FROM pos_lines l WHERE l.fulfillment_id=f.id LIMIT 1) modifiers_json,(SELECT string_agg(DISTINCT s.number, ', ') FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=f.id) sale_numbers FROM pos_fulfillments f JOIN commerce_products p ON p.id=f.product_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND f.kind='recipe' AND f.state NOT IN ('delivered','cancelled') ORDER BY f.created_at",company,branch,branch) : Promise.resolve([]),
  wants('history') || (wants('sale') && sale) ? all(db,'SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100',company,branch,branch,wants('history')?null:sale?.id,wants('history')?null:sale?.id) : Promise.resolve([]),
  wants('shifts') ? all(db,'SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100',company,branch,branch) : Promise.resolve([]),
  wants('sale','settings') ? all(db,'SELECT s.warehouse_id,s.product_id,s.quantity_milli,COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status<>\'released\') THEN 1 ELSE 0 END quality_held FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)',company,branch,branch) : Promise.resolve([]),
  configure ? all(db,"SELECT id,code,name,chart_id,currency,control_kind FROM finance_accounts WHERE company_id=? AND category='asset' AND control_kind='bank' AND status='active' ORDER BY code",company) : Promise.resolve([])
 ]);
 const lines=sale?await all(db,'SELECT l.*,f.state fulfillment_state,f.version fulfillment_version,f.kind,f.cost_minor fulfillment_cost FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id',sale.id):[];
 const payments=sale?await all(db,'SELECT * FROM pos_payments WHERE sale_id=? ORDER BY created_at',sale.id):[];
 const pendingTransfer=hasPermission(actor,'finance.approve')?payments.find(p=>p.method==='transfer'&&p.state==='pending'):null;
 const transferEvidence=pendingTransfer&&sale?await all(db,`SELECT t.id,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.version
  FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
  WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.amount_minor=?
  AND t.external_id=? AND t.transaction_date>=? AND t.match_status IN ('unmatched','proposed')
  ORDER BY t.transaction_date DESC,t.id LIMIT 10`,company,
  JSON.parse(str(sale,'accounts_json')).bank,sale.currency,pendingTransfer.amount_minor,
  pendingTransfer.reference,sale.business_date):[];
 const roomCharge=sale?await one(db,"SELECT ps.property_id,ps.record_id folio_entry_id,p.version property_version,r.reversal_entry_id,r.reason reversal_reason,r.reversed_at FROM pms_sources ps JOIN pms_properties p ON p.id=ps.property_id LEFT JOIN pos_room_charge_reversals r ON r.sale_id=? WHERE ps.source_key=? AND p.company_id=? AND p.branch_id=?",sale.id,`pos:${sale.id}:${sale.id}`,company,sale.branch_id):null;
 const roomChargeEnabled=process.env.NODE_ENV!=='production'||process.env.ERPX_POS_ROOM_CHARGE_PILOT_ENABLED==='true';
 const tenderConnection=await one(db,"SELECT * FROM pos_connections WHERE company_id=? AND state='ready'",company);
 const providerAllowed=process.env.NODE_ENV!=='production'||process.env.POS_STRIPE_LIVE_ENABLED==='true';
 const availableTenderMethods=['cash','transfer',...(providerAllowed?['edc']:[]),...(providerAllowed&&tenderConnection?.charges_enabled&&tenderConnection.payouts_enabled&&(!tenderConnection.livemode||process.env.POS_STRIPE_LIVE_ENABLED==='true')?[...(tenderConnection.card_enabled?['stripe_card']:[]),...(tenderConnection.promptpay_enabled?['stripe_promptpay']:[])]:[])];
 const roomChargeStays=roomChargeEnabled&&sale&&['open','held'].includes(str(sale,'state'))&&hasPermission(actor,'pos.sell')
  ?await all(db,`SELECT p.id property_id,p.version property_version,s.id stay_id,
    r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name
    FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'
    JOIN pms_records f ON f.id=s.data_json::jsonb->>'folioId' AND f.property_id=p.id AND f.kind='folio'
    JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'
    JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'
    WHERE p.company_id=? AND p.branch_id=? AND p.state='open'
    AND p.config_json::jsonb->>'currency'=? AND s.data_json::jsonb->>'state'='in_house'
    AND f.data_json::jsonb->>'state'='open' AND COALESCE(f.data_json::jsonb->>'invoiceId','')=''
    ORDER BY r.data_json::jsonb->>'number',g.data_json::jsonb->>'name' LIMIT 100`,company,sale.branch_id,sale.currency)
  :[];
 const jobs=!branch&&(connect||hasPermission(actor,'pos.approve'))?await all(db,"SELECT id,kind,record_id,state,last_error,payload_json,created_at,(SELECT r.version FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND r.company_id=pos_jobs.company_id AND pos_jobs.kind='refund') refund_version FROM pos_jobs WHERE company_id=? AND (state='exception' OR kind IN ('connect','connect_refresh')) ORDER BY created_at DESC LIMIT 50",company):[];
 if(!connect)for(const job of jobs)delete job.payload_json;
 if(!report){for(const f of kitchen)delete f.cost_minor;for(const l of lines)delete l.fulfillment_cost;}
 const reports=report&&wants('reports')?await all(db,`WITH allocated AS (
 SELECT l.*,f.state fulfillment_state,
 ROUND(f.cost_minor::numeric*SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)/f.quantity_milli)
 -ROUND(f.cost_minor::numeric*(SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)-l.quantity_milli)/f.quantity_milli) cost_share
 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE f.company_id=?
 ), totals AS (SELECT sale_id,SUM(refunded_minor) refunds_minor,SUM(refunded_tax_minor) refunded_tax_minor,
 SUM(CASE WHEN fulfillment_state='delivered' THEN cost_share ELSE 0 END-returned_cost_minor) cost_minor FROM allocated GROUP BY sale_id)
 SELECT s.business_date,s.branch_id,s.register_id,s.shift_id,s.currency,COUNT(*) bills,SUM(s.subtotal_minor) subtotal_minor,SUM(s.tax_minor) tax_minor,
 SUM(s.discount_minor) discount_minor,SUM(s.total_minor) total_minor,SUM(t.refunds_minor) refunds_minor,
 SUM(t.cost_minor) cost_minor,SUM(s.subtotal_minor-t.refunds_minor+t.refunded_tax_minor-t.cost_minor) gross_profit_minor
 FROM pos_sales s JOIN totals t ON t.sale_id=s.id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.state='completed' AND NOT EXISTS(SELECT 1 FROM pms_sources ps WHERE ps.source_key='pos:'||s.id||':'||s.id)
 GROUP BY s.business_date,s.branch_id,s.register_id,s.shift_id,s.currency ORDER BY s.business_date DESC LIMIT 90`,company,company,branch,branch):[];
 const paymentMix=report&&wants('reports')?await all(db,`SELECT s.business_date,s.register_id,s.shift_id,s.currency,p.method,SUM(p.amount_minor) received_minor,
 SUM(COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp WHERE rp.payment_id=p.id AND rp.state='succeeded'),0)) refunded_minor
 FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND p.state='succeeded'
 GROUP BY s.business_date,s.register_id,s.shift_id,s.currency,p.method ORDER BY s.business_date DESC LIMIT 300`,company,branch,branch):[];

 const frontPermissionNames=['pos.read','pos.sell','pos.kitchen','pos.approve'] as const;
 const permissions=frontMode
  ? Object.fromEntries(frontPermissionNames.map((name)=>[name,hasPermission(actor,name)]))
  : posPermissions(actor);
 const frontRow=(row:PosRow|null):PosRow|null=>row?Object.fromEntries(Object.entries(row).filter(([key])=>![
  'accounts_json','chart_id','account_id','bank_account','journal_id','finance_payment_id','invoice_id','debit_account_id','credit_account_id','tax_account_id','provider_id','provider_customer_id','provider_payment_intent','provider_received_at','secret_json','payload_json','recipe_json','cost_minor','cost_price','unit_cost_minor','fulfillment_cost','margin_minor','gross_profit_minor','source_key','external_id','bank_transaction_id'
 ].includes(key))) as PosRow:null;
 const frontRows=(rows:PosRow[])=>frontMode?rows.map((row)=>frontRow(row) as PosRow):rows;
 const frontSettings=settings?frontRow({enabled:settings.enabled,currency:settings.currency,timezone:settings.timezone}):null;
 for(const product of products){
  const availability:Record<string,number>={};
  for(const register of registers){
   if(!JSON.parse(str(product,'register_ids_json')).includes(register.id))continue;
   const parts=product.kind==='recipe'?recipeQuantities(JSON.parse(str(product,'recipe_json')),1000):product.kind==='stock'?[{productId:str(product,'id'),quantityMilli:1000}]:[];
   availability[str(register,'id')]=parts.length?Math.max(0,Math.floor(Math.min(...parts.map(part=>{const row=stock.find(s=>s.product_id===part.productId&&s.warehouse_id===register.warehouse_id);return row&&!row.quality_held?(Number(row.quantity_milli)-Number(row.reserved_milli))/part.quantityMilli:0;}))*1000)):999999999;
  }
  product.availability_json=JSON.stringify(availability);
 }
 const receiptRow=sale?await one(db,'SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=? AND company_id=?',sale.id,company):null;
 const receipt:PosReceiptSnapshot|null=receiptRow?JSON.parse(str(receiptRow,'snapshot_json')):null;
 if(frontMode&&receipt)receipt.payments=receipt.payments.map(p=>{const {finance_payment_id:_finance,...rest}=p;void _finance;return rest;});
 const frontData={currency:settings?.currency ? String(settings.currency) : commerce?.currency ? String(commerce.currency) : commerceCurrency?.currency ? String(commerceCurrency.currency) : null,salesPage:frontMode?null:salesPage,permissions,userId:actor.userId,settings:frontMode?frontSettings:settings,commerce:frontMode?null:configure?commerce:null,availableTenderMethods,roomChargeEnabled,registers:frontRows(registers),products:frontRows(products),warehouses:frontRows(warehouses),shifts:frontRows(shifts),tables:frontRows(tables),sales:frontRows(sales),sale:frontRow(sale),lines:frontRows(lines),payments:payments.map((payment):PosRow=>({...(frontMode?frontRow(payment):payment),provider_payment_confirmed:Boolean(payment.provider_received_at)})),transferEvidence:frontMode?[]:transferEvidence,roomCharge:frontMode&&roomCharge?{property_id:roomCharge.property_id,folio_entry_id:roomCharge.folio_entry_id}:roomCharge,roomChargeStays,kitchen:frontRows(kitchen),refunds:frontRows(refunds),cashMovements:frontRows(cashMovements),stock:frontRows(stock),jobs:frontMode?[]:jobs,reports:frontMode?[]:reports,paymentMix:frontMode?[]:paymentMix,
  companyName: (await one(db,'SELECT display_name FROM organization_nodes WHERE id=?',company))?.display_name??company,
  bankTransactions:frontMode?[]:report&&!branch&&wants('reports')?await all(db,"SELECT id,transaction_date,description,amount_minor,currency,version FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') ORDER BY transaction_date DESC LIMIT 200",company):[],
  customers:wants('sale')?await posCustomers(db,actor):[],receipt,companyId:company,branchId:branch,
  branches:report?await all(db,"SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND (?::text IS NULL OR id=?) ORDER BY display_name",company,branch,branch):[],
  dayCloses:report&&wants('reports')?await all(db,'SELECT id,company_id,branch_id,business_date,currency,state,prepared_by,confirmed_by,version,snapshot_json::text FROM pos_day_closes WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY business_date DESC LIMIT 90',company,branch,branch):[],
  accounts:frontMode?[]:configure?await all(db,"SELECT id,code,name,chart_id,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code",company):[],
  charts:frontMode?[]:configure?await all(db,'SELECT id,name,base_currency,status FROM finance_charts WHERE company_id=? ORDER BY name',company):[],
  bankAccounts:frontMode?[]:bankAccounts,
  settlementConnection:!frontMode&&report&&!branch&&wants('reports')&&tenderConnection?{account_id:tenderConnection.account_id,livemode:tenderConnection.livemode,version:tenderConnection.version}:null,
  connection:frontMode?null:connect?await one(db,'SELECT * FROM pos_connections WHERE company_id=?',company):null,
  settlements:frontMode?[]:report&&!branch&&wants('reports')?await all(db,'SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100',company):[],
  frontSetup:frontMode?{hasProducts:Number(frontProductCount?.count??0)>0,blockedRegisters:frontBlockedRegisters}:null,
  frontReady:frontMode?Boolean(settings?.enabled&&frontHotels.length>frontBlockedRegisters.length&&Number(frontProductCount?.count??0)>0):undefined,
 };
 return frontData;
}
export type PosExecutiveEvidenceMetric="pos-sales"|"net-pos-sales"|"pos-payment-mix";
export async function posExecutiveEvidenceRow(db:SqlDatabase,actor:WorkContext,metric:PosExecutiveEvidenceMetric,recordId:string,filters:{from:string;to:string;currency:string}){
 requirePermission(actor,'pos.read');requirePermission(actor,'pos.reports');
 if(actor.departmentId)throw new ApiError(403,'EVIDENCE_FORBIDDEN','POS evidence is available only at Company or Branch scope.');
 const [kind,id]=recordId.split(':');if(!id||!['sale','refund','payment'].includes(kind))return null;
 const company=actor.companyId,branch=actor.branchId,currency=filters.currency,from=filters.from,to=filters.to;
 if(metric==='pos-sales'&&kind==='sale')return db.prepare(`SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,
     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,
     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id
   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1`)
   .bind(id,company,branch,branch,currency,from,to).first();
 if(metric==='net-pos-sales'&&kind==='sale')return db.prepare(`SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,
     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,
     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id
   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1`)
   .bind(id,company,branch,branch,currency,from,to).first();
 if(metric==='net-pos-sales'&&kind==='refund')return db.prepare(`SELECT r.id source_id,s.id sale_id,'refund' source_type,s.number receipt_number,
     s.business_date,r.accounting_date,s.currency,r.state,r.amount_minor::text amount_minor,
     (-r.amount_minor::numeric)::text contribution_minor,'0'::text refunded_minor,r.version revision,s.version sale_version,r.branch_id
   FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
   WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
     AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed' LIMIT 1`)
   .bind(id,company,branch,branch,currency,from,to).first();
 if(metric==='pos-payment-mix'&&kind==='payment')return db.prepare(`SELECT p.id source_id,'payment' source_type,s.id sale_id,s.number receipt_number,
     s.business_date,NULL::text accounting_date,s.currency,s.state sale_state,p.state,p.method,p.amount_minor::text amount_minor,
     COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)::text refunded_minor,
     (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0))::text contribution_minor,
     p.version revision,s.version sale_version,s.branch_id,p.created_at
   FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
   WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded' LIMIT 1`)
   .bind(id,company,branch,branch,currency,from,to).first();
 return null;
}
export type PosWorkspace=Awaited<ReturnType<typeof posWorkspace>>;
export async function posWork(actor:WorkContext):Promise<import('@/lib/work-item').WorkItem[]>{
 if(!hasPermission(actor,'pos.approve')||actor.departmentId)return [];
 const {write:db}=await holdingResources(actor.holdingId),company=actor.companyId,branch=actor.branchId;
 const refunds=await all(db,"SELECT * FROM pos_refunds WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='requested' AND requested_by<>?",company,branch,branch,actor.userId);
 const shifts=await all(db,"SELECT * FROM pos_shifts WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='closed' AND opened_by<>?",company,branch,branch,actor.userId);
 const cash=await all(db,'SELECT m.* FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND m.approved_by IS NULL AND m.created_by<>?',company,branch,branch,actor.userId);
 const jobs=branch?[]:await all(db,"SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'",company);
 return [...cash.map(r=>({id:str(r,'id'),type:'pos' as const,title:'POS · Review cash drawer',detail:str(r,'reason'),href:'/pos?tab=shifts&queue=approval',status:'requested',updatedAt:Number(r.created_at),queue:'todo' as const,relationships:['approval' as const]})),...refunds.map(r=>({id:str(r,'id'),type:'pos' as const,title:'POS · Review refund request',detail:str(r,'reason'),href:`/pos?tab=history&queue=approval`,status:str(r,'state'),updatedAt:Number(r.created_at),queue:'todo' as const,relationships:['approval' as const]})),...shifts.map(r=>({id:str(r,'id'),type:'pos' as const,title:'POS · Review closed shift',detail:`Variance ${Number(r.variance_minor)/100}`,href:'/pos?tab=shifts&queue=approval',status:str(r,'state'),updatedAt:Number(r.closed_at),queue:'todo' as const,relationships:['approval' as const]})),...jobs.map(r=>({id:str(r,'id'),type:'pos' as const,title:'POS · Payment item requires review',detail:str(r,'last_error'),href:'/pos?tab=reports',status:'exception',updatedAt:Number(r.created_at),queue:'todo' as const,relationships:['team' as const]}))];
}
