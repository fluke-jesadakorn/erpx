import 'server-only';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createCommerceDeliveryApplication, type CommerceDeliveryUnit, type Row } from "@erpx/server-commerce/application/commerce-delivery-evidence";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { returnCompletionValues } from "@erpx/server-commerce/public/commerce-return-accounting";
import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";
function unit(db: SqlDatabase): CommerceDeliveryUnit<SqlStatement> {
    return {
        readCommerceOrders1: (facts: {
            orderId: string;
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            departmentId: string | null;
            departmentId2: string | null;
        }) => db.prepare("SELECT * FROM commerce_orders WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND kind='sales' AND state IN ('confirmed','partially_fulfilled')").bind(facts.orderId, facts.companyId, facts.branchId, facts.branchId2, facts.departmentId, facts.departmentId2).first<Row>(),
        guardRule1: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND kind='sales' AND state IN ('confirmed','partially_fulfilled') AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))", facts.values)
    };
}
const application = createCommerceDeliveryApplication<SqlStatement>({ clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), hash: sha256, forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), forContext: async () => { throw new Error("No registry context for this service"); }, attachmentReadable: attachmentsV2Readable, returnCompletionValues });
export function deliveryEvidenceGuard(db: SqlDatabase, actor: WorkContext, orderId: string, expectedVersion: number, key: string) { return application.deliveryEvidenceGuard(unit(db), actor, orderId, expectedVersion, key); }
