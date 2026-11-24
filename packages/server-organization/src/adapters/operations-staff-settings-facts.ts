import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { nativeOrgStatement } from "./native-organization-owner-write";
import { twoColumnLookup, nodeKindByIdCompany } from './organization-column-alias-families-r27';
import { activeNodeKindExists } from './organization-column-alias-families-r34';
import { ownerVersionGuardSource } from './organization-read-tail-sources';
import { activeStaffLevelSource } from './organization-read-tail-sources-r39';
export function StaffSettingsOrganizationFacts(database: SqlDatabase, companyId = '') {
    return { readListLevelsEffect(companyId: string) {
            return database.prepare("SELECT * FROM staff_levels WHERE company_id=? ORDER BY level_number ASC NULLS LAST,rank DESC,code")
                .bind(companyId)
                .all<{
                id: string;
                company_id: string;
                code: string;
                name: string;
                rank: number;
                level_number: number | null;
                active: number;
                version: number;
            }>();
        },
        readSaveLevelPrevious(id: string, companyId: string) { return database.prepare(twoColumnLookup('level_number','staff_levels','id','company_id')).bind(id, companyId).first<{
            level_number: number | null;
        }>(); },
        guard5(key: string, value: "level-version", id: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, ownerVersionGuardSource('staff_levels'), [id, companyId, expectedVersion]);
        },
        stageSaveLevelEffect(code: string, name: string, rank: number, value: 0 | 1, value2: number | null, now: number, id: string): Promise<SqlStatement> {
            return nativeOrgStatement(database, companyId, 'staff_level_update', [code, name, rank, value, value2, now, id], `staff-level:${id}:${now}`, now);
        },
        stageSaveLevelEffect2(recordId: string, ownerCompanyId: string, code: string, name: string, rank: number, value: 0 | 1, value2: number | null, now: number, now2: number): Promise<SqlStatement> {
            return nativeOrgStatement(database, ownerCompanyId, 'staff_level_insert', [recordId, ownerCompanyId, code, name, rank, value, value2, now, now2], `staff-level:${recordId}`, now);
        },
        guard8(key: string, value: "level-active", levelId: string, companyId: string) {
            return assertion(database, key, value, `EXISTS(${activeStaffLevelSource})`, [levelId, companyId]);
        },
        readSetDepartmentHeadDepartment(id: string, companyId: string) {
            return database.prepare(nodeKindByIdCompany('branch_id','department'))
                .bind(id, companyId)
                .first<{
                branch_id: string | null;
            }>();
        },
        guard14(key: string, value: "department", id: string, companyId: string) {
            return assertion(database, key, value, activeNodeKindExists('department'), [id, companyId]);
        },
        guard15(key: string, value: "head-version", expectedVersion: number, id: string, id2: string, expectedVersion2: number) {
            return assertion(database, key, value, "(?=0 AND NOT EXISTS(SELECT 1 FROM department_heads WHERE department_id=?)) OR EXISTS(SELECT 1 FROM department_heads WHERE department_id=? AND version=?)", [expectedVersion, id, id2, expectedVersion2]);
        },
        stageSetDepartmentHeadEffect(id: string, ownerCompanyId: string, personId: string, value: number, userId: string, now: number): Promise<SqlStatement> {
            return nativeOrgStatement(database, ownerCompanyId, 'staff_department_head_upsert', [id, ownerCompanyId, personId, value, userId, now], `staff-department-head:${id}`, now);
        } };
}
