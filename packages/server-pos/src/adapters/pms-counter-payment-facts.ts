import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativePosSettingsAccountsStatement } from "./native-pos-core-statement";
export function pmsCounterPaymentFacts(db: SqlDatabase) {
    return {
        readPosConnections: (facts: {
            companyId: string;
        }) => db.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(facts.companyId).first<{
            account_id: string;
            livemode: boolean;
        }>(),
        /** Served by the native POS owner statement registry
         * (`core_readpossettingsaccounts_525`); no local SQL mirror remains. */
        readPosSettings: (facts: {
            companyId: string;
        }) => nativePosSettingsAccountsStatement<{ accounts_json: string }>(db, facts.companyId)
    };
}
