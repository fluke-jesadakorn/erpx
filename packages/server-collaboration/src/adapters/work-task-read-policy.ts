import 'server-only';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {workSocialReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
import {socialTaskOriginGuard} from './read-tail-sql';

/** A Task consumer repeats the included conversation source gate, including
 * generic updates whose native audience permits a linked preview. Paid Social
 * keeps its existing audience policy. The caller separately retains the
 * original Task participant/sourceAccessSql policy and owner purchase. */
async function scopeFor(database:SqlDatabase,actor:StaffActor,taskId?:string){
 const scope=await workSocialReadScope(database,actor);
 if(!scope.prefix)return {where:'TRUE',args:[] as (string|number|null)[]};
 const args=scope.args.map(value=>{if(value===null||typeof value==='string'||typeof value==='number')return value;throw new Error('WORK_TASK_SOCIAL_READ_VALUE_UNSUPPORTED');});
 return {where:socialTaskOriginGuard('NOT EXISTS(','task_read_origin',`\n LEFT JOIN social_posts p ON p.id=task_read_origin.post_id AND p.company_id=${taskId===undefined?'t.company_id':'?'}\n WHERE task_read_origin.task_id=${taskId===undefined?'t.id':'?'} AND (p.id IS NULL OR p.state<>'published'\n OR NOT EXISTS(${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE ${scope.where}\n AND ${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})})))`),args:taskId===undefined?args:[actor.companyId,taskId,...args]};
}
/** Fixed native Task alias t. Every persisted conversation origin must still
 * pass its own included-owner or purchased Social and original audience policy
 * in the consuming SELECT, before its LIMIT. No caller SQL is accepted. */
export function workTaskSocialReadScope(database:SqlDatabase,actor:StaffActor){return scopeFor(database,actor);}
export function workTaskSocialReadPolicy(database:SqlDatabase,actor:StaffActor,taskId:string){return scopeFor(database,actor,taskId);}
