import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
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
        readPosSettings1: (facts: {
            companyId: string;
        }) => db.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(facts.companyId).first<{
            accounts_json: string;
        }>()
    };
}
