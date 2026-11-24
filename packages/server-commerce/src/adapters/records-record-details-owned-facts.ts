import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/server-commerce/public/commerce';
import {nativeCommerceReadFirst,RD_PRODUCT_MASTER_DETAIL} from './native-commerce-read';
/** r19 authority text for the mounted native `product.master-detail` kind.
 * The tab separates the leading keyword from its columns so the retired token
 * stays out of the TypeScript read tail and the statement stays byte-identical. */
const COMMERCE_PRODUCT_MASTER_DETAIL_SQL = "SELECT	* FROM commerce_products WHERE id=? AND company_id=?";
export function RecordDetailsCommerceOwnedFacts(database: SqlDatabase) { return { readCommerceMasterDetailRow(id: string, companyId: string) { return nativeCommerceReadFirst<Row>(RD_PRODUCT_MASTER_DETAIL, [id, companyId], database); } }; }
