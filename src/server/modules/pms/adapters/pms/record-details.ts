import "server-only";
import { holdingResources } from "../../../runtime/public/runtime";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import { requirePermission } from "../../../identity_access/public/iam-permissions";
import { ApiError } from "../../../runtime/public/http";
import { loadPms } from "../../public/pms";
import { pmsHotelOverview } from "./home";
import { pmsRecordDetailFacts } from "../../../finance/public/pms-record-details-facts";
import { createPmsRecordDetails, type PmsRecordDetailsRepository } from "../../application/record-details";
function repository(db: SqlDatabase): PmsRecordDetailsRepository {
    const financial = pmsRecordDetailFacts(db);
    return {
        refund: (facts: {
            id: string;
            companyId: string;
            kind: "booking" | "counter";
        }) => db.prepare(facts.kind === 'booking' ? 'SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE r.id=? AND p.company_id=?' : 'SELECT id,property_id,folio_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments WHERE id=? AND company_id=? AND refund_state IS NOT NULL').bind(facts.id, facts.companyId).first<Record<string, string | number | null>>(),
        document: (facts: {
            documentId: string;
        }) => db.prepare('SELECT property_id,snapshot_json FROM pms_documents WHERE id=?').bind(facts.documentId).first<{
            property_id: string;
            snapshot_json: string;
        }>(),
        controlRows: facts => financial.controlRows(facts),
        controlBalance: facts => financial.controlBalance(facts), hotel: (actor, id) => loadPms(db, actor, id), hotels: (actor, id) => pmsHotelOverview(db, actor, id)
    };
}
const application = createPmsRecordDetails({ repository: async (actor) => repository((await holdingResources(actor.holdingId)).write), clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message) });
export const pmsRefundRecord = application.pmsRefundRecord;
export const pmsSnapshotEntry = application.pmsSnapshotEntry;
export const pmsControlSources = application.pmsControlSources;
