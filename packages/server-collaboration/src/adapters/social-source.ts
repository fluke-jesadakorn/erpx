import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from '@erpx/server-runtime/public/http';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { handoverSourcesReadableSql } from '@erpx/server-operations/public/handover-policy';
import { pmsPostSourcesReadableSql } from '@erpx/server-pms/public/pms/source-policy';
import type { PostRow } from '@erpx/server-collaboration/domain/social-source';
import { socialScopeKind } from '@erpx/server-collaboration/domain/social-source';
import { createSocialSourceApplication, type SocialSourceRepository } from '@erpx/server-collaboration/application/social-source';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { socialAssignmentAccessSql } from '@erpx/server-identity_access/public/collaboration-social-authorization';
import { socialProjectAudienceSql } from '@erpx/server-projects/public/project-read-projections';
import {workSocialReadScope} from '@erpx/server-collaboration/adapters/work-social-purchases';
export function sourceAccessSql(alias = 'p', currentAssignment?: string,viewer?:{userId:string;now:string}) {
    const audience = `    ${alias}.audience_kind IS NULL OR ${alias}.audience_kind IN ('company','branch','department')
    OR (${alias}.audience_kind='participants' AND EXISTS(
      SELECT 1 FROM social_post_participants sp WHERE sp.post_id=${alias}.id AND sp.user_id=a.user_id))
    OR (${alias}.audience_kind='team' AND EXISTS(
      SELECT 1 FROM messaging_teams mt JOIN messaging_team_members mm ON mm.team_id=mt.id
      WHERE mt.id=${alias}.audience_id AND mt.company_id=${alias}.company_id
      AND NOT mt.organization_managed AND mm.user_id=a.user_id))
    OR (${alias}.audience_kind='project' AND EXISTS(
      ${socialProjectAudienceSql(alias)}))
`;
    return socialAssignmentAccessSql(alias, currentAssignment ? `AND (NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${alias}.id AND ps.source_kind='pms_record') OR a.id=${currentAssignment})` : '', audience, `${handoverSourcesReadableSql(alias, 'a.user_id')} AND ${pmsPostSourcesReadableSql(alias, 'a.user_id', 'a.id')}`,viewer);
}
export function socialPostScope(actor: Pick<WorkContext, "companyId" | "userId" | "branchId" | "departmentId"> & Partial<Pick<WorkContext, "assignmentId">>, alias = "p") {
    const now = Date.now(), base = `${alias}.company_id=? AND ${alias}.state='published' AND ${sourceAccessSql(alias, actor.assignmentId ? "?::text" : undefined)}`;
    const values = [actor.companyId, actor.userId, now, now, ...(actor.assignmentId ? [actor.assignmentId] : [])];
    return socialScopeKind(actor) === 'department'
        ? { sql: `${base} AND (${alias}.department_id=? OR (${alias}.department_id IS NULL AND (${alias}.branch_id IS NULL OR ${alias}.branch_id=?)))`, bindings: [...values, actor.departmentId, actor.branchId] }
        : { sql: `${base} AND (${alias}.branch_id IS NULL OR ?::text IS NULL OR ${alias}.branch_id=?)`, bindings: [...values, actor.branchId, actor.branchId] };
}
function application(db: SqlDatabase,purchase:Awaited<ReturnType<typeof workSocialReadScope>>={prefix:'',where:'TRUE',args:[]}) {
    const repository: SocialSourceRepository<SqlStatement> = { async publishedPost(id: string, companyId: string, userId: string, now: number, now2: number, value: string[]) {
            return db.prepare(`${purchase.prefix}SELECT p.* FROM social_posts p WHERE p.id=? AND p.company_id=? AND p.state='published' AND ${sourceAccessSql("p", value.length ? "?::text" : undefined)} AND ${purchase.where}`).bind(...purchase.args,id, companyId, userId, now, now2, ...value).first<PostRow>();
        },
        guardPublishedSource(key: string, step: string, id: string, company_id: string, version: number, user: string, now: number, now2: number) {
            return assertion(db, key, step, `EXISTS(SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND p.version=? AND p.state='published' AND ${sourceAccessSql()})`, [id, company_id, version, user, now, now2]);
        },
        async selectedComments(id: string, ids: string[]) {
            return db
                .prepare(`SELECT id,body,author_name,version FROM social_comments WHERE post_id=? AND state='published' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`)
                .bind(id, ...ids)
                .all<{
                id: string;
                body: string;
                author_name: string;
                version: number;
            }>();
        } };
    return createSocialSourceApplication({ repository, clock: { now: () => Date.now() }, sha256 });
}
async function exposed<Result>(work: () => Promise<Result>) {
    try {
        return await work();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function readablePost(db: SqlDatabase, actor: Pick<StaffActor, "userId" | "companyId"> & Partial<Pick<StaffActor, "assignmentId">>, id: string) { return exposed(async() => application(db,await workSocialReadScope(db,actor)).readablePost(actor, id)); }
export function sourceGuard(db: SqlDatabase, key: string, post: PostRow, user: string, step: string) { return application(db).sourceGuard(key, post, user, step); }
export async function sourceSnapshot(db: SqlDatabase, actor: Pick<StaffActor, "userId" | "companyId">, id: string, commentIds: string[]) { return exposed(async() => application(db,await workSocialReadScope(db,actor)).sourceSnapshot(actor, id, commentIds)); }
