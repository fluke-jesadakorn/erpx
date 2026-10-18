import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {recordAttachmentVersionSteps} from './record-version-receipts';
type Row=Record<string,unknown>;
type Graph={attachments:Row[];uploads:Row[];sets:Row[];links:Row[];events:Row[];receipts:Row[];audits:Row[];publications:Row[];steps:Row[]};
export type SocialUploadCleanupFiles=Readonly<{kind:'social-upload-cleanup-files'}>;
const witnesses=new WeakMap<SocialUploadCleanupFiles,{database:SqlDatabase;input:{id:string;companyId:string;userId:string;version:number};graph:Graph;image:string}>();
function changed(){return new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original unused private upload changed. Reload before cleaning it up.');}
function canonical(value:unknown):unknown {return Array.isArray(value)?value.map(canonical):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>[k,canonical(v)])):value;}
const image=(value:unknown)=>createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex');
export function socialUploadCleanupFileLocks(db:SqlDatabase){return db.prepare('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE');}
async function graph(db:SqlDatabase,input:{id:string;companyId:string}){
 const raw=await db.prepare(`WITH owned_sets AS(SELECT * FROM attachment_sets WHERE target_type='social_draft' AND target_id=?),
 uploads AS(SELECT * FROM upload_intents WHERE owner_type='social_draft' AND owner_id=?),
 files AS(SELECT * FROM attachments a WHERE a.owner_type='social_draft' AND a.owner_id=? OR EXISTS(SELECT 1 FROM uploads u WHERE u.id=a.upload_id)),
 direct_links AS(SELECT * FROM attachment_links l WHERE l.set_id IN(SELECT id FROM owned_sets) OR l.attachment_id IN(SELECT id FROM files) OR l.upload_intent_id IN(SELECT id FROM uploads)),
 links AS(SELECT * FROM attachment_links l WHERE l.id IN(SELECT id FROM direct_links) OR l.supersedes_link_id IN(SELECT id FROM direct_links)),
 sets AS(SELECT * FROM attachment_sets s WHERE s.id IN(SELECT id FROM owned_sets UNION SELECT set_id FROM links))
 SELECT jsonb_build_object('attachments',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM files a),'[]'::jsonb),
 'uploads',COALESCE((SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM uploads u),'[]'::jsonb),
 'sets',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM sets s),'[]'::jsonb),
 'links',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM links l),'[]'::jsonb),
 'events',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM attachment_events e WHERE e.attachment_id IN(SELECT id FROM files) OR e.link_id IN(SELECT id FROM links) OR e.set_id IN(SELECT id FROM sets)),'[]'::jsonb),
 'receipts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.command_id) FROM command_receipts c WHERE c.command_id IN(SELECT bound_command_id FROM sets)),'[]'::jsonb),
 'audits',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a WHERE a.command_id IN(SELECT bound_command_id FROM sets)),'[]'::jsonb),
 'steps',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.next_version,s.command_id) FROM (${recordAttachmentVersionSteps}) s WHERE s.company_id=? AND s.target_type='social_draft' AND s.target_id=?),'[]'::jsonb),
 'publications',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM attachment_publications p WHERE p.source_link_id IN(SELECT id FROM links)),'[]'::jsonb))::text image`).bind(input.id,input.id,input.id,input.companyId,input.id).first<string>('image');
 if(!raw)throw changed();return JSON.parse(raw) as Graph;
}
/** Negative cleanup only: these metadata never grant bytes, binding or a new
 * target. A same-draft original compatibility bind is not post consumption. */
export async function beginSocialUploadCleanupFiles(db:SqlDatabase,input:{id:string;companyId:string;userId:string;version:number}){
 const g=await graph(db,input);if(g.publications.length)throw changed();
 // Legacy sources have no Company/set columns populated. Admit only their
 // exact original draft owner and uploader, never a linked/shared null source.
 const legacy=(u:Row)=>u.company_id===null&&u.attachment_set_id===null&&u.attachment_link_id===null;
 for(const u of g.uploads)if(u.owner_type!=='social_draft'||u.owner_id!==input.id||u.created_by!==input.userId||u.company_id!==input.companyId&&!legacy(u))throw changed();
 for(const a of g.attachments){const upload=g.uploads.find(u=>u.id===a.upload_id);if(a.owner_type!=='social_draft'||a.owner_id!==input.id||a.created_by!==input.userId||!upload||a.company_id!==input.companyId&&!(a.company_id===null&&legacy(upload)&&!g.links.some(l=>l.attachment_id===a.id||l.upload_intent_id===upload.id)))throw changed();}
 for(const s of g.sets){if(s.company_id!==input.companyId||s.target_type!=='social_draft'||s.target_id!==input.id||s.created_by!==input.userId||s.purpose_key!=='post_image'||s.component_kind!=='record'||!['draft','bound'].includes(String(s.state)))throw changed();
  if(s.state==='bound'){const step=g.steps.find(row=>row.command_id===s.bound_command_id&&Number(row.next_version)===Number(s.expected_target_version)+1),receipt=g.receipts.find(row=>row.command_id===s.bound_command_id);if(!step||Number(step.next_version)>input.version||!receipt||receipt.operation!=='attachment.compatibility.bind'||receipt.actor_id!==input.userId||receipt.record_id!==input.id||!g.audits.some(a=>a.command_id===s.bound_command_id&&a.actor_id===input.userId&&a.record_id===input.id&&Number(a.record_version)===Number(step.next_version)))throw changed();const result=JSON.parse(String(receipt.result_json)) as Row;if(result.attachmentSetId!==s.id||Number(result.ownerVersion)!==Number(step.next_version)||!g.links.some(l=>l.id===result.attachmentLinkId&&l.attachment_id===result.id&&l.set_id===s.id))throw changed();}
 }
 for(const l of g.links){const set=g.sets.find(s=>s.id===l.set_id);if(!set||l.created_by!==input.userId||l.supersedes_link_id!==null||!['draft','bound'].includes(String(l.state))||l.attachment_id!==null&&!g.attachments.some(a=>a.id===l.attachment_id)||l.upload_intent_id!==null&&!g.uploads.some(u=>u.id===l.upload_intent_id))throw changed();if(l.state==='bound'&&(set.state!=='bound'||l.binding_type!=='legacy_upload_adapter'||l.binding_id!==input.id))throw changed();}
 const witness=Object.freeze({kind:'social-upload-cleanup-files' as const});witnesses.set(witness,{database:db,input:{...input},graph:g,image:image(g)});return witness;
}
export async function assertSocialUploadCleanupFiles(db:SqlDatabase,witness:SocialUploadCleanupFiles,cancelledAt:number|null){const state=witnesses.get(witness);if(!state||state.database!==db)throw changed();const expected:Graph={...state.graph,attachments:state.graph.attachments.map(row=>row.revoked_at===null&&cancelledAt!==null?{...row,revoked_at:cancelledAt}:row),uploads:state.graph.uploads.map(row=>row.status==='ready'&&cancelledAt!==null?{...row,status:'expired',updated_at:cancelledAt}:row)};if(image(await graph(db,state.input))!==(cancelledAt===null?state.image:image(expected)))throw changed();}
