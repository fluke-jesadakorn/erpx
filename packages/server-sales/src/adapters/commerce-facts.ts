import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
import {nativeSalesReadAll,SALES_RD_PRICE_LIST_ACTIVE,SALES_RD_PRICE_LIST_ITEMS_ACTIVE} from "./native-sales-read";
export function CommerceSalesFacts(db:SqlDatabase){return {
/** Owner-local active price-list reads. The fixed SQL is owned by the native
 * Sales `command-plan` port; no TS SQL mirror remains for these facts. */
readSalesPriceLists1:(facts:{companyId:string;})=>nativeSalesReadAll<Row>(SALES_RD_PRICE_LIST_ACTIVE,[facts.companyId],db),
readSalesPriceListItems1:(facts:{companyId:string;})=>nativeSalesReadAll<Row>(SALES_RD_PRICE_LIST_ITEMS_ACTIVE,[facts.companyId],db),
guardRule18:(facts:{key:string;guardId:string;values:(string | number | null)[];})=>assertion(db,facts.key,facts.guardId,"EXISTS(SELECT 1 FROM sales_price_lists WHERE id=? AND company_id=? AND version=? AND status='active' AND valid_from<=? AND valid_until>=?)",facts.values)
};}
