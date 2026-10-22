import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function peopleDirectoryIdentityFacts(database: SqlDatabase) {
    return { async readSaveDirectoryPersonAccount(linkedMemberUserId: string) {
            const env = await runtimeEnv();
            return env.CONTROL_DB.prepare('SELECT id,email FROM "user" WHERE id=?')
                .bind(linkedMemberUserId)
                .first<{
                id: string;
                email: string;
            }>();
        },
        guard7(commandId: string, value: "member-link", id: string, companyId: string, now: number, now2: number, id2: string, companyId2: string, id3: string) {
            return assertion(database, commandId, value, `EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?) AND NOT EXISTS(SELECT 1 ${peoplepayrollSources.people_directory_facts_hr_people_from} WHERE user_id=? AND company_id=? AND id!=? AND merged_into_id IS NULL)`, [
                id,
                companyId,
                now,
                now2,
                id2,
                companyId2,
                id3,
            ]);
        },
        stageSetDirectoryStatusEffect4(now: number, user_id: string | number, companyId: string) {
            return database.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL")
                .bind(now, user_id, companyId);
        },
        async readPersonAccessAccount(user_id: string | number) {
            const env = await runtimeEnv();
            return env.CONTROL_DB.prepare('SELECT id,name,email FROM "user" WHERE id=?')
                .bind(user_id)
                .first();
        },
        readPersonAccessAssignments(user_id: string | number, companyId: string) {
            return database.prepare("SELECT id,role,scope_kind,scope_node_id,company_id,valid_until,revoked_at FROM assignments WHERE user_id=? AND company_id=? ORDER BY role")
                .bind(user_id, companyId)
                .all<Record<string, string | number | null>>();
        } };
}
