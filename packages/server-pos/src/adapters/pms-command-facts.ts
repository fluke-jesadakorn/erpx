import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsPosFacts(db: SqlDatabase) {
    return {
        readPosConnections: (facts: {
            companyId: string;
        }) => db.prepare("SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(facts.companyId).first(),
        readPosConnections2: (facts: {
            companyId: string;
            fact1: string;
        }) => db.prepare("SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true) AND (livemode=false OR ?='true')").bind(facts.companyId, facts.fact1).first(),
        readPosConnections3: (facts: {
            companyId: string;
        }) => db.prepare("SELECT account_id,livemode FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true)").bind(facts.companyId).first<{
            account_id: string;
            livemode: boolean;
        }>()
    };
}
