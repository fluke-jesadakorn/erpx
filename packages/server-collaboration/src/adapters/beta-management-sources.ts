import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {socialTaskOriginByPost,socialAiMonthlyUsageCount} from './read-tail-sql';
export function betaManagementCollaborationSources(db:SqlDatabase){return {
 aiUsageSource:socialAiMonthlyUsageCount('input_tokens,output_tokens'),
 draftFileOwner:'EXISTS(SELECT 1 FROM social_post_drafts p WHERE p.id=a.owner_id AND p.company_id=?)',
 async taskOrigin(id:string){return db.prepare(socialTaskOriginByPost).bind(id).first<{task_id:string}>();},
};}
