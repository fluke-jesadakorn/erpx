/** Read-only evidence for shared physical stock, transport custody and WIP. */
import {createHash} from 'node:crypto';
import type {SqlDatabase} from "@erpx/server-runtime/adapters/runtime/types";

export type StockRow={companyId:string;warehouseId:string;productId:string;quantityMilli:string;valueMinor:string};
export type StockMovement=StockRow&{id:string;sourceType:string;sourceId:string;movementDate:string;createdAt:string};
export type StockCompany={companyId:string;functionalCurrencies:string[];inventoryAccountIds:string[];invalidMappings:number;physicalMinor:string;inTransitMinor:string;ledgerMinor:string;stockRows:number};
export type StockWip={companyId:string;accountId:string;currency:string;sourceMinor:string;ledgerMinor:string;movements:number};
export type StockSnapshot={stocks:StockRow[];movements:StockMovement[];companies:StockCompany[];wip:StockWip[];invalidRows:number;invalidTrackedBalances:number;unmappedWipMovements:number};
const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const stockKey=(row:StockRow)=>`${row.warehouseId}\0${row.productId}`;
const moveKey=(row:StockMovement)=>`${row.sourceType}\0${row.id}\0${stockKey(row)}`;

/** The caller holds one consistent read-only transaction per holding. */
export async function readStockSnapshot(db:SqlDatabase):Promise<StockSnapshot>{
 const stocks=(await db.prepare(`SELECT w.company_id "companyId",s.warehouse_id "warehouseId",s.product_id "productId",s.quantity_milli::text "quantityMilli",s.value_minor::text "valueMinor" FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id ORDER BY w.company_id,s.warehouse_id,s.product_id`).all<StockRow>()).results;
 const movements=(await db.prepare(`SELECT w.company_id "companyId",l.warehouse_id "warehouseId",l.product_id "productId",l.quantity_milli::text "quantityMilli",l.value_minor::text "valueMinor",l.id,l.source_type "sourceType",l.source_id "sourceId",l.movement_date "movementDate",l.created_at::text "createdAt" FROM inventory_ledger l JOIN commerce_warehouses w ON w.id=l.warehouse_id ORDER BY w.company_id,l.warehouse_id,l.product_id,l.source_type,l.id`).all<StockMovement>()).results;
 const companyRows=(await db.prepare(`WITH mappings AS (
 SELECT company_id,inventory_account_id account_id FROM commerce_settings
 UNION SELECT company_id,accounts_json::jsonb->>'inventory' FROM pos_settings
 UNION SELECT company_id,config_json::jsonb->'accounts'->>'inventory' FROM pms_properties WHERE config_json::jsonb->'accounts'->>'inventory' IS NOT NULL
 UNION SELECT company_id,inventory_account_id FROM inventory_transit
 UNION SELECT company_id,inventory_account_id FROM manufacturing_accounting_profiles
 ),scope AS (SELECT w.company_id FROM commerce_warehouses w JOIN commerce_stock s ON s.warehouse_id=w.id UNION SELECT w.company_id FROM inventory_ledger l JOIN commerce_warehouses w ON w.id=l.warehouse_id UNION SELECT company_id FROM inventory_in_transit UNION SELECT company_id FROM manufacturing_accounting_profiles),
 mapped AS (SELECT m.company_id,m.account_id,a.company_id account_company,c.base_currency FROM mappings m LEFT JOIN finance_accounts a ON a.id=m.account_id LEFT JOIN finance_charts c ON c.id=a.chart_id),
 physical AS (SELECT w.company_id,SUM(s.value_minor)::text total,COUNT(*) n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id GROUP BY w.company_id),
 transport AS (SELECT company_id,SUM(value_minor)::text total FROM inventory_in_transit GROUP BY company_id),
 ledger AS (SELECT j.company_id,SUM(l.base_debit_minor-l.base_credit_minor)::text total FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.status='posted' AND EXISTS(SELECT 1 FROM mapped m WHERE m.company_id=j.company_id AND m.account_id=l.account_id) GROUP BY j.company_id)
 SELECT s.company_id "companyId",COALESCE((SELECT json_agg(DISTINCT base_currency) FILTER(WHERE base_currency IS NOT NULL) FROM mapped WHERE company_id=s.company_id),'[]'::json) "functionalCurrencies",
 COALESCE((SELECT json_agg(DISTINCT account_id) FILTER(WHERE account_id IS NOT NULL) FROM mapped WHERE company_id=s.company_id),'[]'::json) "inventoryAccountIds",
 (SELECT COUNT(*)::int FROM mapped WHERE company_id=s.company_id AND (account_id IS NULL OR account_company IS DISTINCT FROM s.company_id OR base_currency IS NULL)) "invalidMappings",
 COALESCE(p.total,'0') "physicalMinor",COALESCE(t.total,'0') "inTransitMinor",COALESCE(l.total,'0') "ledgerMinor",COALESCE(p.n,0)::int "stockRows"
 FROM scope s LEFT JOIN physical p USING(company_id) LEFT JOIN transport t USING(company_id) LEFT JOIN ledger l USING(company_id) ORDER BY s.company_id`).all<Omit<StockCompany,'functionalCurrencies'|'inventoryAccountIds'>&{functionalCurrencies:string;inventoryAccountIds:string}>()).results;
 const companies=companyRows.map(row=>({...row,functionalCurrencies:(JSON.parse(row.functionalCurrencies) as string[]).sort(),inventoryAccountIds:(JSON.parse(row.inventoryAccountIds) as string[]).sort()}));
 const wip=(await db.prepare(`WITH profiles AS (SELECT DISTINCT company_id,wip_account_id account_id,currency FROM manufacturing_accounting_profiles),source AS (
 SELECT m.company_id,p.account_snapshot_json::jsonb->>'wip' account_id,SUM(CASE WHEN m.movement_kind IN ('output','abnormal_scrap') THEN -m.amount_minor ELSE m.amount_minor END)::text total,COUNT(*) n
 FROM manufacturing_cost_movements m JOIN manufacturing_order_postings p ON p.journal_id=m.journal_id GROUP BY m.company_id,p.account_snapshot_json::jsonb->>'wip'),ledger AS (
 SELECT j.company_id,l.account_id,SUM(l.base_debit_minor-l.base_credit_minor)::text total FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.status='posted' GROUP BY j.company_id,l.account_id)
 SELECT p.company_id "companyId",p.account_id "accountId",p.currency,COALESCE(s.total,'0') "sourceMinor",COALESCE(l.total,'0') "ledgerMinor",COALESCE(s.n,0)::int movements FROM profiles p LEFT JOIN source s ON s.company_id=p.company_id AND s.account_id=p.account_id LEFT JOIN ledger l ON l.company_id=p.company_id AND l.account_id=p.account_id ORDER BY p.company_id,p.account_id`).all<StockWip>()).results;
 const invalidRows=Number(await db.prepare(`SELECT COUNT(*) FROM commerce_stock s LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id LEFT JOIN commerce_products p ON p.id=s.product_id WHERE s.quantity_milli<0 OR s.value_minor<0 OR w.id IS NULL OR p.id IS NULL OR p.company_id<>w.company_id`).first('count'));
 const invalidTrackedBalances=Number(await db.prepare(`SELECT COUNT(*) FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id WHERE p.tracking_mode<>'none' AND (s.quantity_milli<>COALESCE((SELECT SUM(b.quantity_milli) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=s.warehouse_id AND l.product_id=s.product_id),0) OR s.value_minor<>COALESCE((SELECT SUM(b.value_minor) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=s.warehouse_id AND l.product_id=s.product_id),0))`).first('count'));
 const unmappedWipMovements=Number(await db.prepare(`SELECT COUNT(*) FROM manufacturing_cost_movements m LEFT JOIN manufacturing_order_postings p ON p.journal_id=m.journal_id WHERE p.id IS NULL OR p.company_id<>m.company_id OR p.account_snapshot_json::jsonb->>'wip' IS NULL`).first('count'));
 return {stocks,movements,companies,wip,invalidRows,invalidTrackedBalances,unmappedWipMovements};
}

