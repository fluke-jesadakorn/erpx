import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativePosSettingsAccountsStatement, nativePosCounterPaymentConnectionStatement } from "./native-pos-core-statement";
export function pmsCounterPaymentFacts(db: SqlDatabase) {
    return {
        /** Served by the native POS owner statement registry
         * (`counter_readposconnectionreadiness_536`); no local SQL mirror remains. */
        readPosConnections: (facts: {
            companyId: string;
        }) => nativePosCounterPaymentConnectionStatement<{
            account_id: string;
            livemode: boolean;
        }>(db, facts.companyId),
        /** Served by the native POS owner statement registry
         * (`core_readpossettingsaccounts_525`); no local SQL mirror remains. */
        readPosSettings: (facts: {
            companyId: string;
        }) => nativePosSettingsAccountsStatement<{ accounts_json: string }>(db, facts.companyId)
    };
}
