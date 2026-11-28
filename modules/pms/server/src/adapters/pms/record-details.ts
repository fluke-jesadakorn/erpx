import "server-only";
import { holdingResources } from "../../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { loadPms } from "../../public/pms";
import { pmsHotelOverview } from "@erpx/pms-server/adapters/pms/home";
import { pmsRecordDetailFacts } from "@erpx/finance-server/public/pms-record-details-facts";
import { createPmsRecordDetails, type PmsRecordDetailsRepository } from "@erpx/pms-server/application/record-details";
import { nativePmsReadFirst, PMS_RD_DOCUMENT_SNAPSHOT } from "../native-pms-read";
import { PMS_STRIPE_REFUND_COLUMNS_SQL } from "../pms-read-tail-r22-sql";
import { pmsRefundPaymentJoinHeadSql } from "../pms-read-tail-r34-sql";
function repository(db: SqlDatabase): PmsRecordDetailsRepository {
    const financial = pmsRecordDetailFacts(db);
    return {
        refund: (facts: {
            id: string;
            companyId: string;
            kind: "booking" | "counter";
        }) => db.prepare(facts.kind === 'booking' ? `${pmsRefundPaymentJoinHeadSql('detail')} WHERE r.id=? AND p.company_id=?` : `${PMS_STRIPE_REFUND_COLUMNS_SQL} WHERE id=? AND company_id=? AND refund_state IS NOT NULL`).bind(facts.id, facts.companyId).first<Record<string, string | number | null>>(),
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
