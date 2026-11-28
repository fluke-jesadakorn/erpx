import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativePosBookingConnectionStatement, nativePosSettingsAccountsStatement } from "./native-pos-core-statement";
export function pmsBookingPosFacts(db: SqlDatabase) {
    return {
        /** Served by the native POS owner statement registry
         * (`stripe_readposbookingconnection_528`); no local SQL mirror remains. */
        readPosConnections1: (facts: {
            companyId: string;
        }) => nativePosBookingConnectionStatement<{
            account_id: string;
            livemode: boolean;
            card_enabled: boolean;
            promptpay_enabled: boolean;
        }>(db, facts.companyId),
        /** Served by the native POS owner statement registry
         * (`core_readpossettingsaccounts_525`); no local SQL mirror remains. */
        readPosSettings1: (facts: {
            companyId: string;
        }) => nativePosSettingsAccountsStatement<{ accounts_json: string }>(db, facts.companyId)
    };
}
