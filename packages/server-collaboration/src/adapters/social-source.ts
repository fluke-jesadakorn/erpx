import { sourceAccessSql } from '@erpx/server-collaboration/public/social-source-policy';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from '@erpx/server-runtime/public/http';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';

import type { PostRow } from '@erpx/server-collaboration/domain/social-source';

import { createSocialSourceApplication, type SocialSourceRepository } from '@erpx/server-collaboration/application/social-source';
import { BusinessRuleError } from '@erpx/server-shared/index';

import {workSocialReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';

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
