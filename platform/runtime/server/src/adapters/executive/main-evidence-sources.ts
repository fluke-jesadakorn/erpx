import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsAudit(actor: WorkContext, provider: string, metric: string, action: "search" | "open", fields: string[], queryHash: string, now: number, auditRows: Array<{ id: string; version: number | null }>): Promise<void> {
            await recordExecutiveEvidenceAccess({
                userId: actor.userId,
                companyId: actor.companyId,
                assignmentId: actor.assignmentId,
                provider,
                metric,
                action,
                fieldIds: JSON.stringify(fields),
                queryHash,
                records: auditRows.map((row) => ({ recordId: row.id, recordVersion: Number(row.version ?? 0) })),
            });
        } };
}
