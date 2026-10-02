import {captureJobAuthority} from "./iam-jobs";
import { z } from "zod";
import { ASSISTANT_ID, hashtags, validMentionSpan, type Mention, type Suggestion } from "@/lib/social-text";
import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { runtimeEnv } from "./runtime";
import { getRecord, searchReferences } from "./records";
import { ApiError } from "./http";
import { sourceAccessSql } from "./social-source";
export const mentionsSchema = z.array(z.object({ kind: z.enum(["person", "record", "agent"]), targetId: z.string().min(1).max(260), label: z.string().min(1).max(160), start: z.number().int().min(0), end: z.number().int().positive() })).max(20);
export async function mentionPeople(db: SqlDatabase, actor: Pick<WorkContext,"companyId">) {
  const now = Date.now();
  const ids = (await db.prepare("SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(actor.companyId, now, now).all<{user_id:string}>()).results;
  if (!ids.length) return [];
  const env = await runtimeEnv();
  return (await env.CONTROL_DB.prepare(`SELECT user_id,display_name FROM principals WHERE status='active' AND user_id IN (${ids.map(()=>"?").join(",")}) ORDER BY display_name,user_id`).bind(...ids.map(p=>p.user_id)).all<{user_id:string;display_name:string}>()).results;
}
export async function textSuggestions(db: SqlDatabase, actor: WorkContext, trigger: string, q: string): Promise<Suggestion[]> {
  if (trigger === "#") {
    const now=Date.now();
    return (await db.prepare(`SELECT DISTINCT t.tag FROM social_topics t WHERE lower(t.tag) LIKE ? AND EXISTS(SELECT 1 FROM social_posts p WHERE p.company_id=? AND p.state='published' AND ${sourceAccessSql()} AND (EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=t.tag) OR EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=t.tag))) ORDER BY t.tag LIMIT 12`).bind(`${q.normalize("NFC").toLowerCase().replace(/[\\%_]/g,"\\$&")}%`,actor.companyId,actor.userId,now,now).all<{tag:string}>()).results.map(r=>({kind:"tag",targetId:r.tag,label:r.tag,detail:"Hashtag"}));
  }
  const needle=q.toLowerCase();
  const people=(await mentionPeople(db,actor)).filter(p=>p.display_name.toLowerCase().includes(needle)).slice(0,8).map(p=>({kind:"person" as const,targetId:p.user_id,label:p.display_name,detail:`Person · ${p.user_id}`}));
  const aiName = (await db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(actor.companyId).first<{home_ai_name:string}>())?.home_ai_name ?? "AI";
  const agents = ["ai","erpx assistant",aiName.toLowerCase()].some(name=>name.includes(needle)) ? [{kind:"agent" as const,targetId:ASSISTANT_ID,label:aiName,detail:"AI · replies after publishing"}] : [];
  const records=(await searchReferences(actor,q ? `@${q}` : "@")).filter((r): r is {kind:string;module_id:string;id:string;title:string}=>r.kind==="record").slice(0,8).map(r=>({kind:"record" as const,targetId:`${r.module_id}/${r.id}`,label:String(r.title),detail:`${r.module_id} · ${r.id}`}));
  return [...people,...agents,...records];
}
export async function validateMentions(db: SqlDatabase, actor: WorkContext, body: string, input: Mention[] = []) {
  const mentions=mentionsSchema.parse(input).sort((a,b)=>a.start-b.start);
  const people=mentions.some(m=>m.kind==="person") ? await mentionPeople(db,actor) : [];
  let end=0;
  for(const m of mentions) {
    if(m.start<end || m.end>body.length || !validMentionSpan(body,m)) throw new ApiError(422,"MENTION_CHANGED","Choose the mention again after editing its text.");
    end=m.end;
    let label: string | undefined;
    if(m.kind==="person") label=people.find(p=>p.user_id===m.targetId)?.display_name;
    else if(m.kind==="agent" && m.targetId===ASSISTANT_ID) {
      const current=(await db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(actor.companyId).first<{home_ai_name:string}>())?.home_ai_name ?? "AI";
      if([current,"AI","ERPX assistant"].includes(m.label)) label=m.label;
      else {
        const old=await db.prepare("SELECT 1 FROM ai_home_name_history WHERE company_id=? AND name=?").bind(actor.companyId,m.label).first();
        if(old) label=m.label;
      }
    }
    else if(m.kind==="record") { const [module,...ids]=m.targetId.split("/"); label=(await getRecord(actor,module,ids.join("/"))).title; }
    if(!label) throw new ApiError(422,"MENTION_UNAVAILABLE","A mention is no longer available. Remove it or choose another target.");
    if(label!==m.label) throw new ApiError(422,"MENTION_RENAMED","A mention has been renamed. Choose it again.");
  }
  return mentions;
}
export async function textStatements(db:SqlDatabase, actor:WorkContext, postId:string, ownerType:"post"|"comment", ownerId:string, body:string, mentions:Mention[]) {
  const now=Date.now(), steps=[];
  if(mentions.some(m=>m.kind==="agent"))actor=await captureJobAuthority(db,actor);
  for(const m of new Map(mentions.map(m=>[`${m.kind}:${m.targetId}`,m])).values()) {
    steps.push(db.prepare("INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(ownerType,ownerId,m.kind,m.targetId,m.label,actor.userId,now));
    if(m.kind==="person" && m.targetId!==actor.userId) steps.push(db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,p.company_id,?,'post',p.id,p.version,'You were mentioned in a conversation',? FROM social_posts p WHERE p.id=? AND ${sourceAccessSql()}`).bind(crypto.randomUUID(),m.targetId,now,postId,m.targetId,now,now));
    if(m.kind==="agent") steps.push(db.prepare("INSERT INTO social_agent_replies(id,post_id,owner_type,owner_id,company_id,actor_id,actor_json,requester_name,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`agent-${ownerId}`,postId,ownerType,ownerId,actor.companyId,actor.userId,JSON.stringify({userId:actor.userId,assignmentId:actor.assignmentId,role:actor.role,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,iam:actor.iam,authority:actor.authority}),actor.name,now,now));
  }
  for(const tag of new Set(hashtags(body).filter(t=>!mentions.some(m=>t.start<m.end&&t.end>m.start)).map(t=>t.tag))) {
    steps.push(db.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(tag,actor.userId,now));
    steps.push(db.prepare(`INSERT INTO ${ownerType==="post"?"social_post_topics(post_id":"social_comment_topics(comment_id"},tag) VALUES(?,?) ON CONFLICT DO NOTHING`).bind(ownerId,tag));
  }
  return steps;
}
