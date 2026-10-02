import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { PostgresDatabase } from "./runtime/postgres";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { readAutomationInventory, applyDirectoryAction } from "./directory-provider";
import type { DirectoryInventory } from "@/lib/directory-operations";
import type { ProviderConnection } from "./iam-providers";
import { createHash,randomUUID } from "node:crypto";
import { hasPermission } from "@/lib/permissions";
import { requireFreshMfa } from "./iam-mfa";
import { effectiveGpoRules, gpoRuleSourceId } from "./gpo-rules";

const id=z.string().min(1).max(180);
export const positionGpoInput=z.object({
  positionId:id,expectedVersion:z.number().int().min(0),connectionId:id,gpoId:id,
  userFilterGroupId:id,computerTargetOuId:id,
  reason:z.string().trim().min(3).max(1000),
});
type Input=z.infer<typeof positionGpoInput>;
const previewToken=z.string().regex(/^[a-f0-9]{64}$/);
const previewDigest=(value:unknown)=>createHash("sha256").update(JSON.stringify(value)).digest("hex");
type Connection=ProviderConnection&{company_id:string;status:string;version:number};
function it(actor:WorkContext){
  if(actor.role!=="it_admin" || actor.assignmentScope!=="holding" || !hasPermission(actor,"iam.directory.apply"))
    throw new ApiError(403,"IT_AUTHORITY_REQUIRED","Choose an active Company IT assignment to publish GPO mappings.");
}
async function mutatingIt(actor:WorkContext){
  it(actor);
  if(!actor.authority?.sessionId)throw new ApiError(403,"FRESH_MFA_REQUIRED","Verify MFA before changing Directory access.");
  await requireFreshMfa({userId:actor.userId,sessionId:actor.authority.sessionId});
}
async function connection(db:SqlDatabase,companyId:string,connectionId:string){
  const row=await db.prepare("SELECT id,company_id,provider,config,status,version FROM iam_connections WHERE id=? AND company_id=? AND status='active'")
    .bind(connectionId,companyId).first<Connection>();
  if(!row)throw new ApiError(409,"DIRECTORY_CONNECTION_REQUIRED","Choose an active Directory connection.");
  return {...row,config:typeof row.config==="string"?JSON.parse(row.config):row.config};
}
async function writableConnection(db:SqlDatabase,companyId:string,connectionId:string){
  const c=await connection(db,companyId,connectionId);
  const state=await db.prepare("SELECT version,tested_version,write_enabled FROM people_directory_state WHERE company_id=? AND connection_id=?")
    .bind(companyId,connectionId).first<{version:number;tested_version:number|null;write_enabled:boolean}>();
  if(!state?.write_enabled||state.tested_version!==state.version)throw new ApiError(409,"DIRECTORY_WRITE_NOT_READY","Test and enable Directory writes before GPO delivery.");
  return c;
}
async function inventoryFor(db:PostgresDatabase,actor:WorkContext,input:Input){
  const c=await connection(db,actor.companyId,input.connectionId);
  const state=await db.prepare("SELECT version,write_enabled,tested_version FROM people_directory_state WHERE connection_id=? AND company_id=?")
    .bind(c.id,actor.companyId).first<{version:number;write_enabled:boolean;tested_version:number|null}>();
  if(!state?.write_enabled || state.tested_version!==state.version)throw new ApiError(409,"DIRECTORY_WRITE_NOT_READY","Test and enable Directory writes before publishing a position GPO.");
  const inventory=await readAutomationInventory(c);
  const find=(objectId:string,kind:string)=>{
    const obj=inventory.objects.find(o=>o.id===objectId&&o.kind===kind);
    if(!obj)throw new ApiError(422,"DIRECTORY_TARGET_INVALID",`Choose a current ${kind} from this connection.`);
    return obj;
  };
  const gpo=find(input.gpoId,"gpo"),group=find(input.userFilterGroupId,"group"),ou=find(input.computerTargetOuId,"ou");
  return {connection:c,state,inventory,gpo,group,ou};
}
export async function previewPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown){
  it(actor);
  const input=positionGpoInput.parse(raw);
  const position=await db.prepare("SELECT id,title FROM organization_positions WHERE id=? AND company_id=? AND active=1").bind(input.positionId,actor.companyId).first<{id:string;title:string}>();
  if(!position)throw new ApiError(404,"POSITION_NOT_FOUND","Active position not found.");
  const current=await db.prepare("SELECT * FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(actor.companyId,input.positionId).first<{version:number;revision:number;user_filter_group_id:string|null}>();
  if((current?.version??0)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The Position GPO changed. Reload before publishing.");
  const directory=await inventoryFor(db,actor,input);
  const holders=(await db.prepare(`SELECT e.id engagement_id,e.person_id,e.version,p.legal_name name,i.external_id account_id
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    LEFT JOIN LATERAL (SELECT DISTINCT i.external_id FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
        AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=? AND i.enabled=true) i ON true
    WHERE e.company_id=? AND e.position_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text
      AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND p.status='active' AND p.merged_into_id IS NULL
    ORDER BY p.legal_name,e.id,i.external_id`).bind(input.connectionId,actor.companyId,input.positionId).all<{engagement_id:string;person_id:string;version:number;name:string;account_id:string|null}>()).results;
  const deduplicated=new Map<string,typeof holders[number]&{accountStatus:"linked"|"awaiting_account_linkage"}>();
  for(const holder of holders){
    const key=`${holder.engagement_id}:${holder.account_id??"awaiting"}`;
    deduplicated.set(key,{...holder,accountStatus:holder.account_id?"linked":"awaiting_account_linkage"});
  }
  const holderRows=[...deduplicated.values()];
  const sharedMappings=(await db.prepare(`SELECT p.position_id,p.revision,p.user_filter_group_id,p.computer_target_ou_id
    FROM position_gpo_policies p JOIN organization_positions o ON o.company_id=p.company_id AND o.id=p.position_id AND o.active=1
    WHERE p.company_id=? AND p.connection_id=? AND p.gpo_id=? AND p.position_id<>?
    ORDER BY p.position_id`).bind(actor.companyId,input.connectionId,input.gpoId,input.positionId).all()).results;
  const result={position,version:input.expectedVersion+1,revision:(current?.revision??0)+1,
    directory:{connectionId:directory.connection.id,gpo:{id:directory.gpo.id,version:directory.gpo.version},userFilterGroup:{id:directory.group.id,version:directory.group.version},computerTargetOu:{id:directory.ou.id,version:directory.ou.version},observedAt:directory.inventory.observedAt},
    holders:holderRows,previousFilterGroupId:current?.user_filter_group_id??null,
    sharedMappings,
    changes:{directoryConfiguration:true,membersToAdd:holderRows.flatMap(h=>h.account_id?[h.account_id]:[]),membersToRemoveFromPreviousGroup:current?.user_filter_group_id && current.user_filter_group_id!==input.userFilterGroupId?holderRows.flatMap(h=>h.account_id?[h.account_id]:[]):[]},
    endpointStatus:"unconfirmed"};
  const directoryEvidence={connectionId:result.directory.connectionId,gpo:result.directory.gpo,
    userFilterGroup:result.directory.userFilterGroup,computerTargetOu:result.directory.computerTargetOu};
  return {...result,token:previewDigest({input,...result,directory:directoryEvidence})};
}
export async function publishPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){
  const parsed=positionGpoInput.extend({previewToken}).parse(raw),{previewToken:reviewedToken,...input}=parsed,operation="technology.position-gpo.publish",recordId=input.positionId;
  await mutatingIt(actor);
  const prior=await replayCommand(db,actor,key,operation,recordId,parsed);if(prior)return prior;
  const preview=await previewPositionGpo(db,actor,input),now=Date.now(),revision=preview.revision,version=preview.version;
  if(preview.token!==reviewedToken)throw new ApiError(409,"GPO_PREVIEW_CHANGED","Review the current Position mapping impact again.");
  const statements=[
    db.prepare("SELECT pg_advisory_xact_lock(821039320)"),
    assertion(db,key,"it-role","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='it_admin' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'iam.directory.apply'))",[actor.assignmentId,actor.userId,actor.companyId]),
    assertion(db,key,"gpo-version","(?=0 AND NOT EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=?)) OR EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=?)",[input.expectedVersion,actor.companyId,input.positionId,actor.companyId,input.positionId,input.expectedVersion]),
    assertion(db,key,"gpo-connection","EXISTS(SELECT 1 FROM iam_connections c JOIN people_directory_state s ON s.connection_id=c.id WHERE c.id=? AND c.company_id=? AND c.status='active' AND s.write_enabled=true AND s.tested_version=s.version)",[input.connectionId,actor.companyId]),
    db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?)`).bind(`position-gpo-${key}`,actor.companyId,input.positionId,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,preview.directory.gpo.version,JSON.stringify(preview.holders.map(h=>h.person_id)),actor.userId,input.reason,key,now),
    db.prepare(`INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_config_status,updated_at)
      VALUES(?,?,?,?,?,?,?,?,'pending',?) ON CONFLICT(company_id,position_id) DO UPDATE SET version=excluded.version,revision=excluded.revision,connection_id=excluded.connection_id,gpo_id=excluded.gpo_id,user_filter_group_id=excluded.user_filter_group_id,computer_target_ou_id=excluded.computer_target_ou_id,directory_config_status='pending',directory_config_error=NULL,updated_at=excluded.updated_at`)
      .bind(actor.companyId,input.positionId,version,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,now),
  ];
  return executeCommand(db,actor,key,operation,parsed,recordId,version,{positionId:recordId,version,revision,queued:preview.holders.filter(h=>h.account_id).length,awaitingAccountLinkage:preview.holders.filter(h=>!h.account_id).length,directoryStatus:"pending",endpointStatus:"unconfirmed"},statements);
}
const unmapInput=z.object({positionId:id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)});
export async function previewUnmapPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown){
  it(actor);
  const input=unmapInput.parse(raw);
  const policy=await db.prepare(`SELECT p.*,o.title FROM position_gpo_policies p JOIN organization_positions o ON o.id=p.position_id AND o.company_id=p.company_id
    WHERE p.company_id=? AND p.position_id=?`).bind(actor.companyId,input.positionId)
    .first<{version:number;revision:number;title:string;connection_id:string|null;gpo_id:string|null;user_filter_group_id:string|null;computer_target_ou_id:string|null}>();
  if(!policy||!policy.connection_id||policy.version!==input.expectedVersion)throw new ApiError(409,"GPO_MAPPING_CHANGED","Reload the current Position mapping before removing it.");
  const holders=(await db.prepare(`SELECT e.person_id,p.legal_name,
      ARRAY(SELECT DISTINCT i.external_id FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id
        WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
          AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=? AND i.enabled=true ORDER BY i.external_id) account_ids
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.company_id=? AND e.position_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text
      AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) AND p.status='active' AND p.merged_into_id IS NULL ORDER BY p.legal_name,e.id`)
    .bind(policy.connection_id,actor.companyId,input.positionId).all<{person_id:string;legal_name:string;account_ids:string[]}>()).results;
  const sharedMappings=(await db.prepare(`SELECT p.position_id,p.revision,p.user_filter_group_id,p.computer_target_ou_id
    FROM position_gpo_policies p JOIN organization_positions o ON o.company_id=p.company_id AND o.id=p.position_id AND o.active=1
    WHERE p.company_id=? AND p.connection_id=? AND p.gpo_id=? AND p.position_id<>?
    ORDER BY p.position_id`).bind(actor.companyId,policy.connection_id,policy.gpo_id,input.positionId).all()).results;
  const result={positionId:input.positionId,title:policy.title,version:policy.version+1,revision:policy.revision+1,
    current:{connectionId:policy.connection_id,gpoId:policy.gpo_id,userFilterGroupId:policy.user_filter_group_id,computerTargetOuId:policy.computer_target_ou_id},
    holders,sharedMappings,accountMemberships:"queued_for_reconciliation"};
  return {...result,token:previewDigest({input,result})};
}
export async function unmapPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){
  const parsed=unmapInput.extend({previewToken}).parse(raw),{previewToken:reviewedToken,...input}=parsed,operation="technology.position-gpo.unmap";
  await mutatingIt(actor);
  const prior=await replayCommand(db,actor,key,operation,input.positionId,parsed);if(prior)return prior;
  const preview=await previewUnmapPositionGpo(db,actor,input),now=Date.now();
  if(preview.token!==reviewedToken)throw new ApiError(409,"GPO_PREVIEW_CHANGED","Review the current Position mapping impact again.");
  return executeCommand(db,actor,key,operation,parsed,input.positionId,preview.version,
    {positionId:input.positionId,version:preview.version,revision:preview.revision,mapped:false,queuedForReconciliation:true},[
      db.prepare("SELECT pg_advisory_xact_lock(821039320)"),
      assertion(db,key,"it-role","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='it_admin' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'iam.directory.apply'))",[actor.assignmentId,actor.userId,actor.companyId]),
      assertion(db,key,"mapping-version","EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=? AND connection_id IS NOT NULL)",[actor.companyId,input.positionId,input.expectedVersion]),
      db.prepare(`INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,
        computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at)
        VALUES(?,?,?,?,NULL,NULL,NULL,NULL,NULL,?::jsonb,?,?,?,?)`)
        .bind(`position-gpo-unmap-${key}`,actor.companyId,input.positionId,preview.revision,JSON.stringify(preview.holders.map(h=>h.person_id)),actor.userId,input.reason,key,now),
      db.prepare(`UPDATE position_gpo_policies SET version=?,revision=?,connection_id=NULL,gpo_id=NULL,user_filter_group_id=NULL,
        computer_target_ou_id=NULL,directory_config_status='directory_applied',directory_config_error=NULL,updated_at=?
        WHERE company_id=? AND position_id=?`).bind(preview.version,preview.revision,now,actor.companyId,input.positionId),
    ]);
}
export async function positionGpoOverview(db:PostgresDatabase,actor:WorkContext){
  it(actor);
  const [policies,jobs,positions]=await Promise.all([
    db.prepare("SELECT p.*,p.connection_id IS NOT NULL mapped,o.title FROM position_gpo_policies p JOIN organization_positions o ON o.id=p.position_id AND o.company_id=p.company_id WHERE p.company_id=? ORDER BY o.title").bind(actor.companyId).all(),
    db.prepare("SELECT j.id,j.position_id,j.person_id,j.engagement_id,j.account_id,p.legal_name,j.revision,j.desired_action,j.status,j.version,j.attempts,j.last_error,j.directory_observed_at,j.endpoint_observed_at FROM position_gpo_jobs j JOIN hr_people p ON p.id=j.person_id WHERE j.company_id=? ORDER BY j.updated_at DESC LIMIT 200").bind(actor.companyId).all(),
    db.prepare(`SELECT o.id,o.title,COUNT(DISTINCT e.id)::integer holder_count,p.version mapping_version,
      p.connection_id IS NOT NULL mapped,CASE WHEN p.connection_id IS NOT NULL THEN p.directory_config_status ELSE NULL END mapping_status
      FROM organization_positions o LEFT JOIN position_gpo_policies p ON p.company_id=o.company_id AND p.position_id=o.id
      LEFT JOIN hr_engagements e ON e.company_id=o.company_id AND e.position_id=o.id AND e.status='active'
        AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      WHERE o.company_id=? AND o.active=1 GROUP BY o.id,o.title,p.version,p.connection_id,p.directory_config_status ORDER BY o.title,o.id`).bind(actor.companyId).all(),
  ]);
  return {policies:policies.results,jobs:jobs.results,positions:positions.results};
}
export async function retryPositionGpo(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){
  await mutatingIt(actor);
  const input=z.object({id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)}).parse(raw),operation="technology.position-gpo.retry";
  const prior=await replayCommand(db,actor,key,operation,input.id,input);if(prior)return prior;
  const row=await db.prepare("SELECT company_id,version,status FROM position_gpo_jobs WHERE id=? AND company_id=?").bind(input.id,actor.companyId).first<{company_id:string;version:number;status:string}>();
  if(!row || row.version!==input.expectedVersion || !["blocked","retry"].includes(row.status))throw new ApiError(409,"GPO_JOB_CHANGED","Only a blocked current job can be retried.");
  return executeCommand(db,actor,key,operation,input,input.id,row.version+1,{id:input.id,status:"queued",version:row.version+1},[
    assertion(db,key,"it-role","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='it_admin' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'iam.directory.apply'))",[actor.assignmentId,actor.userId,actor.companyId]),
    assertion(db,key,"job-version","EXISTS(SELECT 1 FROM position_gpo_jobs WHERE id=? AND company_id=? AND version=? AND status IN ('blocked','retry'))",[input.id,actor.companyId,input.expectedVersion]),
    db.prepare("UPDATE position_gpo_jobs SET status='queued',version=version+1,last_error=NULL,updated_at=? WHERE id=?").bind(Date.now(),input.id),
  ]);
}
export async function retryPositionGpoConfig(db:PostgresDatabase,actor:WorkContext,raw:unknown,key:string){
  await mutatingIt(actor);
  const input=z.object({positionId:id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)}).parse(raw),operation="technology.position-gpo.config-retry";
  const prior=await replayCommand(db,actor,key,operation,input.positionId,input);if(prior)return prior;
  const policy=await db.prepare("SELECT version,revision,directory_config_status FROM position_gpo_policies WHERE company_id=? AND position_id=?").bind(actor.companyId,input.positionId).first<{version:number;revision:number;directory_config_status:string}>();
  if(!policy || policy.version!==input.expectedVersion || policy.directory_config_status!=="blocked")throw new ApiError(409,"GPO_CONFIG_CHANGED","Only a blocked current configuration can be retried.");
  return executeCommand(db,actor,key,operation,input,input.positionId,policy.version,{positionId:input.positionId,version:policy.version,status:"pending"},[
    assertion(db,key,"it-role","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='it_admin' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'iam.directory.apply'))",[actor.assignmentId,actor.userId,actor.companyId]),
    assertion(db,key,"config-version","EXISTS(SELECT 1 FROM position_gpo_policies WHERE company_id=? AND position_id=? AND version=? AND directory_config_status='blocked')",[actor.companyId,input.positionId,input.expectedVersion]),
    db.prepare("UPDATE position_gpo_policies SET directory_config_status='pending',directory_config_error=NULL,updated_at=? WHERE company_id=? AND position_id=?").bind(Date.now(),actor.companyId,input.positionId),
  ]);
}

type Job={id:string;company_id:string;position_id:string;person_id:string;engagement_id:string|null;account_id:string|null;revision:number;engagement_version:number;connection_id:string;gpo_id:string;filter_group_id:string;computer_target_ou_id:string;desired_action:"add"|"remove";version:number;rule_id:string;source_id:string};
type Claim={connection_id:string;group_id:string;member_id:string;source_id:string;rule_id:string;company_id:string;position_id:string;person_id:string;engagement_id:string;engagement_version:number;revision:number;gpo_id:string;computer_target_ou_id:string};
type Ownership={connection_id:string;gpo_id:string;kind:"filter_group"|"ou_link";target_id:string;status:"pending"|"written"|"confirmed"};
type Desired=Omit<Job,"id"|"version"|"desired_action"> & {engagement_id:string;account_id:string|null};
const jobId=(part:string[])=>`gpo:v3:${createHash("sha256").update(JSON.stringify(part)).digest("hex")}`;
const membershipKey=(connectionId:string,groupId:string,accountId:string,source:string)=>JSON.stringify([connectionId,groupId,accountId,source]);
const codeFor=(error:unknown)=>error instanceof ApiError?error.code:"DIRECTORY_RETRY_REQUIRED";
function filters(inventory:DirectoryInventory,gpoId:string){
  const gpo=inventory.objects.find(o=>o.id===gpoId&&o.kind==="gpo");
  if(!gpo)throw new ApiError(409,"DIRECTORY_TARGET_INVALID","The mapped GPO is absent from this Directory.");
  return Array.isArray(gpo.properties.securityFilterGroupIds)?gpo.properties.securityFilterGroupIds as string[]:[];
}
const sameIds=(left:string[],right:string[])=>left.length===right.length&&left.every(id=>right.includes(id));
function links(inventory:DirectoryInventory,ouId:string){
  const ou=inventory.objects.find(o=>o.id===ouId&&o.kind==="ou");
  if(!ou)throw new ApiError(409,"DIRECTORY_TARGET_INVALID","The mapped OU is absent from this Directory.");
  return Array.isArray(ou.properties.gpoLinks)?ou.properties.gpoLinks as {gpoId:string;enabled?:boolean}[]:[];
}
async function configureGpos(db:SqlDatabase){
  const policies=await effectiveGpoRules(db);
  const ownership=(await db.prepare("SELECT connection_id,gpo_id,kind,target_id,status FROM position_gpo_config_ownership").all<Ownership>()).results;
  const pairs=new Set([...policies.map(p=>JSON.stringify([p.connection_id,p.gpo_id])),...ownership.map(o=>JSON.stringify([o.connection_id,o.gpo_id]))]);
  for(const pair of pairs){
    const [connectionId,gpoId]=JSON.parse(pair) as [string,string];
    const target=policies.filter(p=>p.connection_id===connectionId&&p.gpo_id===gpoId);
    const owned=ownership.filter(o=>o.connection_id===connectionId&&o.gpo_id===gpoId);
    try{
      const companyId=target[0]?.company_id??(await db.prepare("SELECT company_id FROM iam_connections WHERE id=?").bind(connectionId).first<{company_id:string}>())?.company_id;
      if(!companyId)continue;
      const c=await writableConnection(db,companyId,connectionId);
      let inventory=await readAutomationInventory(c);
      const desiredGroups=new Set(target.map(p=>p.user_filter_group_id));
      const ownedGroups=new Set(owned.filter(o=>o.kind==="filter_group"&&o.status!=="pending").map(o=>o.target_id));
      const actualGroups=filters(inventory,gpoId);
      const nextGroups=[...new Set([...actualGroups.filter(g=>!ownedGroups.has(g)||desiredGroups.has(g)),...desiredGroups])];
      if(!sameIds(actualGroups,nextGroups)){
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await db.prepare(`INSERT INTO position_gpo_config_ownership(connection_id,gpo_id,kind,target_id,created_at,status)
          VALUES(?,?,'filter_group',?,?,'pending') ON CONFLICT DO NOTHING`).bind(connectionId,gpoId,groupId,Date.now()).run();
        await applyDirectoryAction(c,jobId(["filter",pair,JSON.stringify(nextGroups),inventory.objects.find(o=>o.id===gpoId)!.version]),{kind:"gpo.filter",objectId:gpoId,groupIds:nextGroups},inventory);
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await db.prepare(`UPDATE position_gpo_config_ownership SET status='written'
          WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='pending'`).bind(connectionId,gpoId,groupId).run();
        inventory=await readAutomationInventory(c);
        if(!sameIds(filters(inventory,gpoId),nextGroups))throw new ApiError(409,"GPO_READBACK_REQUIRED","The Directory did not confirm the GPO security filter.");
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await db.prepare(`UPDATE position_gpo_config_ownership SET status='confirmed'
          WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='written'`).bind(connectionId,gpoId,groupId).run();
      }
      for(const item of owned.filter(o=>o.kind==="filter_group"&&o.status==="written"))if(filters(inventory,gpoId).includes(item.target_id))
        await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,item.target_id).run();
      for(const item of owned.filter(o=>o.kind==="filter_group"))if(!desiredGroups.has(item.target_id))await db.prepare("DELETE FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id=? AND kind='filter_group' AND target_id=?").bind(connectionId,gpoId,item.target_id).run();
      const desiredOus=new Set(target.map(p=>p.computer_target_ou_id));
      for(const ouId of desiredOus){
        const existingLink=links(inventory,ouId).find(l=>l.gpoId===gpoId);
        if(existingLink){
          if(existingLink.enabled===false)throw new ApiError(409,"GPO_LINK_DISABLED","Enable the existing GPO link in the Directory before using this mapping.");
          continue;
        }
        await db.prepare(`INSERT INTO position_gpo_config_ownership(connection_id,gpo_id,kind,target_id,created_at,status)
          VALUES(?,?,'ou_link',?,?,'pending') ON CONFLICT DO NOTHING`).bind(connectionId,gpoId,ouId,Date.now()).run();
        await applyDirectoryAction(c,jobId(["link",pair,ouId,inventory.objects.find(o=>o.id===ouId)!.version]),{kind:"gpo.link",objectId:gpoId,parentId:ouId,enabled:true,enforced:false,remove:false},inventory);
        await db.prepare("UPDATE position_gpo_config_ownership SET status='written' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='pending'")
          .bind(connectionId,gpoId,ouId).run();
        inventory=await readAutomationInventory(c);
        if(!links(inventory,ouId).some(l=>l.gpoId===gpoId))throw new ApiError(409,"GPO_READBACK_REQUIRED","The Directory did not confirm the GPO OU link.");
        await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,ouId).run();
      }
      for(const item of owned.filter(o=>o.kind==="ou_link"&&o.status==="written"))if(links(inventory,item.target_id).some(l=>l.gpoId===gpoId))
        await db.prepare("UPDATE position_gpo_config_ownership SET status='confirmed' WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=? AND status='written'")
          .bind(connectionId,gpoId,item.target_id).run();
      for(const item of owned.filter(o=>o.kind==="ou_link"))if(!desiredOus.has(item.target_id)){
        const ouId=item.target_id;
        if(item.status!=="pending"&&links(inventory,ouId).some(l=>l.gpoId===gpoId)){
          await applyDirectoryAction(c,jobId(["unlink",pair,ouId,inventory.objects.find(o=>o.id===ouId)!.version]),{kind:"gpo.link",objectId:gpoId,parentId:ouId,enabled:true,enforced:false,remove:true},inventory);
          inventory=await readAutomationInventory(c);
          if(links(inventory,ouId).some(l=>l.gpoId===gpoId))throw new ApiError(409,"GPO_UNLINK_READBACK_REQUIRED","The previous GPO OU link remains in the Directory.");
        }
        await db.prepare("DELETE FROM position_gpo_config_ownership WHERE connection_id=? AND gpo_id=? AND kind='ou_link' AND target_id=?").bind(connectionId,gpoId,ouId).run();
      }
      for(const policy of target){
        await db.prepare("UPDATE organization_gpo_rules SET directory_config_status='directory_applied',directory_config_error=NULL WHERE company_id=? AND id=? AND revision=?").bind(policy.company_id,policy.id,policy.revision).run();
        if(policy.legacy_position_id)await db.prepare("UPDATE position_gpo_policies SET directory_config_status='directory_applied',directory_config_error=NULL WHERE company_id=? AND position_id=? AND revision=?").bind(policy.company_id,policy.legacy_position_id,policy.revision).run();
      }
    }catch(error){
      for(const policy of target){
        await db.prepare("UPDATE organization_gpo_rules SET directory_config_status='blocked',directory_config_error=? WHERE company_id=? AND id=? AND revision=?").bind(codeFor(error),policy.company_id,policy.id,policy.revision).run();
        if(policy.legacy_position_id)await db.prepare("UPDATE position_gpo_policies SET directory_config_status='blocked',directory_config_error=? WHERE company_id=? AND position_id=? AND revision=?").bind(codeFor(error),policy.company_id,policy.legacy_position_id,policy.revision).run();
      }
    }
  }
}
async function desiredMemberships(db:SqlDatabase):Promise<Desired[]>{
  const rows=(await db.prepare(`SELECT e.id engagement_id,e.company_id,e.position_id,e.person_id,e.version engagement_version,
      r.id rule_id,r.legacy_position_id,r.revision,r.connection_id,r.gpo_id,r.user_filter_group_id filter_group_id,r.computer_target_ou_id,i.external_id account_id
    FROM hr_engagements e JOIN hr_people person ON person.id=e.person_id AND person.company_id=e.company_id
    JOIN organization_positions position ON position.id=e.position_id AND position.company_id=e.company_id AND position.active=1
    JOIN organization_gpo_rules r ON r.company_id=e.company_id AND r.active
      AND r.target_kind='position' AND r.target_id=position.id
    LEFT JOIN LATERAL (SELECT DISTINCT i.external_id FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
        AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=r.connection_id AND i.enabled=true) i ON true
    WHERE e.status='active' AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND person.status='active' AND person.merged_into_id IS NULL`).all<Omit<Desired,"source_id">&{legacy_position_id:string|null}>()).results;
  return rows.map(row=>({...row,source_id:gpoRuleSourceId(row.rule_id,row.engagement_id,!!row.legacy_position_id)}));
}
async function queueJob(db:SqlDatabase,row:Omit<Job,"id"|"version">){
  if(row.account_id&&row.engagement_id){
    const prior=await db.prepare(`SELECT status FROM position_gpo_jobs WHERE company_id=? AND position_id=? AND source_id=? AND account_id=?
      AND rule_id=? AND revision=? AND connection_id=? AND filter_group_id=? AND desired_action=?
      AND status IN ('queued','running','retry','blocked','directory_applied') ORDER BY updated_at DESC LIMIT 1`)
      .bind(row.company_id,row.position_id,row.source_id,row.account_id,row.rule_id,row.revision,row.connection_id,row.filter_group_id,row.desired_action).first<{status:string}>();
    if(prior&&prior.status!=="directory_applied")return;
    if(prior&&row.desired_action==="add"){
      const source=await db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?")
        .bind(row.connection_id,row.filter_group_id,row.account_id,row.source_id).first();
      if(source)return;
    }
  }
  const id=jobId([row.desired_action,row.connection_id,row.filter_group_id,row.account_id??"awaiting",row.source_id,row.rule_id,String(row.engagement_version),String(row.revision),row.account_id?randomUUID():""]);
  await db.prepare(`INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,engagement_id,account_id,revision,engagement_version,
    connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at,rule_id,source_id)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING`)
    .bind(id,row.company_id,row.position_id,row.person_id,row.engagement_id,row.account_id,row.revision,row.engagement_version,
      row.connection_id,row.gpo_id,row.filter_group_id,row.computer_target_ou_id,row.desired_action,row.account_id?"queued":"awaiting_identity",Date.now(),Date.now(),row.rule_id,row.source_id).run();
}
const waitingKey=(row:Pick<Job,"company_id"|"engagement_id"|"position_id"|"rule_id"|"revision">)=>JSON.stringify([row.company_id,row.engagement_id,row.position_id,row.rule_id,row.revision]);
export async function reconcilePositionGpo(db:SqlDatabase){
  const rows=await desiredMemberships(db),desired=new Map<string,Desired>(),waiting=new Set<string>();
  for(const row of rows){
    if(!row.account_id){waiting.add(waitingKey(row));await queueJob(db,{...row,desired_action:"add"});continue;}
    desired.set(membershipKey(row.connection_id,row.filter_group_id,row.account_id,row.source_id),row);
  }
  const claims=(await db.prepare(`SELECT c.* FROM organization_gpo_membership_claims c JOIN people_directory_membership_sources s
    ON s.connection_id=c.connection_id AND s.group_id=c.group_id AND s.member_id=c.member_id AND s.source_id=c.source_id`).all<Claim>()).results;
  for(const row of desired.values())await queueJob(db,{...row,desired_action:"add"});
  const staleAdds=(await db.prepare(`SELECT id,position_id,engagement_id,account_id,connection_id,filter_group_id,rule_id,source_id,revision
    FROM position_gpo_jobs WHERE desired_action='add' AND status IN ('queued','retry','blocked')`).all<Job>()).results;
  for(const job of staleAdds){
    const current=job.account_id&&job.source_id?desired.get(membershipKey(job.connection_id,job.filter_group_id,job.account_id,job.source_id)):null;
    if(!current||current.revision!==job.revision||current.rule_id!==job.rule_id||current.position_id!==job.position_id)await db.prepare("UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=? WHERE id=? AND status IN ('queued','retry','blocked')").bind(Date.now(),job.id).run();
  }
  for(const claim of claims){
    if(desired.has(membershipKey(claim.connection_id,claim.group_id,claim.member_id,claim.source_id)))continue;
    await queueJob(db,{company_id:claim.company_id,position_id:claim.position_id,person_id:claim.person_id,engagement_id:claim.engagement_id,
      account_id:claim.member_id,revision:claim.revision,engagement_version:claim.engagement_version,connection_id:claim.connection_id,
      gpo_id:claim.gpo_id,filter_group_id:claim.group_id,computer_target_ou_id:claim.computer_target_ou_id,desired_action:"remove",rule_id:claim.rule_id,source_id:claim.source_id});
  }
  const waitingJobs=(await db.prepare("SELECT id,company_id,engagement_id,position_id,rule_id,revision FROM position_gpo_jobs WHERE status='awaiting_identity'").all<Job>()).results;
  for(const job of waitingJobs)if(!waiting.has(waitingKey(job)))await db.prepare("UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=? WHERE id=? AND status='awaiting_identity'").bind(Date.now(),job.id).run();
  return desired;
}
async function finishJob(db:SqlDatabase,job:Job,status:"directory_applied"|"superseded",observedAt?:number){
  await db.prepare("UPDATE position_gpo_jobs SET status=?,directory_observed_at=COALESCE(?,directory_observed_at),last_error=NULL,version=version+1,updated_at=? WHERE id=?")
    .bind(status,observedAt??null,Date.now(),job.id).run();
}
async function recordClaim(db:SqlDatabase,job:Job){
  await db.batch([
    db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING")
      .bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id,Date.now()),
    db.prepare(`INSERT INTO organization_gpo_membership_claims(connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(connection_id,group_id,member_id,source_id) DO UPDATE SET rule_id=excluded.rule_id,position_id=excluded.position_id,revision=excluded.revision,engagement_version=excluded.engagement_version,gpo_id=excluded.gpo_id,computer_target_ou_id=excluded.computer_target_ou_id`)
      .bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id,job.rule_id,job.company_id,job.position_id,job.engagement_id,job.person_id,job.revision,job.engagement_version,job.gpo_id,job.computer_target_ou_id),
  ]);
}
async function deleteClaim(db:SqlDatabase,job:Job){
  await db.batch([
    db.prepare("DELETE FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id),
    db.prepare("DELETE FROM organization_gpo_membership_claims WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id),
  ]);
}
export async function drainPositionGpo(db:PostgresDatabase,limit=10){
  // One pinned connection retains the session lock while short claim batches
  // commit independently. This also works with the production pool size of one.
  await db.withSession(async session=>{
    const locked=await session.prepare("SELECT pg_try_advisory_lock(821039320) locked").first<boolean>("locked");
    if(!locked)return;
    const connections:string[]=[];
    try{
      // Share the existing Directory worker's provider/source-ledger lock. Take
      // every relevant connection before any work so a busy provider defers the
      // tick without publishing partial configuration or failed delivery state.
      const relevant=(await session.prepare(`SELECT connection_id FROM organization_gpo_rules WHERE active
        UNION SELECT connection_id FROM position_gpo_config_ownership
        UNION SELECT connection_id FROM organization_gpo_membership_claims
        UNION SELECT connection_id FROM position_gpo_jobs WHERE status IN ('queued','retry','running')
        ORDER BY connection_id`).all<{connection_id:string}>()).results;
      for(const row of relevant){
        const acquired=await session.prepare("SELECT pg_try_advisory_lock(hashtext(?)) locked").bind("people-directory:"+row.connection_id).first<boolean>("locked");
        if(!acquired)return;
        connections.push(row.connection_id);
      }
      await drainPositionGpoLocked(session,limit);
    }finally{
      for(const connectionId of connections.reverse())await session.prepare("SELECT pg_advisory_unlock(hashtext(?))").bind("people-directory:"+connectionId).run();
      await session.prepare("SELECT pg_advisory_unlock(821039320)").run();
    }
  });
}
async function drainPositionGpoLocked(db:SqlDatabase,limit=10){
  await db.prepare("UPDATE position_gpo_jobs SET status='retry',version=version+1,last_error='WORKER_INTERRUPTED',updated_at=? WHERE status='running' AND updated_at<?").bind(Date.now(),Date.now()-900000).run();
  await reconcilePositionGpo(db);
  await configureGpos(db);
  const due=(await db.prepare("SELECT * FROM position_gpo_jobs WHERE status IN ('queued','retry') ORDER BY updated_at,id LIMIT ?").bind(limit).all<Job>()).results;
  for(const job of due){
    const claimed=await db.prepare("UPDATE position_gpo_jobs SET status='running',attempts=attempts+1,version=version+1,updated_at=? WHERE id=? AND version=? AND status IN ('queued','retry') RETURNING id").bind(Date.now(),job.id,job.version).first();
    if(!claimed)continue;
    try{
      if(!job.account_id||!job.engagement_id||!job.rule_id||!job.source_id){await finishJob(db,job,"superseded");continue;}
      const desired=new Map((await desiredMemberships(db)).filter(row=>row.account_id).map(row=>[membershipKey(row.connection_id,row.filter_group_id,row.account_id!,row.source_id),row]));
      const key=membershipKey(job.connection_id,job.filter_group_id,job.account_id,job.source_id);
      if(job.desired_action==="add" && (!desired.has(key)||desired.get(key)?.revision!==job.revision||desired.get(key)?.rule_id!==job.rule_id||desired.get(key)?.position_id!==job.position_id)){await finishJob(db,job,"superseded");continue;}
      const currentClaim=await db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id=?").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id).first();
      if(job.desired_action==="remove" && (!currentClaim||desired.has(key))){await finishJob(db,job,"superseded");continue;}
      const c=await writableConnection(db,job.company_id,job.connection_id);
      const policy=await db.prepare("SELECT directory_config_status FROM organization_gpo_rules WHERE company_id=? AND id=? AND revision=? AND active").bind(job.company_id,job.rule_id,job.revision).first<{directory_config_status:string}>();
      if(job.desired_action==="add"&&policy?.directory_config_status!=="directory_applied")throw new ApiError(409,"GPO_CONFIG_BLOCKED","The GPO Directory configuration needs IT attention.");
      let inventory=await readAutomationInventory(c);
      const group=inventory.objects.find(o=>o.id===job.filter_group_id&&o.kind==="group");
      if(!group){
        if(job.desired_action==="remove"){await deleteClaim(db,job);await finishJob(db,job,"directory_applied",inventory.observedAt);continue;}
        throw new ApiError(409,"DIRECTORY_TARGET_INVALID","The mapped group is absent from the Directory.");
      }
      if(job.desired_action==="add"){
        if(!inventory.objects.some(o=>o.id===job.account_id&&o.kind==="account"))throw new ApiError(409,"DIRECTORY_TARGET_INVALID","The linked account is absent from the Directory.");
        if(group.memberIds.includes(job.account_id)){
          const existing=await db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? LIMIT 1").bind(job.connection_id,job.filter_group_id,job.account_id).first();
          if(!existing)await db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,?,?,'external',?) ON CONFLICT DO NOTHING").bind(job.connection_id,job.filter_group_id,job.account_id,Date.now()).run();
        }else{
          await applyDirectoryAction(c,job.id,{kind:"group.members",objectId:job.filter_group_id,memberIds:[job.account_id],action:"add"},inventory);
          // A verified provider write owns its claim even if subsequent read-back fails.
          await recordClaim(db,job);
          inventory=await readAutomationInventory(c);
        }
        if(!inventory.objects.find(o=>o.id===job.filter_group_id)?.memberIds.includes(job.account_id))throw new ApiError(409,"DIRECTORY_READBACK_REQUIRED","The Directory did not confirm the membership.");
        await recordClaim(db,job);
      }else{
        const others=await db.prepare("SELECT 1 FROM people_directory_membership_sources WHERE connection_id=? AND group_id=? AND member_id=? AND source_id<>? LIMIT 1").bind(job.connection_id,job.filter_group_id,job.account_id,job.source_id).first();
        const otherDesired=[...desired.values()].some(d=>d.connection_id===job.connection_id&&d.filter_group_id===job.filter_group_id&&d.account_id===job.account_id);
        if(otherDesired&&!others){await db.prepare("UPDATE position_gpo_jobs SET status='retry',last_error='AWAITING_REPLACEMENT_SOURCE',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),job.id).run();continue;}
        if(!others&&!otherDesired&&group.memberIds.includes(job.account_id)){
          await applyDirectoryAction(c,job.id,{kind:"group.members",objectId:job.filter_group_id,memberIds:[job.account_id],action:"remove"},inventory);
          inventory=await readAutomationInventory(c);
          if(inventory.objects.find(o=>o.id===job.filter_group_id)?.memberIds.includes(job.account_id))throw new ApiError(409,"DIRECTORY_READBACK_REQUIRED","The Directory did not confirm membership removal.");
        }
        await deleteClaim(db,job);
      }
      await finishJob(db,job,"directory_applied",inventory.observedAt);
    }catch(error){await db.prepare("UPDATE position_gpo_jobs SET status='blocked',last_error=?,version=version+1,updated_at=? WHERE id=?").bind(codeFor(error),Date.now(),job.id).run();}
  }
}
