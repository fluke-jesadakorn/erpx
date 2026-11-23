import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {all,one} from '@erpx/server-runtime/public/runtime/sql';
import { nativePosTablesStatement } from './native-pos-core-statement';
export function pmsHospitalitySetupPosFacts(db: SqlDatabase) {
    return {
        settings: (facts: {
            company: string;
        }) => one(db, 'SELECT enabled,currency FROM pos_settings WHERE company_id=?', facts.company),
        registers: (facts: {
            company: string;
            branch: string;
        }) => all(db, "SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=? AND branch_id=? AND active=true AND mode='restaurant'", facts.company, facts.branch),
        /** Served by the native POS owner statement registry (`pos_core_tables`).
         * The branch is pinned, so the registry's Company/branch scope returns
         * the same table rows; no local SQL mirror remains for this read. */
        tables: (facts: {
            company: string;
            branch: string;
        }) => nativePosTablesStatement(db, { company: facts.company, branch: facts.branch, branch2: facts.branch })
    };
}
