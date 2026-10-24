import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posImagesCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts1: (facts: {
            productId: string;
            companyId: string;
        }) => one(db, 'SELECT version FROM commerce_products WHERE id=? AND company_id=?', facts.productId, facts.companyId)
    };
}
/** Establish the native product source before the POS private-read snapshot. */
export function posImageProductSourceLocks(db:SqlDatabase){return db.prepare('LOCK TABLE commerce_products IN SHARE ROW EXCLUSIVE MODE');}
export function posImageProductSourceImage(db:SqlDatabase,companyId:string,productId:string){
    return db.prepare('SELECT to_jsonb(product)::text image FROM commerce_products product WHERE product.id=? AND product.company_id=?').bind(productId,companyId).first<string>('image');
}
