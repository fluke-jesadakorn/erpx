import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../commerce/public/commerce-contracts";
export function CommerceSalesFacts(db:SqlDatabase){return {
readSalesPriceLists1:(facts:{companyId:string;})=>db.prepare("SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name").bind(facts.companyId).all<Row>(),
readSalesPriceListItems1:(facts:{companyId:string;})=>db.prepare("SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal").bind(facts.companyId).all<Row>(),
guardRule18:(facts:{key:string;guardId:string;values:(string | number | null)[];})=>assertion(db,facts.key,facts.guardId,"EXISTS(SELECT 1 FROM sales_price_lists WHERE id=? AND company_id=? AND version=? AND status='active' AND valid_from<=? AND valid_until>=?)",facts.values)
};}
