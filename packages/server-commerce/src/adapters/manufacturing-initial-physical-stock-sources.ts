import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CountRow} from '@erpx/server-inventory/public/count-contract';
import {manufacturingPostingSetupReceiptRows,manufacturingExistingPostingMarker} from '@erpx/server-commerce/adapters/manufacturing-existing-posting-proof';
export {initialPhysicalStockMasterChoices,initialPhysicalStockMasterSource,initialPhysicalStockCommerceSourceLocks as manufacturingInitialPhysicalStockCommerceLocks} from '@erpx/server-commerce/adapters/initial-physical-stock-count-sources';
/** Private source identity. The shared settings row alone never grants this source. */
export async function manufacturingInitialPhysicalStockCommerceSource(database:SqlDatabase,companyId:string){
 return database.prepare(`SELECT s.*,r.command_id manufacturing_setup_command_id,r.request_hash manufacturing_setup_request_hash
 FROM commerce_settings s JOIN (${manufacturingPostingSetupReceiptRows}) r ON r.record_id=s.company_id
 AND (r.operation='commerce.manufacturing-posting-setup.initialize' OR to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL AND r.result_json::jsonb->'${manufacturingExistingPostingMarker}'->'mapping'=to_jsonb(s))
 JOIN finance_charts chart ON chart.id=s.chart_id AND chart.company_id=s.company_id AND chart.base_currency='THB'
 WHERE s.company_id=? ORDER BY r.created_at,r.command_id LIMIT 1`).bind(companyId).first<CountRow>();
}
