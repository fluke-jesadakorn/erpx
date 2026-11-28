import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  nativePosPromptPayConnectionStatement,
  nativePosChargeConnectionStatement,
  nativePosConnectionAccountStatement,
} from "./native-pos-core-statement";
export function pmsPosFacts(db: SqlDatabase) {
    return {
        /** Served by the native POS owner statement registry
         * (`pms_readposconnectionspromptpay_533`); no local SQL mirror remains. */
        readPosConnections: (facts: {
            companyId: string;
        }) => nativePosPromptPayConnectionStatement(db, facts.companyId),
        /** Served by the native POS owner statement registry
         * (`pms_readposconnectionscharge_534`); no local SQL mirror remains. */
        readPosConnections2: (facts: {
            companyId: string;
            fact1: string;
        }) => nativePosChargeConnectionStatement(db, facts.companyId, facts.fact1),
        /** Served by the native POS owner statement registry
         * (`pms_readposconnectionsaccount_535`); no local SQL mirror remains. */
        readPosConnections3: (facts: {
            companyId: string;
        }) => nativePosConnectionAccountStatement<{
            account_id: string;
            livemode: boolean;
        }>(db, facts.companyId)
    };
}
