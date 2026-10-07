import "server-only";
import { one } from "../../pos/public/pos-core";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function posImagesCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts1: (facts: {
            productId: string;
            companyId: string;
        }) => one(db, 'SELECT version FROM commerce_products WHERE id=? AND company_id=?', facts.productId, facts.companyId)
    };
}
