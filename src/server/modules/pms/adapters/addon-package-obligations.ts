import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { assertPmsPackageObligationSources } from "./package-obligations";
import { assertPosAddonPackageObligationSources, posAddonPackageSettlementSources as pos } from "../../pos/public/addon-package-obligations";

type Input = { companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number };
/** PMS owns the source-key/entry relationship and the guest-account settlement
 * unit. Transfers/reversals follow originalId; a different Hotel bill is never
 * selected merely because it shares a property or a configured GL account. */
function sourceSql(packageId:string) {
 if(!['pos-hotel-charge','pms-minibar-linen'].includes(packageId))throw new Error('PMS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const origins=packageId==='pos-hotel-charge'?`SELECT 'pos-hotel-charge'::text package_id,p.company_id,s.property_id,s.source_key,s.record_id entry_id,sale.total_minor expected_total FROM pms_sources s JOIN pms_properties p ON p.id=s.property_id
      JOIN (${pos('pos-hotel-charge').sales}) sale ON sale.company_id=p.company_id AND s.source_key='pos:'||sale.id||':'||sale.id`:
    `SELECT 'pms-minibar-linen'::text package_id,m.company_id,m.property_id,'minibar:'||m.id source_key,s.record_id entry_id,NULL::bigint expected_total FROM records m LEFT JOIN pms_sources s ON s.property_id=m.property_id AND s.source_key='minibar:'||m.id
      WHERE m.kind='movement' AND m.data->>'kind'='minibar.charge'`;
 return `WITH RECURSIVE records AS MATERIALIZED (SELECT p.company_id,r.id,r.property_id,r.kind,r.version,r.data_json::jsonb data FROM pms_records r JOIN pms_properties p ON p.id=r.property_id),
  origins AS MATERIALIZED (${origins}),
  entries(package_id,company_id,property_id,id) AS (
    SELECT o.package_id,o.company_id,e.property_id,e.id FROM origins o JOIN records e ON e.property_id=o.property_id AND e.id=o.entry_id AND e.kind='entry'
    UNION SELECT e.package_id,e.company_id,r.property_id,r.id FROM entries e JOIN records r ON r.property_id=e.property_id AND r.kind='entry' AND r.data->>'originalId'=e.id),
  account_entries AS MATERIALIZED (SELECT e.package_id,e.company_id,r.property_id,r.data->>'guestAccountId' id,
      r.data->>'kind' IN ('charge','transfer') AND (r.data->>'total')::bigint>0 track_due
    FROM entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id WHERE
      r.data->>'kind'='adjustment' AND (r.data->>'total')::bigint<0
      OR r.data->>'kind' IN ('charge','transfer') AND (r.data->>'total')::bigint>0 AND NOT EXISTS(
        SELECT 1 FROM entries child JOIN records changed ON changed.id=child.id AND changed.property_id=child.property_id
        WHERE child.package_id=e.package_id AND child.company_id=e.company_id AND changed.data->>'originalId'=e.id AND (changed.data->>'total')::bigint<0)),
  accounts AS MATERIALIZED (SELECT package_id,company_id,property_id,id,bool_or(track_due) track_due FROM account_entries GROUP BY package_id,company_id,property_id,id),
  financial_entries AS MATERIALIZED (SELECT * FROM entries UNION SELECT a.package_id,a.company_id,r.property_id,r.id FROM accounts a JOIN records r ON r.property_id=a.property_id AND r.kind='entry' AND r.data->>'guestAccountId'=a.id),
  movements AS MATERIALIZED (SELECT 'pms-minibar-linen'::text package_id,r.company_id,r.property_id,r.id,r.version,r.data,
      i.data->>'warehouseId' warehouse_id,i.data->>'productId' product_id
    FROM records r LEFT JOIN records i ON i.id=r.data->>'itemId' AND i.property_id=r.property_id AND i.kind='item'
    WHERE r.kind='movement' AND r.data->>'kind' IN ('minibar.charge','linen.receive','linen.issue','linen.return','linen.wash','linen.clean','linen.loss'))`;
}
export function pmsAddonPackageSettlementSources(packageId:string) {
 const sources=sourceSql(packageId);
 return {
  accounts: `${sources} SELECT a.package_id,a.company_id,a.property_id,a.id,a.track_due,r.version,r.data FROM accounts a LEFT JOIN records r ON r.id=a.id AND r.property_id=a.property_id AND r.kind='guest_account'`,
  postings: `${sources} SELECT DISTINCT source.package_id,p.company_id,pp.property_id,pp.source_key,pp.journal_id,p.id expected_source_id,p.config_json::jsonb->>'currency' expected_currency FROM pms_postings pp JOIN pms_properties p ON p.id=pp.property_id JOIN (
    SELECT package_id,company_id,property_id,id FROM financial_entries UNION SELECT package_id,company_id,property_id,id FROM movements WHERE data->>'kind' IN ('minibar.charge','linen.receive')) source
    ON source.company_id=p.company_id AND source.property_id=pp.property_id AND source.id=pp.record_id`,
  requiredPostings: `${sources} SELECT e.package_id,e.company_id,e.property_id,r.id||':charge' source_key FROM financial_entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id WHERE r.data->>'kind'='charge' AND r.data->>'method' IS DISTINCT FROM 'opening' AND (r.data->>'total')::bigint>0
    UNION SELECT e.package_id,e.company_id,e.property_id,r.id||':reverse' FROM entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id WHERE r.data->>'kind'='adjustment' AND (r.data->>'total')::bigint<0
    UNION SELECT e.package_id,e.company_id,e.property_id,r.id||CASE WHEN r.data->>'description'='Receive OTA / company remittance' THEN ':settlement' ELSE ':receipt' END FROM financial_entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id WHERE r.data->>'kind' IN ('payment','deposit') AND r.data->>'method' IN ('cash','transfer','edc','stripe') AND (r.data->>'total')::bigint>0
    UNION SELECT e.package_id,e.company_id,e.property_id,r.id||':refund' FROM financial_entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id WHERE r.data->>'kind'='refund' AND r.data->>'method' IN ('cash','transfer','edc','stripe') AND (r.data->>'total')::bigint<0
    UNION SELECT m.package_id,m.company_id,m.property_id,'stock:'||m.id FROM movements m JOIN pms_stock_moves stock ON stock.id=m.id AND stock.property_id=m.property_id WHERE stock.value_minor<>0`,
  // Derive required children from native money entries, not only documents
  // that happen to remain present. Reviewed opening pockets use their distinct
  // original import payment/number/journal and are deliberately not reclassified.
  payments: `${sources}, expected AS MATERIALIZED (SELECT e.package_id,e.company_id,e.property_id,r.id entry_id,r.version,r.data,
      CASE WHEN r.data->>'kind'='refund' THEN 'refund' ELSE 'receipt' END expected_kind,abs((r.data->>'total')::bigint) expected_amount,
      p.config_json::jsonb->>'currency' expected_currency,CASE r.data->>'method' WHEN 'cash' THEN p.config_json::jsonb#>>'{accounts,cash}' WHEN 'transfer' THEN p.config_json::jsonb#>>'{accounts,bank}' ELSE p.config_json::jsonb#>>'{accounts,edc}' END expected_bank_account
    FROM financial_entries e JOIN records r ON r.id=e.id AND r.property_id=e.property_id JOIN pms_properties p ON p.id=r.property_id AND p.company_id=e.company_id
    WHERE r.data->>'method' IN ('cash','transfer','edc','stripe') AND (r.data->>'kind' IN ('payment','deposit') AND (r.data->>'total')::bigint>0 OR r.data->>'kind'='refund' AND (r.data->>'total')::bigint<0))
    SELECT e.package_id,e.company_id,e.property_id,e.entry_id,e.version,e.expected_kind,e.expected_amount,e.expected_currency,e.expected_bank_account,
      d.id document_id,d.finance_id,count(d.id) OVER(PARTITION BY e.package_id,e.company_id,e.property_id,e.entry_id) document_count,
      -- Preserve document()'s original first-source-key journal selection: a
      -- taxable advance may point to its advance-tax posting, not receipt.
      (SELECT pp.journal_id FROM pms_postings pp WHERE pp.property_id=e.property_id AND pp.record_id=e.entry_id ORDER BY pp.source_key LIMIT 1) expected_journal_id,
      d.snapshot_json::jsonb->>'amount' document_amount,d.snapshot_json::jsonb#>>'{property,currency}' document_currency,
      CASE WHEN jsonb_typeof(d.snapshot_json::jsonb->'entries')='array' THEN jsonb_array_length(d.snapshot_json::jsonb->'entries') ELSE 0 END snapshot_entry_count,
      d.snapshot_json::jsonb#>>'{entries,0,data,kind}' snapshot_entry_kind,d.snapshot_json::jsonb#>>'{entries,0,data,method}' snapshot_entry_method,d.snapshot_json::jsonb#>>'{entries,0,data,total}' snapshot_entry_total,
      e.data->>'kind' entry_kind,e.data->>'method' entry_method,e.data->>'total' entry_total
    FROM expected e LEFT JOIN pms_documents d ON d.property_id=e.property_id AND d.kind=e.expected_kind AND d.snapshot_json::jsonb#>>'{entries,0,id}'=e.entry_id`,
  stockMoves: `${sources} SELECT m.package_id,m.company_id,m.property_id,m.id,m.version,m.warehouse_id,m.product_id,-(m.data->>'quantity')::bigint*1000 expected_quantity,m.data->>'kind' kind,
    s.warehouse_id actual_warehouse_id,s.product_id actual_product_id,s.quantity_milli,s.value_minor FROM movements m LEFT JOIN pms_stock_moves s ON s.id=m.id AND s.property_id=m.property_id
    WHERE m.data->>'kind' IN ('minibar.charge','linen.receive')`,
} as const;
}
export function assertPmsAddonPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string,packageId:string) {
  assertPmsPackageObligationSources(owner,database,companyId);
  if(packageId==='pos-hotel-charge')assertPosAddonPackageObligationSources(owner,database,companyId,packageId);
}
export async function pmsAddonPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input) {
  assertPmsAddonPackageObligationSources(owner,database,input.companyId,input.packageId);
  if(input.moduleId!=='pms'||!['pos-hotel-charge','pms-minibar-linen'].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)
    throw new Error('PMS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
  const sources=sourceSql(input.packageId);
  const facts=(await database.prepare(`${sources}, scope AS (SELECT ?::text company_id,?::text package_id)
    SELECT 'source' kind,o.source_key id,0::bigint version,'source-mismatch' state FROM origins o,scope WHERE o.company_id=scope.company_id AND o.package_id=scope.package_id AND NOT EXISTS(SELECT 1 FROM records e WHERE e.company_id=o.company_id AND e.property_id=o.property_id AND e.id=o.entry_id AND e.kind='entry' AND e.data->>'kind'='charge' AND e.data->>'sourceKey'=o.source_key AND COALESCE((e.data->>'total')::bigint,0)>0 AND (o.expected_total IS NULL OR (e.data->>'total')::bigint=o.expected_total))
    UNION ALL SELECT 'account',a.id,COALESCE(r.version,0),COALESCE(r.data->>'state','missing') FROM accounts a LEFT JOIN records r ON r.id=a.id AND r.property_id=a.property_id AND r.kind='guest_account',scope WHERE a.company_id=scope.company_id AND a.package_id=scope.package_id AND (
      r.id IS NULL OR a.track_due AND COALESCE((r.data->>'due')::bigint,0)<>0 OR COALESCE((r.data->>'advance')::bigint,0)<>0 OR COALESCE((r.data->>'refundReserved')::bigint,0)<>0 OR COALESCE((r.data->>'depositRefundReserved')::bigint,0)<>0)
    UNION ALL SELECT 'refund',r.id,r.version,r.data->>'state' FROM records r,scope WHERE r.company_id=scope.company_id AND r.kind='refund' AND r.data->>'state' NOT IN ('paid','rejected') AND EXISTS(SELECT 1 FROM accounts a WHERE a.package_id=scope.package_id AND a.company_id=r.company_id AND a.property_id=r.property_id AND a.id=r.data->>'guestAccountId')
    UNION ALL SELECT 'correction',c.id,c.version,c.status FROM pms_charge_corrections c,scope WHERE c.company_id=scope.company_id AND c.status='pending' AND EXISTS(SELECT 1 FROM entries e WHERE e.package_id=scope.package_id AND e.company_id=c.company_id AND e.property_id=c.property_id AND e.id=c.entry_id)
    UNION ALL SELECT 'linen',r.id,r.version,'custody' FROM records r,scope WHERE scope.package_id='pms-minibar-linen' AND r.company_id=scope.company_id AND r.kind='item' AND r.data->>'kind'='linen' AND (COALESCE((r.data->>'issued')::bigint,0)<>0 OR COALESCE((r.data->>'laundry')::bigint,0)<>0)
    ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results;
  return {companyId:input.companyId,packageId:input.packageId,moduleId:'pms',observedAt:input.now,
    revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.length};
}
