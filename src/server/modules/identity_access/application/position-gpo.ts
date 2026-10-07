import type {WorkContext} from '../public/authorization';
import type {ProviderConnection} from '../public/iam-providers';
import type {DirectoryInventory,DirectoryAction} from '@/lib/directory-operations';

import {z} from 'zod';
import {hasPermission} from '@/lib/permissions';

import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';
import {positionGpoInput,unmapInput,id,previewToken,membershipKey,waitingKey,codeFor,sameIds,filters,links,type Input,type Connection,type Job,type Claim,type Ownership,type Desired} from '../domain/position-gpo';
import {gpoRuleSourceId,type EffectiveRuleRow} from '../domain/gpo-rules';
import type {GpoCommands} from './gpo-rules';
export interface PositionGpoRepository<Effect>{activeConnection(connectionId:string,companyId:string):Promise<Connection | null>;
directoryWriteState(companyId:string,connectionId:string):Promise<{version:number;tested_version:number|null;write_enabled:boolean} | null>;
publishingDirectoryState(id:string,companyId:string):Promise<{version:number;write_enabled:boolean;tested_version:number|null} | null>;
activePosition(positionId:string,companyId:string):Promise<{id:string;title:string} | null>;
currentPolicy(companyId:string,positionId:string):Promise<{version:number;revision:number;user_filter_group_id:string|null} | null>;
positionHolders(connectionId:string,companyId:string,positionId:string):Promise<{engagement_id:string;person_id:string;version:number;name:string;account_id:string|null}[]>;
sharedMappings(companyId:string,connectionId:string,gpoId:string,positionId:string):Promise<Record<string,unknown>[]>;
lockCatalog():Effect;
guardItAuthority(key:string,assignmentId:string,userId:string,companyId:string):Effect;
guardPublishVersion(key:string,expectedVersion:number,companyId:string,positionId:string):Effect;
guardConnection(key:string,connectionId:string,companyId:string):Effect;
preparePublishRevision(value:string,companyId:string,positionId:string,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,version:string,value2:string,userId:string,reason:string,key:string,now:number):Effect;
preparePublishPolicy(companyId:string,positionId:string,version:number,revision:number,connectionId:string,gpoId:string,userFilterGroupId:string,computerTargetOuId:string,now:number):Effect;
currentMapping(companyId:string,positionId:string):Promise<{version:number;revision:number;title:string;connection_id:string|null;gpo_id:string|null;user_filter_group_id:string|null;computer_target_ou_id:string|null} | null>;
unmapHolders(connection_id:string,companyId:string,positionId:string):Promise<{person_id:string;legal_name:string;account_ids:string[]}[]>;
unmapSharedMappings(companyId:string,connection_id:string,gpo_id:string | null,positionId:string):Promise<Record<string,unknown>[]>;

guardUnmapVersion(key:string,companyId:string,positionId:string,expectedVersion:number):Effect;
prepareUnmapRevision(value:string,companyId:string,positionId:string,revision:number,value2:string,userId:string,reason:string,key:string,now:number):Effect;
prepareUnmapPolicy(version:number,revision:number,now:number,companyId:string,positionId:string):Effect;
overviewPolicies(companyId:string):Promise<Record<string,unknown>[]>;
overviewJobs(companyId:string):Promise<Record<string,unknown>[]>;
overviewPositions(companyId:string):Promise<Record<string,unknown>[]>;
currentJob(id:string,companyId:string):Promise<{company_id:string;version:number;status:string} | null>;

guardJobVersion(key:string,id:string,companyId:string,expectedVersion:number):Effect;
prepareJobRetry(now:number,id:string):Effect;
currentConfig(companyId:string,positionId:string):Promise<{version:number;revision:number;directory_config_status:string} | null>;

guardConfigVersion(key:string,companyId:string,positionId:string,expectedVersion:number):Effect;
prepareConfigRetry(now:number,companyId:string,positionId:string):Effect;
configOwnership():Promise<Ownership[]>;
connectionCompany(connectionId:string):Promise<{company_id:string} | null>;
stageFilterOwnership(connectionId:string,gpoId:string,groupId:string,now:number):Promise<void>;
markFilterWritten(connectionId:string,gpoId:string,groupId:string):Promise<void>;
markFilterConfirmed(connectionId:string,gpoId:string,groupId:string):Promise<void>;
recoverFilterConfirmed(connectionId:string,gpoId:string,target_id:string):Promise<void>;
removeFilterOwnership(connectionId:string,gpoId:string,target_id:string):Promise<void>;
stageLinkOwnership(connectionId:string,gpoId:string,ouId:string,now:number):Promise<void>;
markLinkWritten(connectionId:string,gpoId:string,ouId:string):Promise<void>;
markLinkConfirmed(connectionId:string,gpoId:string,ouId:string):Promise<void>;
recoverLinkConfirmed(connectionId:string,gpoId:string,target_id:string):Promise<void>;
removeLinkOwnership(connectionId:string,gpoId:string,ouId:string):Promise<void>;
markRuleApplied(company_id:string,id:string,revision:number):Promise<void>;
markLegacyApplied(company_id:string,legacy_position_id:string,revision:number):Promise<void>;
markRuleBlocked(value:string,company_id:string,id:string,revision:number):Promise<void>;
markLegacyBlocked(value:string,company_id:string,legacy_position_id:string,revision:number):Promise<void>;
desiredMembershipFacts():Promise<(Omit<Desired,"source_id">&{legacy_position_id:string|null})[]>;
latestDelivery(company_id:string,position_id:string,source_id:string,account_id:string,rule_id:string,revision:number,connection_id:string,filter_group_id:string,desired_action:"add" | "remove"):Promise<{status:string} | null>;
ownedMembership(connection_id:string,filter_group_id:string,account_id:string,source_id:string):Promise<Record<string,unknown> | null>;
enqueueDelivery(id:string,company_id:string,position_id:string,person_id:string,engagement_id:string | null,account_id:string | null,revision:number,engagement_version:number,connection_id:string,gpo_id:string,filter_group_id:string,computer_target_ou_id:string,desired_action:"add" | "remove",value:"queued" | "awaiting_identity",now:number,rule_id:string,source_id:string):Promise<void>;
membershipClaims():Promise<Claim[]>;
pendingAdds():Promise<Job[]>;
supersedeAdd(now:number,id:string):Promise<void>;
waitingIdentities():Promise<Job[]>;
supersedeWaiting(now:number,id:string):Promise<void>;
finishDelivery(status:"directory_applied" | "superseded",value:number | null,now:number,id:string):Promise<void>;
recoverInterrupted(now:number,now2:number):Promise<void>;
dueDeliveries(limit:number):Promise<Job[]>;
claimDelivery(now:number,id:string,version:number):Promise<Record<string,unknown> | null>;
currentMembership(connection_id:string,filter_group_id:string,account_id:string,source_id:string):Promise<Record<string,unknown> | null>;
deliveryRule(company_id:string,rule_id:string,revision:number):Promise<{directory_config_status:string} | null>;
existingMembershipSource(connection_id:string,filter_group_id:string,account_id:string):Promise<Record<string,unknown> | null>;
preserveExternalMembership(connection_id:string,filter_group_id:string,account_id:string,now:number):Promise<void>;
otherMembershipSource(connection_id:string,filter_group_id:string,account_id:string,source_id:string):Promise<Record<string,unknown> | null>;
deferReplacement(now:number,id:string):Promise<void>;
blockDelivery(value:string,now:number,id:string):Promise<void>;
recordClaim(job:Job,now:number):Promise<void>;
deleteClaim(job:Job):Promise<void>;}
export interface PositionGpoPorts<Effect>{repository:PositionGpoRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:unknown):string;commands:GpoCommands<Effect>;requireFreshMfa(input:{userId:string;sessionId:string}):Promise<unknown>;readInventory(connection:ProviderConnection):Promise<DirectoryInventory>;effectiveRules():Promise<EffectiveRuleRow[]>;applyAction(connection:ProviderConnection,key:string,action:DirectoryAction,inventory:DirectoryInventory):Promise<unknown>;}
export function createPositionGpoApplication<Effect>(ports:PositionGpoPorts<Effect>){const r=ports.repository;
const deliveryId=(part:string[])=>'gpo:v3:'+ports.hash(part);
function it(actor:WorkContext){
  if(actor.role!=="it_admin" || actor.assignmentScope!=="holding" || !hasPermission(actor,"iam.directory.apply"))
    throw new BusinessRuleError(403,"IT_AUTHORITY_REQUIRED","Choose an active Company IT assignment to publish GPO mappings.");
}
async function mutatingIt(actor:WorkContext){
  it(actor);
  if(!actor.authority?.sessionId)throw new BusinessRuleError(403,"FRESH_MFA_REQUIRED","Verify MFA before changing Directory access.");
  await ports.requireFreshMfa({userId:actor.userId,sessionId:actor.authority.sessionId});
}
async function connection(companyId:string,connectionId:string){
  const row=await r.activeConnection(connectionId,companyId);
  if(!row)throw new BusinessRuleError(409,"DIRECTORY_CONNECTION_REQUIRED","Choose an active Directory connection.");
  return {...row,config:typeof row.config==="string"?JSON.parse(row.config):row.config};
}
async function writableConnection(companyId:string,connectionId:string){
  const c=await connection(companyId,connectionId);
  const state=await r.directoryWriteState(companyId,connectionId);
  if(!state?.write_enabled||state.tested_version!==state.version)throw new BusinessRuleError(409,"DIRECTORY_WRITE_NOT_READY","Test and enable Directory writes before GPO delivery.");
  return c;
}
async function inventoryFor(actor:WorkContext,input:Input){
  const c=await connection(actor.companyId,input.connectionId);
  const state=await r.publishingDirectoryState(c.id,actor.companyId);
  if(!state?.write_enabled || state.tested_version!==state.version)throw new BusinessRuleError(409,"DIRECTORY_WRITE_NOT_READY","Test and enable Directory writes before publishing a position GPO.");
  const inventory=await ports.readInventory(c);
  const find=(objectId:string,kind:string)=>{
    const obj=inventory.objects.find(o=>o.id===objectId&&o.kind===kind);
    if(!obj)throw new BusinessRuleError(422,"DIRECTORY_TARGET_INVALID",`Choose a current ${kind} from this connection.`);
    return obj;
  };
  const gpo=find(input.gpoId,"gpo"),group=find(input.userFilterGroupId,"group"),ou=find(input.computerTargetOuId,"ou");
  return {connection:c,state,inventory,gpo,group,ou};
}
async function previewPositionGpo(actor:WorkContext,raw:unknown){
  it(actor);
  const input=positionGpoInput.parse(raw);
  const position=await r.activePosition(input.positionId,actor.companyId);
  if(!position)throw new BusinessRuleError(404,"POSITION_NOT_FOUND","Active position not found.");
  const current=await r.currentPolicy(actor.companyId,input.positionId);
  if((current?.version??0)!==input.expectedVersion)throw new BusinessRuleError(409,"VERSION_CONFLICT","The Position GPO changed. Reload before publishing.");
  const directory=await inventoryFor(actor,input);
  const holders=(await r.positionHolders(input.connectionId,actor.companyId,input.positionId));
  const deduplicated=new Map<string,typeof holders[number]&{accountStatus:"linked"|"awaiting_account_linkage"}>();
  for(const holder of holders){
    const key=`${holder.engagement_id}:${holder.account_id??"awaiting"}`;
    deduplicated.set(key,{...holder,accountStatus:holder.account_id?"linked":"awaiting_account_linkage"});
  }
  const holderRows=[...deduplicated.values()];
  const sharedMappings=(await r.sharedMappings(actor.companyId,input.connectionId,input.gpoId,input.positionId));
  const result={position,version:input.expectedVersion+1,revision:(current?.revision??0)+1,
    directory:{connectionId:directory.connection.id,gpo:{id:directory.gpo.id,version:directory.gpo.version},userFilterGroup:{id:directory.group.id,version:directory.group.version},computerTargetOu:{id:directory.ou.id,version:directory.ou.version},observedAt:directory.inventory.observedAt},
    holders:holderRows,previousFilterGroupId:current?.user_filter_group_id??null,
    sharedMappings,
    changes:{directoryConfiguration:true,membersToAdd:holderRows.flatMap(h=>h.account_id?[h.account_id]:[]),membersToRemoveFromPreviousGroup:current?.user_filter_group_id && current.user_filter_group_id!==input.userFilterGroupId?holderRows.flatMap(h=>h.account_id?[h.account_id]:[]):[]},
    endpointStatus:"unconfirmed"};
  const directoryEvidence={connectionId:result.directory.connectionId,gpo:result.directory.gpo,
    userFilterGroup:result.directory.userFilterGroup,computerTargetOu:result.directory.computerTargetOu};
  return {...result,token:ports.hash({input,...result,directory:directoryEvidence})};
}
async function publishPositionGpo(actor:WorkContext,raw:unknown,key:string){
  const parsed=positionGpoInput.extend({previewToken}).parse(raw),{previewToken:reviewedToken,...input}=parsed,operation="technology.position-gpo.publish",recordId=input.positionId;
  await mutatingIt(actor);
  const prior=await ports.commands.replay(actor,key,operation,recordId,parsed);if(prior)return prior;
  const preview=await previewPositionGpo(actor,input),now=ports.clock.now(),revision=preview.revision,version=preview.version;
  if(preview.token!==reviewedToken)throw new BusinessRuleError(409,"GPO_PREVIEW_CHANGED","Review the current Position mapping impact again.");
  const statements=[
    r.lockCatalog(),
    r.guardItAuthority(key,actor.assignmentId,actor.userId,actor.companyId),
    r.guardPublishVersion(key,input.expectedVersion,actor.companyId,input.positionId),
    r.guardConnection(key,input.connectionId,actor.companyId),
    r.preparePublishRevision(`position-gpo-${key}`,actor.companyId,input.positionId,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,preview.directory.gpo.version,JSON.stringify(preview.holders.map(h=>h.person_id)),actor.userId,input.reason,key,now),
    r.preparePublishPolicy(actor.companyId,input.positionId,version,revision,input.connectionId,input.gpoId,input.userFilterGroupId,input.computerTargetOuId,now),
  ];
  return ports.commands.commit(actor,key,operation,parsed,recordId,version,{positionId:recordId,version,revision,queued:preview.holders.filter(h=>h.account_id).length,awaitingAccountLinkage:preview.holders.filter(h=>!h.account_id).length,directoryStatus:"pending",endpointStatus:"unconfirmed"},statements);
}
async function previewUnmapPositionGpo(actor:WorkContext,raw:unknown){
  it(actor);
  const input=unmapInput.parse(raw);
  const policy=await r.currentMapping(actor.companyId,input.positionId);
  if(!policy||!policy.connection_id||policy.version!==input.expectedVersion)throw new BusinessRuleError(409,"GPO_MAPPING_CHANGED","Reload the current Position mapping before removing it.");
  const holders=(await r.unmapHolders(policy.connection_id,actor.companyId,input.positionId));
  const sharedMappings=(await r.unmapSharedMappings(actor.companyId,policy.connection_id,policy.gpo_id,input.positionId));
  const result={positionId:input.positionId,title:policy.title,version:policy.version+1,revision:policy.revision+1,
    current:{connectionId:policy.connection_id,gpoId:policy.gpo_id,userFilterGroupId:policy.user_filter_group_id,computerTargetOuId:policy.computer_target_ou_id},
    holders,sharedMappings,accountMemberships:"queued_for_reconciliation"};
  return {...result,token:ports.hash({input,result})};
}
async function unmapPositionGpo(actor:WorkContext,raw:unknown,key:string){
  const parsed=unmapInput.extend({previewToken}).parse(raw),{previewToken:reviewedToken,...input}=parsed,operation="technology.position-gpo.unmap";
  await mutatingIt(actor);
  const prior=await ports.commands.replay(actor,key,operation,input.positionId,parsed);if(prior)return prior;
  const preview=await previewUnmapPositionGpo(actor,input),now=ports.clock.now();
  if(preview.token!==reviewedToken)throw new BusinessRuleError(409,"GPO_PREVIEW_CHANGED","Review the current Position mapping impact again.");
  return ports.commands.commit(actor,key,operation,parsed,input.positionId,preview.version,
    {positionId:input.positionId,version:preview.version,revision:preview.revision,mapped:false,queuedForReconciliation:true},[
      r.lockCatalog(),
      r.guardItAuthority(key,actor.assignmentId,actor.userId,actor.companyId),
      r.guardUnmapVersion(key,actor.companyId,input.positionId,input.expectedVersion),
      r.prepareUnmapRevision(`position-gpo-unmap-${key}`,actor.companyId,input.positionId,preview.revision,JSON.stringify(preview.holders.map(h=>h.person_id)),actor.userId,input.reason,key,now),
      r.prepareUnmapPolicy(preview.version,preview.revision,now,actor.companyId,input.positionId),
    ]);
}
async function positionGpoOverview(actor:WorkContext){
  it(actor);
  const [policies,jobs,positions]=await Promise.all([
    r.overviewPolicies(actor.companyId),
    r.overviewJobs(actor.companyId),
    r.overviewPositions(actor.companyId),
  ]);
  return {policies:policies,jobs:jobs,positions:positions};
}
async function retryPositionGpo(actor:WorkContext,raw:unknown,key:string){
  await mutatingIt(actor);
  const input=z.object({id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)}).parse(raw),operation="technology.position-gpo.retry";
  const prior=await ports.commands.replay(actor,key,operation,input.id,input);if(prior)return prior;
  const row=await r.currentJob(input.id,actor.companyId);
  if(!row || row.version!==input.expectedVersion || !["blocked","retry"].includes(row.status))throw new BusinessRuleError(409,"GPO_JOB_CHANGED","Only a blocked current job can be retried.");
  return ports.commands.commit(actor,key,operation,input,input.id,row.version+1,{id:input.id,status:"queued",version:row.version+1},[
    r.guardItAuthority(key,actor.assignmentId,actor.userId,actor.companyId),
    r.guardJobVersion(key,input.id,actor.companyId,input.expectedVersion),
    r.prepareJobRetry(ports.clock.now(),input.id),
  ]);
}
async function retryPositionGpoConfig(actor:WorkContext,raw:unknown,key:string){
  await mutatingIt(actor);
  const input=z.object({positionId:id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)}).parse(raw),operation="technology.position-gpo.config-retry";
  const prior=await ports.commands.replay(actor,key,operation,input.positionId,input);if(prior)return prior;
  const policy=await r.currentConfig(actor.companyId,input.positionId);
  if(!policy || policy.version!==input.expectedVersion || policy.directory_config_status!=="blocked")throw new BusinessRuleError(409,"GPO_CONFIG_CHANGED","Only a blocked current configuration can be retried.");
  return ports.commands.commit(actor,key,operation,input,input.positionId,policy.version,{positionId:input.positionId,version:policy.version,status:"pending"},[
    r.guardItAuthority(key,actor.assignmentId,actor.userId,actor.companyId),
    r.guardConfigVersion(key,actor.companyId,input.positionId,input.expectedVersion),
    r.prepareConfigRetry(ports.clock.now(),actor.companyId,input.positionId),
  ]);
}
async function configureGpos(){
  const policies=await ports.effectiveRules();
  const ownership=(await r.configOwnership());
  const pairs=new Set([...policies.map(p=>JSON.stringify([p.connection_id,p.gpo_id])),...ownership.map(o=>JSON.stringify([o.connection_id,o.gpo_id]))]);
  for(const pair of pairs){
    const [connectionId,gpoId]=JSON.parse(pair) as [string,string];
    const target=policies.filter(p=>p.connection_id===connectionId&&p.gpo_id===gpoId);
    const owned=ownership.filter(o=>o.connection_id===connectionId&&o.gpo_id===gpoId);
    try{
      const companyId=target[0]?.company_id??(await r.connectionCompany(connectionId))?.company_id;
      if(!companyId)continue;
      const c=await writableConnection(companyId,connectionId);
      let inventory=await ports.readInventory(c);
      const desiredGroups=new Set(target.map(p=>p.user_filter_group_id));
      const ownedGroups=new Set(owned.filter(o=>o.kind==="filter_group"&&o.status!=="pending").map(o=>o.target_id));
      const actualGroups=filters(inventory,gpoId);
      const nextGroups=[...new Set([...actualGroups.filter(g=>!ownedGroups.has(g)||desiredGroups.has(g)),...desiredGroups])];
      if(!sameIds(actualGroups,nextGroups)){
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await r.stageFilterOwnership(connectionId,gpoId,groupId,ports.clock.now());
        await ports.applyAction(c,deliveryId(["filter",pair,JSON.stringify(nextGroups),inventory.objects.find(o=>o.id===gpoId)!.version]),{kind:"gpo.filter",objectId:gpoId,groupIds:nextGroups},inventory);
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await r.markFilterWritten(connectionId,gpoId,groupId);
        inventory=await ports.readInventory(c);
        if(!sameIds(filters(inventory,gpoId),nextGroups))throw new BusinessRuleError(409,"GPO_READBACK_REQUIRED","The Directory did not confirm the GPO security filter.");
        for(const groupId of desiredGroups)if(!actualGroups.includes(groupId))await r.markFilterConfirmed(connectionId,gpoId,groupId);
      }
      for(const item of owned.filter(o=>o.kind==="filter_group"&&o.status==="written"))if(filters(inventory,gpoId).includes(item.target_id))
        await r.recoverFilterConfirmed(connectionId,gpoId,item.target_id);
      for(const item of owned.filter(o=>o.kind==="filter_group"))if(!desiredGroups.has(item.target_id))await r.removeFilterOwnership(connectionId,gpoId,item.target_id);
      const desiredOus=new Set(target.map(p=>p.computer_target_ou_id));
      for(const ouId of desiredOus){
        const existingLink=links(inventory,ouId).find(l=>l.gpoId===gpoId);
        if(existingLink){
          if(existingLink.enabled===false)throw new BusinessRuleError(409,"GPO_LINK_DISABLED","Enable the existing GPO link in the Directory before using this mapping.");
          continue;
        }
        await r.stageLinkOwnership(connectionId,gpoId,ouId,ports.clock.now());
        await ports.applyAction(c,deliveryId(["link",pair,ouId,inventory.objects.find(o=>o.id===ouId)!.version]),{kind:"gpo.link",objectId:gpoId,parentId:ouId,enabled:true,enforced:false,remove:false},inventory);
        await r.markLinkWritten(connectionId,gpoId,ouId);
        inventory=await ports.readInventory(c);
        if(!links(inventory,ouId).some(l=>l.gpoId===gpoId))throw new BusinessRuleError(409,"GPO_READBACK_REQUIRED","The Directory did not confirm the GPO OU link.");
        await r.markLinkConfirmed(connectionId,gpoId,ouId);
      }
      for(const item of owned.filter(o=>o.kind==="ou_link"&&o.status==="written"))if(links(inventory,item.target_id).some(l=>l.gpoId===gpoId))
        await r.recoverLinkConfirmed(connectionId,gpoId,item.target_id);
      for(const item of owned.filter(o=>o.kind==="ou_link"))if(!desiredOus.has(item.target_id)){
        const ouId=item.target_id;
        if(item.status!=="pending"&&links(inventory,ouId).some(l=>l.gpoId===gpoId)){
          await ports.applyAction(c,deliveryId(["unlink",pair,ouId,inventory.objects.find(o=>o.id===ouId)!.version]),{kind:"gpo.link",objectId:gpoId,parentId:ouId,enabled:true,enforced:false,remove:true},inventory);
          inventory=await ports.readInventory(c);
          if(links(inventory,ouId).some(l=>l.gpoId===gpoId))throw new BusinessRuleError(409,"GPO_UNLINK_READBACK_REQUIRED","The previous GPO OU link remains in the Directory.");
        }
        await r.removeLinkOwnership(connectionId,gpoId,ouId);
      }
      for(const policy of target){
        await r.markRuleApplied(policy.company_id,policy.id,policy.revision);
        if(policy.legacy_position_id)await r.markLegacyApplied(policy.company_id,policy.legacy_position_id,policy.revision);
      }
    }catch(error){
      for(const policy of target){
        await r.markRuleBlocked(codeFor(error),policy.company_id,policy.id,policy.revision);
        if(policy.legacy_position_id)await r.markLegacyBlocked(codeFor(error),policy.company_id,policy.legacy_position_id,policy.revision);
      }
    }
  }
}
async function desiredMemberships():Promise<Desired[]>{
  const rows=(await r.desiredMembershipFacts());
  return rows.map(row=>({...row,source_id:gpoRuleSourceId(row.rule_id,row.engagement_id,!!row.legacy_position_id)}));
}
async function queueJob(row:Omit<Job,"id"|"version">){
  if(row.account_id&&row.engagement_id){
    const prior=await r.latestDelivery(row.company_id,row.position_id,row.source_id,row.account_id,row.rule_id,row.revision,row.connection_id,row.filter_group_id,row.desired_action);
    if(prior&&prior.status!=="directory_applied")return;
    if(prior&&row.desired_action==="add"){
      const source=await r.ownedMembership(row.connection_id,row.filter_group_id,row.account_id,row.source_id);
      if(source)return;
    }
  }
  const id=deliveryId([row.desired_action,row.connection_id,row.filter_group_id,row.account_id??"awaiting",row.source_id,row.rule_id,String(row.engagement_version),String(row.revision),row.account_id?ports.identifiers.next():""]);
  await r.enqueueDelivery(id,row.company_id,row.position_id,row.person_id,row.engagement_id,row.account_id,row.revision,row.engagement_version,row.connection_id,row.gpo_id,row.filter_group_id,row.computer_target_ou_id,row.desired_action,row.account_id?"queued":"awaiting_identity",ports.clock.now(),row.rule_id,row.source_id);
}
async function reconcilePositionGpo(){
  const rows=await desiredMemberships(),desired=new Map<string,Desired>(),waiting=new Set<string>();
  for(const row of rows){
    if(!row.account_id){waiting.add(waitingKey(row));await queueJob({...row,desired_action:"add"});continue;}
    desired.set(membershipKey(row.connection_id,row.filter_group_id,row.account_id,row.source_id),row);
  }
  const claims=(await r.membershipClaims());
  for(const row of desired.values())await queueJob({...row,desired_action:"add"});
  const staleAdds=(await r.pendingAdds());
  for(const job of staleAdds){
    const current=job.account_id&&job.source_id?desired.get(membershipKey(job.connection_id,job.filter_group_id,job.account_id,job.source_id)):null;
    if(!current||current.revision!==job.revision||current.rule_id!==job.rule_id||current.position_id!==job.position_id)await r.supersedeAdd(ports.clock.now(),job.id);
  }
  for(const claim of claims){
    if(desired.has(membershipKey(claim.connection_id,claim.group_id,claim.member_id,claim.source_id)))continue;
    await queueJob({company_id:claim.company_id,position_id:claim.position_id,person_id:claim.person_id,engagement_id:claim.engagement_id,
      account_id:claim.member_id,revision:claim.revision,engagement_version:claim.engagement_version,connection_id:claim.connection_id,
      gpo_id:claim.gpo_id,filter_group_id:claim.group_id,computer_target_ou_id:claim.computer_target_ou_id,desired_action:"remove",rule_id:claim.rule_id,source_id:claim.source_id});
  }
  const waitingJobs=(await r.waitingIdentities());
  for(const job of waitingJobs)if(!waiting.has(waitingKey(job)))await r.supersedeWaiting(ports.clock.now(),job.id);
  return desired;
}
async function finishJob(job:Job,status:"directory_applied"|"superseded",observedAt?:number){
  await r.finishDelivery(status,observedAt??null,ports.clock.now(),job.id);
}
async function drainPositionGpoLocked(limit=10){
  await r.recoverInterrupted(ports.clock.now(),ports.clock.now()-900000);
  await reconcilePositionGpo();
  await configureGpos();
  const due=(await r.dueDeliveries(limit));
  for(const job of due){
    const claimed=await r.claimDelivery(ports.clock.now(),job.id,job.version);
    if(!claimed)continue;
    try{
      if(!job.account_id||!job.engagement_id||!job.rule_id||!job.source_id){await finishJob(job,"superseded");continue;}
      const desired=new Map((await desiredMemberships()).filter(row=>row.account_id).map(row=>[membershipKey(row.connection_id,row.filter_group_id,row.account_id!,row.source_id),row]));
      const key=membershipKey(job.connection_id,job.filter_group_id,job.account_id,job.source_id);
      if(job.desired_action==="add" && (!desired.has(key)||desired.get(key)?.revision!==job.revision||desired.get(key)?.rule_id!==job.rule_id||desired.get(key)?.position_id!==job.position_id)){await finishJob(job,"superseded");continue;}
      const currentClaim=await r.currentMembership(job.connection_id,job.filter_group_id,job.account_id,job.source_id);
      if(job.desired_action==="remove" && (!currentClaim||desired.has(key))){await finishJob(job,"superseded");continue;}
      const c=await writableConnection(job.company_id,job.connection_id);
      const policy=await r.deliveryRule(job.company_id,job.rule_id,job.revision);
      if(job.desired_action==="add"&&policy?.directory_config_status!=="directory_applied")throw new BusinessRuleError(409,"GPO_CONFIG_BLOCKED","The GPO Directory configuration needs IT attention.");
      let inventory=await ports.readInventory(c);
      const group=inventory.objects.find(o=>o.id===job.filter_group_id&&o.kind==="group");
      if(!group){
        if(job.desired_action==="remove"){await deleteClaim(job);await finishJob(job,"directory_applied",inventory.observedAt);continue;}
        throw new BusinessRuleError(409,"DIRECTORY_TARGET_INVALID","The mapped group is absent from the Directory.");
      }
      if(job.desired_action==="add"){
        if(!inventory.objects.some(o=>o.id===job.account_id&&o.kind==="account"))throw new BusinessRuleError(409,"DIRECTORY_TARGET_INVALID","The linked account is absent from the Directory.");
        if(group.memberIds.includes(job.account_id)){
          const existing=await r.existingMembershipSource(job.connection_id,job.filter_group_id,job.account_id);
          if(!existing)await r.preserveExternalMembership(job.connection_id,job.filter_group_id,job.account_id,ports.clock.now());
        }else{
          await ports.applyAction(c,job.id,{kind:"group.members",objectId:job.filter_group_id,memberIds:[job.account_id],action:"add"},inventory);
          // A verified provider write owns its claim even if subsequent read-back fails.
          await recordClaim(job);
          inventory=await ports.readInventory(c);
        }
        if(!inventory.objects.find(o=>o.id===job.filter_group_id)?.memberIds.includes(job.account_id))throw new BusinessRuleError(409,"DIRECTORY_READBACK_REQUIRED","The Directory did not confirm the membership.");
        await recordClaim(job);
      }else{
        const others=await r.otherMembershipSource(job.connection_id,job.filter_group_id,job.account_id,job.source_id);
        const otherDesired=[...desired.values()].some(d=>d.connection_id===job.connection_id&&d.filter_group_id===job.filter_group_id&&d.account_id===job.account_id);
        if(otherDesired&&!others){await r.deferReplacement(ports.clock.now(),job.id);continue;}
        if(!others&&!otherDesired&&group.memberIds.includes(job.account_id)){
          await ports.applyAction(c,job.id,{kind:"group.members",objectId:job.filter_group_id,memberIds:[job.account_id],action:"remove"},inventory);
          inventory=await ports.readInventory(c);
          if(inventory.objects.find(o=>o.id===job.filter_group_id)?.memberIds.includes(job.account_id))throw new BusinessRuleError(409,"DIRECTORY_READBACK_REQUIRED","The Directory did not confirm membership removal.");
        }
        await deleteClaim(job);
      }
      await finishJob(job,"directory_applied",inventory.observedAt);
    }catch(error){await r.blockDelivery(codeFor(error),ports.clock.now(),job.id);}
  }
}
async function recordClaim(job:Job){await r.recordClaim(job,ports.clock.now());}
async function deleteClaim(job:Job){await r.deleteClaim(job);}
return {previewPositionGpo,publishPositionGpo,previewUnmapPositionGpo,unmapPositionGpo,positionGpoOverview,retryPositionGpo,retryPositionGpoConfig,reconcilePositionGpo,drainPositionGpoLocked};}
