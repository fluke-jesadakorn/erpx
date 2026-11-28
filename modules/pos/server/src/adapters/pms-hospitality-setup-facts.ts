import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
    nativePosTablesStatement,
    nativePosSettingsEnabledCurrencyStatement,
    nativePosRestaurantRegistersStatement,
} from './native-pos-core-statement';
export function pmsHospitalitySetupPosFacts(db: SqlDatabase) {
    return {
        settings: (facts: {
            company: string;
        }) => nativePosSettingsEnabledCurrencyStatement(db, facts.company),
        registers: (facts: {
            company: string;
            branch: string;
        }) => nativePosRestaurantRegistersStatement(db, facts.company, facts.branch),
        /** Served by the native POS owner statement registry (`pos_core_tables`).
         * The branch is pinned, so the registry's Company/branch scope returns
         * the same table rows; no local SQL mirror remains for this read. */
        tables: (facts: {
            company: string;
            branch: string;
        }) => nativePosTablesStatement(db, { company: facts.company, branch: facts.branch, branch2: facts.branch })
    };
}
