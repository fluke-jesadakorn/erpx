import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { all, one } from "@erpx/server-pos/public/pos-core";
export function pmsHospitalitySetupPosFacts(db: SqlDatabase) {
    return {
        settings: (facts: {
            company: string;
        }) => one(db, 'SELECT enabled,currency FROM pos_settings WHERE company_id=?', facts.company),
        registers: (facts: {
            company: string;
            branch: string;
        }) => all(db, "SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=? AND branch_id=? AND active=true AND mode='restaurant'", facts.company, facts.branch),
        tables: (facts: {
            company: string;
            branch: string;
        }) => all(db, 'SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND r.branch_id=?', facts.company, facts.branch)
    };
}
