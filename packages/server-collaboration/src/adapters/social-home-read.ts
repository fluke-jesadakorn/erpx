
import 'server-only';

import type { StaffActor } from '@erpx/server-operations/public/staff-policy';

import { type PostDraft } from '@erpx/server-collaboration/domain/social-home';

import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

import { readablePost } from '@erpx/server-collaboration/public/social-source';

import { BusinessRuleError } from '@erpx/server-shared/index';
import { ApiError } from '@erpx/server-runtime/public/http';




import {requireWorkSocialDraftRead} from '@erpx/server-collaboration/adapters/work-social-draft-policy';
import{createSocialUploadRead,type createSocialUploadReadPorts}from '@erpx/server-collaboration/application/social-home';

export function nativeSocialUploadReadRepository(db:SqlDatabase):createSocialUploadReadPorts<SqlStatement>['repository']{return{async draft(id: string, companyId: string) {
            return db
                .prepare("SELECT * FROM social_post_drafts WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<PostDraft>();
        },async publishedComment(published_comment_id: string, comment_parent_post_id: string) { return db.prepare("SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND state='published'").bind(published_comment_id, comment_parent_post_id).first(); }};}
function application(db:SqlDatabase){return createSocialUploadRead({repository:{draft:(...args)=>nativeSocialUploadReadRepository(db).draft(...args),publishedComment:(...args)=>nativeSocialUploadReadRepository(db).publishedComment(...args)},readablePost: (...args) => inward(() => readablePost(db, ...args))});}
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function socialUploadOwner(db: SqlDatabase, actor: StaffActor, id: string, edit = false) { const result=await exposed(() => application(db).socialUploadOwner(actor, id, edit));await requireWorkSocialDraftRead(db,actor,id,edit);return result; }
