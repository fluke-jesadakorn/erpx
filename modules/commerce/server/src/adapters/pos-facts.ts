import { commerceFinalOwnerProjectionSources as posProjectionSources } from "@erpx/pos-server/public/commerce-final-owner-projection-sources";
import "server-only";
import {one,type SqlValueRow} from '@erpx/platform-runtime-server/public/runtime/sql';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeCommerceReadAll,nativeCommerceReadFirst,RD_SETTINGS_BY_COMPANY,RD_CURRENCY_BY_COMPANY,RD_POS_WAREHOUSES_ALL} from './native-commerce-read';
export function PosReadCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => one(db, `SELECT COUNT(DISTINCT p.id)::int count FROM commerce_products p JOIN (${posProjectionSources.pos_catalog}) c ON c.product_id=p.id AND c.enabled=true JOIN (${posProjectionSources.pos_register_catalog}) rc ON rc.product_id=p.id AND rc.enabled=true JOIN (${posProjectionSources.pos_registers}) r ON r.id=rc.register_id WHERE p.company_id=? AND p.status='active' AND r.active=true AND (?::text IS NULL OR r.branch_id=?)`, facts.company, facts.branch, facts.branch2),
        readCommerceSettings1: (facts: {
            company: string;
        }) => nativeCommerceReadFirst<SqlValueRow>(RD_SETTINGS_BY_COMPANY, [facts.company], db),
        readCommerceSettings2: (facts: {
            company: string;
        }) => nativeCommerceReadFirst<SqlValueRow>(RD_CURRENCY_BY_COMPANY, [facts.company], db),
        readCommerceWarehouses1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => nativeCommerceReadAll<SqlValueRow>(RD_POS_WAREHOUSES_ALL,[facts.company,facts.branch,facts.branch2],db)
    };
}
