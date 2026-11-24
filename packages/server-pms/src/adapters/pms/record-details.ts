import "server-only";
import { holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { loadPms } from "../../../../../packages/server-pms/src/public/pms";
import { pmsHotelOverview } from "@erpx/server-pms/adapters/pms/home";
import { pmsRecordDetailFacts } from "@erpx/server-finance/public/pms-record-details-facts";
import { createPmsRecordDetails, type PmsRecordDetailsRepository } from "@erpx/server-pms/application/record-details";
import { nativePmsReadFirst, PMS_RD_DOCUMENT_SNAPSHOT } from "../native-pms-read";
import { PMS_STRIPE_REFUND_COLUMNS_SQL } from "../pms-read-tail-r22-sql";
function repository(db: SqlDatabase): PmsRecordDetailsRepository {
    const financial = pmsRecordDetailFacts(db);
    return {
        refund: (facts: {
            id: string;
            companyId: string;
            kind: "booking" | "counter";
        }) => db.prepare(facts.kind === 'booking' ? 'SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE r.id=? AND p.company_id=?' : `${PMS_STRIPE_REFUND_COLUMNS_SQL} WHERE id=? AND company_id=? AND refund_state IS NOT NULL`).bind(facts.id, facts.companyId).first<Record<string, string | number | null>>(),
        document: (facts: {
            documentId: string;
        }) => nativePmsReadFirst<{
            property_id: string;
            snapshot_json: string;
        }>(PMS_RD_DOCUMENT_SNAPSHOT, [facts.documentId], db),
        controlRows: facts => financial.controlRows(facts),
        controlBalance: facts => financial.controlBalance(facts), hotel: (actor, id) => loadPms(db, actor, id), hotels: (actor, id) => pmsHotelOverview(db, actor, id)
    };
}
const application = createPmsRecordDetails({ repository: async (actor) => repository((await holdingResources(actor.holdingId)).write), clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export const pmsRefundRecord = application.pmsRefundRecord;
export const pmsSnapshotEntry = application.pmsSnapshotEntry;
export const pmsControlSources = application.pmsControlSources;