/** No offset between Companies, products or source kinds can hide a discrepancy. */
export function reconcileStockSnapshots(before:StockSnapshot,after:StockSnapshot){
 const oldMoves=new Map(before.movements.map(row=>[moveKey(row),JSON.stringify(row)])),currentMoves=new Map(after.movements.map(row=>[moveKey(row),JSON.stringify(row)]));
 const retainedMovements=oldMoves.size===before.movements.length&&currentMoves.size===after.movements.length&&[...oldMoves].every(([key,row])=>currentMoves.get(key)===row);
 const expected=new Map(before.stocks.map(row=>[stockKey(row),{companyId:row.companyId,quantity:BigInt(row.quantityMilli),value:BigInt(row.valueMinor)}]));
 for(const row of after.movements)if(!oldMoves.has(moveKey(row))){const previous=expected.get(stockKey(row))??{companyId:row.companyId,quantity:BigInt(0),value:BigInt(0)};previous.quantity+=BigInt(row.quantityMilli);previous.value+=BigInt(row.valueMinor);expected.set(stockKey(row),previous);}
 const actual=new Map(after.stocks.map(row=>[stockKey(row),row])),stockAssertions=[...new Set([...expected.keys(),...actual.keys()])].sort().map(key=>{
  const row=actual.get(key),source=expected.get(key),[warehouseId,productId]=key.split('\0');
  return {companyId:row?.companyId??source!.companyId,warehouseId,productId,expectedQuantityMilli:String(source?.quantity??0),quantityMilli:row?.quantityMilli??'0',expectedValueMinor:String(source?.value??0),valueMinor:row?.valueMinor??'0'};
 });
 const companies=after.companies.map(row=>({...row,stockMinor:String(BigInt(row.physicalMinor)+BigInt(row.inTransitMinor)),differenceMinor:String(BigInt(row.physicalMinor)+BigInt(row.inTransitMinor)-BigInt(row.ledgerMinor))}));
 const wip=after.wip.map(row=>({...row,differenceMinor:String(BigInt(row.sourceMinor)-BigInt(row.ledgerMinor))}));
 const passed=retainedMovements&&before.stocks.length===new Map(before.stocks.map(row=>[stockKey(row),row])).size&&after.stocks.length===actual.size&&after.invalidRows===0&&after.invalidTrackedBalances===0&&after.unmappedWipMovements===0
  &&stockAssertions.every(row=>row.quantityMilli===row.expectedQuantityMilli&&row.valueMinor===row.expectedValueMinor)
  &&companies.every(row=>row.invalidMappings===0&&row.inventoryAccountIds.length>0&&row.functionalCurrencies.length===1&&row.differenceMinor==='0')&&wip.every(row=>row.differenceMinor==='0');
 return {passed,baselineSha256:digest(before),finalSha256:digest(after),retainedMovements,newMovements:after.movements.length-before.movements.length,stockAssertions,companies,wip,invalidRows:after.invalidRows,invalidTrackedBalances:after.invalidTrackedBalances,unmappedWipMovements:after.unmappedWipMovements};
}
