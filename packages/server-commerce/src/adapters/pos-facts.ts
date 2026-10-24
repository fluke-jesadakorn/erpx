import { commerceFinalOwnerProjectionSources as posProjectionSources } from "@erpx/server-pos/public/commerce-final-owner-projection-sources";
import "server-only";
import {one,all} from '@erpx/server-runtime/public/runtime/sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function PosReadCommerceFacts(db: SqlDatabase) {
    return {
        readCommerceProducts1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => one(db, `SELECT COUNT(DISTINCT p.id)::int count FROM commerce_products p JOIN (${posProjectionSources.pos_catalog}) c ON c.product_id=p.id AND c.enabled=true JOIN (${posProjectionSources.pos_register_catalog}) rc ON rc.product_id=p.id AND rc.enabled=true JOIN (${posProjectionSources.pos_registers}) r ON r.id=rc.register_id WHERE p.company_id=? AND p.status='active' AND r.active=true AND (?::text IS NULL OR r.branch_id=?)`, facts.company, facts.branch, facts.branch2),
        readCommerceSettings1: (facts: {
            company: string;
        }) => one(db, 'SELECT * FROM commerce_settings WHERE company_id=?', facts.company),
        readCommerceSettings2: (facts: {
            company: string;
        }) => one(db, 'SELECT currency FROM commerce_settings WHERE company_id=?', facts.company),
        readCommerceWarehouses1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, "SELECT * FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name", facts.company, facts.branch, facts.branch2)
    };
}
