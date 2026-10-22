import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsCounterPaymentFacts(db: SqlDatabase) {
    return {
        readPosConnections: (facts: {
            companyId: string;
        }) => db.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(facts.companyId).first<{
            account_id: string;
            livemode: boolean;
        }>(),
        readPosSettings: (facts: {
            companyId: string;
        }) => db.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(facts.companyId).first<{
            accounts_json: string;
        }>()
    };
}
