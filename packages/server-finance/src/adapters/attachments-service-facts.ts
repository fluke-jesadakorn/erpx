import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeFinanceAttachmentVersionGuard, nativeFinanceOwnerWriteSystem } from "./native-finance-owner-write";

/**
 * The invoice-payment/supplier-bill attachment version guard is owned by the
 * native Finance port. This adapter only carries typed facts to that port; it
 * authors no guard SQL and has no TypeScript fallback.
 */
export function attachmentServiceFinanceFacts(db: SqlDatabase) {
    return {
        versionGuard: (
            commandId: string,
            targetType: string,
            targetId: string,
            actor: WorkContext,
            version: number,
            index: number,
        ) => {
            // Routing discriminator only; the native owner owns the allow-list.
            if (targetType !== "invoice_payment" && targetType !== "supplier_bill") return null;
            if (!commandId || !actor || index < 0)
                throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This file target has no atomic version guard.");
            return nativeFinanceAttachmentVersionGuard({
                companyId: actor.companyId,
                targetType,
                targetId,
                expectedVersion: version,
                userId: actor.userId,
                assignmentId: actor.assignmentId,
                role: actor.role,
                scopeKind: actor.assignmentScope,
            });
        },
        versionStep: (targetType: string, targetId: string, expectedVersion: number, now: number) => {
            if (targetType !== "invoice_payment" && targetType !== "supplier_bill") return null;
            // Native Finance owner-write executes the version step in Rust with
            // the guard, receipt/audit/outbox in the caller unit. The reply is
            // an inert sequencing placeholder; no write SQL remains in TypeScript.
            return nativeFinanceOwnerWriteSystem(
                db,
                "system",
                `finance_attachment_version_step_${targetType}`,
                [now, targetId, expectedVersion],
                `avs-${targetId}-${expectedVersion}`,
                now,
            );
        },
    };
}
