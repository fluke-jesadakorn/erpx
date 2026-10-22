import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/domain/posting-setup';
/** The same Commerce→Inventory→POS→Finance SHARE order used by
 * source readers. No business writer leases are acquired by page discovery. */
export function posStartupCommerceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_settings,commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_goods_valuations,commerce_stock_moves,commerce_stock,commerce_warehouses,commerce_products IN SHARE MODE')];}
export async function posStartupMappingState(database:SqlDatabase,companyId:string){return database.prepare('SELECT chart_id "chartId",currency,receivable_account_id "receivableAccountId",payable_account_id "payableAccountId",revenue_account_id "revenueAccountId",inventory_account_id "inventoryAccountId",clearing_account_id "clearingAccountId",cogs_account_id "cogsAccountId",input_tax_account_id "inputTaxAccountId",output_tax_account_id "outputTaxAccountId",cash_account_id "cashAccountId" FROM commerce_settings WHERE company_id=?').bind(companyId).first<CommercePostingSetupConfiguration>();}
export {manufacturingPostingDocumentState as posStartupDocumentState} from '@erpx/server-commerce/adapters/sales-goods-posting-setup';
