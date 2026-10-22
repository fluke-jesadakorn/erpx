import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CountRow} from '@erpx/server-inventory/public/count-contract';
import {servicePartsPostingSourceRows} from '@erpx/server-commerce/adapters/service-parts-posting-proof';
export {initialPhysicalStockMasterChoices,initialPhysicalStockMasterSource,initialPhysicalStockCommerceSourceLocks as serviceInitialPhysicalStockCommerceLocks} from '@erpx/server-commerce/adapters/initial-physical-stock-count-sources';
/** Only the unchanged real Service mapping and its immutable native receipt.
 * Existing-table proof remains safe when1062 has not been installed. */
export async function serviceInitialPhysicalStockCommerceSource(database:SqlDatabase,companyId:string){
 return database.prepare(`SELECT s.*,source.command_id service_setup_command_id,source.request_hash service_setup_request_hash
 FROM commerce_settings s JOIN (${servicePartsPostingSourceRows}) source ON source.company_id=s.company_id AND source.mapping=to_jsonb(s)
 JOIN finance_charts chart ON chart.id=s.chart_id AND chart.company_id=s.company_id AND chart.base_currency=s.currency AND chart.status='active'
 WHERE s.company_id=? ORDER BY source.command_id LIMIT 1`).bind(companyId).first<CountRow>();
}
