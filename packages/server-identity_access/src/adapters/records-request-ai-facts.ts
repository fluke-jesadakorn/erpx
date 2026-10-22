import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function RequestAiIdentityAccessFacts(database: SqlDatabase) {
    return { readRunRequestAiGrant(assignmentId: string, userId: string, companyId: string, role: string, value: number | null, value2: number | null) {
            return database.prepare("SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")
                .bind(assignmentId, userId, companyId, role, value, value2)
                .first<{
                role: string;
            }>();
        },
        guard13(command: string, value: "queued-assignment", assignmentId: string, userId: string, role: string, value2: number | null, value3: number | null, now: number, now2: number) { return assertion(database, command, value, "EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)", [assignmentId, userId, role, value2, value3, now, now2]); } };
}
