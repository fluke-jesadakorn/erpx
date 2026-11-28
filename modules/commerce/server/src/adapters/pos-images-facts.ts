import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeCommerceReadFirst,RD_POS_PRODUCT_IMAGE,RD_POS_PRODUCT_VERSION} from './native-commerce-read';
export function posImagesCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts1: (facts: {
            productId: string;
            companyId: string;
        }) => nativeCommerceReadFirst<{version:number}>(RD_POS_PRODUCT_VERSION,[facts.productId,facts.companyId],db)
    };
}
/** Establish the native product source before the POS private-read snapshot. */
export function posImageProductSourceLocks(db:SqlDatabase){return db.prepare('LOCK TABLE commerce_products IN SHARE ROW EXCLUSIVE MODE');}
export function posImageProductSourceImage(db:SqlDatabase,companyId:string,productId:string){
    return nativeCommerceReadFirst<string>(RD_POS_PRODUCT_IMAGE,[productId,companyId],db);
}
