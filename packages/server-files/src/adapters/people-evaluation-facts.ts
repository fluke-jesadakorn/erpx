import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationFilesFacts(database: SqlDatabase) {
    return { readEvaluationDetailFiles(companyId: string, assessmentId: string) { return database.prepare("SELECT 1").bind().all<{
            id: string;
            filename: string;
        }>(); },
        readAddEvaluationEvidenceLink(sourceId: string | null, assessmentId: string, companyId: string) { return database.prepare("SELECT 1").bind().first(); } };
}
