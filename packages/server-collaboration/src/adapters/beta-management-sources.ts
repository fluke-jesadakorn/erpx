import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {socialTaskOriginByPost} from './read-tail-sql';
export function betaManagementCollaborationSources(db:SqlDatabase){return {
 aiUsageSource:'SELECT input_tokens,output_tokens FROM social_ai_usage WHERE company_id=? AND month=?',
 draftFileOwner:'EXISTS(SELECT 1 FROM social_post_drafts p WHERE p.id=a.owner_id AND p.company_id=?)',
 async taskOrigin(id:string){return db.prepare(socialTaskOriginByPost).bind(id).first<{task_id:string}>();},
};}
