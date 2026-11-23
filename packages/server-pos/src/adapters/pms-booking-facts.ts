import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativePosSettingsAccountsStatement } from "./native-pos-core-statement";
export function pmsBookingPosFacts(db: SqlDatabase) {
    return {
        readPosConnections1: (facts: {
            companyId: string;
        }) => db.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true").bind(facts.companyId).first<{
            account_id: string;
            livemode: boolean;
            card_enabled: boolean;
            promptpay_enabled: boolean;
        }>(),
        /** Served by the native POS owner statement registry
         * (`core_readpossettingsaccounts_525`); no local SQL mirror remains. */
        readPosSettings1: (facts: {
            companyId: string;
        }) => nativePosSettingsAccountsStatement<{ accounts_json: string }>(db, facts.companyId)
    };
}
