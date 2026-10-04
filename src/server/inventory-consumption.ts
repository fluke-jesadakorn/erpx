import 'server-only';
import { z } from 'zod';
import type { WorkContext } from './authorization';
import type { SqlDatabase, SqlStatement } from './runtime/types';
import { assertion } from './commands';
import { ApiError } from './http';
import { prepareJournal } from './finance';
import { snapshotMoney } from './company-money';
import { minorToDecimal } from '@/lib/finance-money';
import { scaledAmount } from '@/lib/commerce-math';

export const consumptionLotSchema=z.object({lotId:z.string().min(1),quantityMilli:z.number().int().positive().safe(),balanceVersion:z.number().int().positive()});
type LotInput=z.infer<typeof consumptionLotSchema>;
type Input={key:string;sourceKind:'maintenance'|'field_service';sourceId:string;sourceVersion:number;branchId:string;departmentId:string|null;
 warehouseId:string;productId:string;quantityMilli:number;entryDate:string;reason:string;ownReservationMilli?:number;allocations:LotInput[];beforeStock?:SqlStatement[]};

export async function availableServiceSpares(db:SqlDatabase,a:WorkContext,branchId:string){
 const stock=(await db.prepare(`SELECT w.id warehouse_id,w.name warehouse_name,p.id product_id,p.sku,p.name product_name,p.unit,p.tracking_mode,
  GREATEST(0,s.quantity_milli-COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)
  -inventory_quarantined_quantity(s.warehouse_id,s.product_id)) available_milli
  FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id JOIN commerce_products p ON p.id=s.product_id
  WHERE w.company_id=? AND p.company_id=w.company_id AND w.branch_id=? AND w.status='active' AND p.status='active' AND s.quantity_milli>0
  ORDER BY w.name,p.sku LIMIT 200`).bind(a.companyId,branchId).all<{warehouse_id:string;warehouse_name:string;product_id:string;sku:string;product_name:string;unit:string;tracking_mode:string;available_milli:number}>()).results;
 const lots=(await db.prepare(`SELECT b.lot_id,b.warehouse_id,l.product_id,l.lot_code,l.serial_number,b.quantity_milli,b.version,l.expiry_date
  FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id JOIN commerce_warehouses w ON w.id=b.warehouse_id
  WHERE l.company_id=? AND w.company_id=l.company_id AND w.branch_id=? AND w.status='active' AND l.status='active' AND b.quantity_milli>0
  AND (l.expiry_date IS NULL OR l.expiry_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
  AND NOT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed'))
  ORDER BY l.expiry_date NULLS LAST,l.lot_code,l.serial_number LIMIT 500`).bind(a.companyId,branchId).all<{lot_id:string;warehouse_id:string;product_id:string;lot_code:string|null;serial_number:string|null;quantity_milli:number;version:number;expiry_date:string|null}>()).results;
 return {stock,lots};
}

