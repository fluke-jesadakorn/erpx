import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeFinanceAttachmentVersionGuard, nativeFinanceAttachmentVersionStep } from "./native-finance-owner-write";

/**
 * The invoice-payment/supplier-bill attachment version guard is owned by the
 * native Finance port. This adapter only carries typed facts to that port; it
 * authors no guard SQL and has no TypeScript fallback.
 */
export function attachmentServiceFinanceFacts(_db: SqlDatabase) {
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
        versionStep: (
            targetType: string,
            targetId: string,
            expectedVersion: number,
            now: number,
            context?: { actor: WorkContext; commandId: string },
        ) => {
            if (targetType !== "invoice_payment" && targetType !== "supplier_bill") return null;
            // The mounted native Finance version-step port executes the bump,
            // receipt, audit and outbox in the caller's serializable unit. This
            // adapter carries typed actor facts only; it authors no SQL and has
            // no TypeScript fallback.
            if (!context) throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "The native Finance attachment version step requires an authorized actor.");
            return nativeFinanceAttachmentVersionStep({
                companyId: context.actor.companyId,
                targetType,
                targetId,
                expectedVersion,
                now,
                userId: context.actor.userId,
                assignmentId: context.actor.assignmentId,
                role: context.actor.role,
                scopeKind: context.actor.assignmentScope,
                key: context.commandId,
            });
        },
    };
}