/** Owner stock consumption, immutable lot evidence and independently posted Finance GL share one command transaction. */
export async function prepareInventoryConsumption(db:SqlDatabase,a:WorkContext,input:Input){
 const stock=await db.prepare(`SELECT s.*,p.tracking_mode,w.branch_id
  FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id AND p.company_id=? JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=p.company_id
  WHERE s.warehouse_id=? AND s.product_id=? AND w.branch_id=? AND p.status='active' AND w.status='active'`).bind(a.companyId,input.warehouseId,input.productId,input.branchId)
  .first<{quantity_milli:number;value_minor:number;version:number;tracking_mode:string;branch_id:string}>();
 if(!stock||stock.quantity_milli<input.quantityMilli)throw new ApiError(409,'PART_STOCK_UNAVAILABLE','Choose available spare stock from the service branch warehouse.');
 const config=await db.prepare(`SELECT s.chart_id,c.base_currency currency,s.inventory_account_id,s.cogs_account_id,c.base_currency
  FROM commerce_settings s JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?`).bind(a.companyId)
  .first<{chart_id:string;currency:string;inventory_account_id:string;cogs_account_id:string;base_currency:string}>();
 if(!config||config.currency!==config.base_currency)throw new ApiError(422,'PART_FUNCTIONAL_CURRENCY_REQUIRED','Review the functional-currency inventory and service expense adapter before consuming parts.');
 const periods=(await db.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(a.companyId,input.entryDate,input.entryDate).all<{id:string}>()).results;
 if(periods.length!==1)throw new ApiError(409,'PART_POSTING_PERIOD_REQUIRED','Choose a date in one open Finance period.');
 const id=`operational-part-${input.key}`,now=Date.now(),steps:SqlStatement[]=[
  db.prepare('SELECT product_id FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE').bind(input.warehouseId,input.productId),
  assertion(db,input.key,'part-stock-version','EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)',[input.warehouseId,input.productId,stock.version]),
  assertion(db,input.key,'part-source-availability',`EXISTS(SELECT 1 FROM commerce_stock s WHERE s.warehouse_id=? AND s.product_id=? AND s.quantity_milli-
    COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)
    -inventory_quarantined_quantity(s.warehouse_id,s.product_id)+?>=?)`,[input.warehouseId,input.productId,input.ownReservationMilli??0,input.quantityMilli]),
  assertion(db,input.key,'part-date','NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)',[input.warehouseId,input.productId,input.entryDate]),
  assertion(db,input.key,'part-finance-mapping',`EXISTS(SELECT 1 FROM commerce_settings s JOIN finance_charts c ON c.id=s.chart_id
    JOIN finance_accounts inventory ON inventory.id=s.inventory_account_id AND inventory.company_id=s.company_id AND inventory.chart_id=s.chart_id AND inventory.category='asset' AND inventory.status='active'
    JOIN finance_accounts expense ON expense.id=s.cogs_account_id AND expense.company_id=s.company_id AND expense.chart_id=s.chart_id AND expense.category IN ('expense','other_expense') AND expense.status='active'
    WHERE s.company_id=? AND s.inventory_account_id=? AND s.cogs_account_id=? AND inventory.currency=c.base_currency AND expense.currency=c.base_currency AND inventory.id<>expense.id)`,[a.companyId,config.inventory_account_id,config.cogs_account_id]),
 ];
 let value=0;const lots:Array<LotInput&{value:number}>=[];
 if(stock.tracking_mode==='none'){
  if(input.allocations.length)throw new ApiError(422,'PART_LOT_NOT_ALLOWED','This spare does not use lot or serial tracking.');
  value=input.quantityMilli===stock.quantity_milli?stock.value_minor:scaledAmount(stock.value_minor,input.quantityMilli,stock.quantity_milli);
 }else{
  if(!input.allocations.length||new Set(input.allocations.map(l=>l.lotId)).size!==input.allocations.length||input.allocations.reduce((n,l)=>n+l.quantityMilli,0)!==input.quantityMilli)
   throw new ApiError(422,'PART_LOT_ALLOCATION_REQUIRED','Select the full spare quantity from distinct available lots or serials.');
  for(const [index,allocation] of input.allocations.entries()){
   const lot=await db.prepare(`SELECT b.quantity_milli,b.value_minor,b.version,l.tracking_mode,l.expiry_date,l.status
    FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=? AND b.warehouse_id=? AND l.product_id=? AND l.company_id=?`)
    .bind(allocation.lotId,input.warehouseId,input.productId,a.companyId).first<{quantity_milli:number;value_minor:number;version:number;tracking_mode:string;expiry_date:string|null;status:string}>();
   if(!lot||lot.status!=='active'||lot.version!==allocation.balanceVersion||lot.quantity_milli<allocation.quantityMilli||lot.expiry_date&&lot.expiry_date<input.entryDate||lot.tracking_mode==='serial'&&allocation.quantityMilli!==1000)
    throw new ApiError(409,'PART_LOT_CHANGED','Reload an available unexpired spare lot or whole serial.');
   const cost=allocation.quantityMilli===lot.quantity_milli?lot.value_minor:scaledAmount(lot.value_minor,allocation.quantityMilli,lot.quantity_milli);value+=cost;lots.push({...allocation,value:cost});
   steps.push(db.prepare('SELECT lot_id FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=? FOR UPDATE').bind(allocation.lotId,input.warehouseId),
    assertion(db,input.key,`part-lot-${index}`,`EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.lot_id=? AND b.warehouse_id=? AND b.version=? AND b.quantity_milli>=?
     AND l.status='active' AND (l.expiry_date IS NULL OR l.expiry_date>=?) AND NOT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')))`
      ,[allocation.lotId,input.warehouseId,allocation.balanceVersion,allocation.quantityMilli,input.entryDate]),
    db.prepare('UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE lot_id=? AND warehouse_id=?').bind(allocation.quantityMilli,cost,allocation.lotId,input.warehouseId),
    db.prepare("UPDATE inventory_lots SET status=CASE WHEN NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND quantity_milli>0) THEN 'depleted' ELSE status END WHERE id=?").bind(allocation.lotId,allocation.lotId),
    db.prepare('INSERT INTO operational_inventory_consumption_lots VALUES(?,?,?,?,?,?)').bind(`${id}-lot-${index}`,id,allocation.lotId,allocation.quantityMilli,cost,allocation.balanceVersion),
    db.prepare('INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`${id}-lot-move-${index}`,allocation.lotId,input.warehouseId,input.sourceKind,id,-allocation.quantityMilli,-cost,input.entryDate,a.userId,now));
  }
 }
 if(value<=0)throw new ApiError(422,'PART_VALUATION_REQUIRED','Reconcile positive spare valuation before posting its service cost.');
 const journal=await prepareJournal(db,a,{chartId:config.chart_id,periodId:periods[0].id,entryDate:input.entryDate,currency:config.currency,
  memo:`${input.sourceKind} spare usage ${input.sourceId} · ${input.reason}`,sourceType:`${input.sourceKind}_parts`,sourceId:id,isOpening:false,
  lines:[{accountId:config.cogs_account_id,debit:minorToDecimal(value),credit:'0'},{accountId:config.inventory_account_id,debit:'0',credit:minorToDecimal(value)}]},`${input.key}:parts`,'draft');
 const money=await snapshotMoney(db,a,{sourceType:'operational_inventory_consumption',sourceId:id,amountKind:'stock_cost',transactionDate:input.entryDate,currency:config.currency,currencyScale:2,amountUnits:String(value)});
 // Insert source before its lot evidence to retain immediate FK checks; all writes
 // and the deferred lot-control totals remain in the owning command transaction.
 const beforeLots=steps.splice(0,5);
 return {id,journalId:journal.result.id,currency:config.currency,valueMinor:value,lots,
  steps:[...beforeLots,...(input.beforeStock??[]),...journal.steps,
   db.prepare('INSERT INTO operational_inventory_consumptions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,input.branchId,input.departmentId,input.sourceKind,input.sourceId,input.sourceVersion,input.warehouseId,input.productId,input.quantityMilli,value,config.currency,input.entryDate,journal.result.id,input.reason,a.userId,now),
   ...steps,money.statement,
   db.prepare('UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?').bind(input.quantityMilli,value,input.warehouseId,input.productId)]};
}
