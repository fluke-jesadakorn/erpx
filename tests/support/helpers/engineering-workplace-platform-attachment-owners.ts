import {expenseDb as db,expenseFixture,key} from "./expense-fixture";
import {env} from "../runtime-env";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {selectWorkRole} from "@erpx/server-work_projection/public/work-role-store";
import {sealSecret} from "@erpx/server-identity_access/public/iam-mfa";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {iamActorFromWork} from "@erpx/server-identity_access/public/iam-context";
import {grantIamAccess,grantInput} from "@erpx/server-identity_access/public/iam-service";
import {bindIamMigrationEvidence,bindIamReviewEvidence,bindProvisioningEvidence} from "@erpx/server-platform_administration/public/system-evidence";
import {bindReleaseEvidence} from "@erpx/server-platform_administration/public/release-acceptance";
import {organizationNodeChangeSchema,saveOrganizationNodeChange,decideOrganizationNodeChange} from '@erpx/server-organization/public/organization';
import {savePostDraft,publishPost,postDraftSchema} from "@erpx/server-collaboration/public/social-home";
import {saveMaster,saveOrder,orderSchema} from "@erpx/server-commerce/public/commerce";
import {createSupportTicket} from "@erpx/server-support/public/support";
import {bindSupportEvidence} from "@erpx/server-support/public/support-evidence";
import {createSupportIncident,changeSupportIncident} from "@erpx/server-support/public/support-incidents";
import {createMessagingTeam,sendMessagingMessage,messageSchema} from "@erpx/server-collaboration/public/messaging";
import {legalCommand} from "@erpx/server-legal/public/legal/service";
import {customerPortalCommand} from "@erpx/server-customer_360/public/customer-portal";
import {customerPortalWorkContext} from "@erpx/server-customer_360/public/customer-portal-attachments";
export type WorkplaceAttachmentTuple={id:string;targetType:string;purposeKey:string;componentKind:'record'|'action'|'publication'|'message'};
export type WorkplaceAttachmentOwner={actor:WorkContext;targetId:string;table:string;idColumn?:string;versionColumn:string;version:number;componentRef:string|null;bindingType:string;versionIncrement?:number;bindAction?:(setId:string,command:string,sourceId:string)=>Promise<unknown>;revoke?:()=>Promise<void>;assertEffect?:(command:string)=>Promise<void>};
export async function controlAccount(actor:WorkContext,mfa=false){
 const now=Date.now(),sessionId=key();
 if(actor.role!=='external_customer')actor.email=`${actor.userId}@example.test`;
 await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?) ON CONFLICT(id) DO NOTHING').bind(actor.userId,actor.name,actor.email,now,now).run();
 if(mfa){
  await env.CONTROL_DB.prepare('INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date(now).toISOString(),new Date(now).toISOString(),actor.userId).run();
  await env.CONTROL_DB.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true)').bind(actor.userId,await sealSecret('JBSWY3DPEHPK3PXP')).run();
  await env.CONTROL_DB.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,actor.userId,now).run();
  await selectWorkRole(db,{sessionId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion:1,revision:0,expiresAt:now+86400000});
  actor.authority={sessionId,revision:1,assignmentVersion:1};
 }
 if(actor.role!=='external_customer')actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role,mfa?sessionId:undefined);
}

async function supportSource(f:Awaited<ReturnType<typeof expenseFixture>>){
 const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Declared synthetic customer'},key()),warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Synthetic attachment warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Synthetic attachment product',price:'10'},key());
 const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:warehouse.id,orderDate:'2026-09-20',dueDate:'2026-09-24',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key());
 return {customerId:String(customer.id),orderId:String(order.id)};
}

export async function workplaceAttachmentOwner(tuple:WorkplaceAttachmentTuple):Promise<WorkplaceAttachmentOwner>{
 const f=await expenseFixture(false),actor=f.owner,now=Date.now(),base={actor,targetId:'',table:'',versionColumn:'version',version:1,componentRef:null,bindingType:'record_document'};
 await controlAccount(actor);
 if(tuple.targetType.endsWith('_brand'))return {...base,targetId:tuple.targetType==='holding_brand'?'h001':tuple.targetType==='branch_brand'?f.branch:f.company,table:'organization_nodes',idColumn:'id',versionColumn:'hierarchy_revision',versionIncrement:0,bindingType:'publication_source'};
 if(tuple.targetType==='organization_node')return {...base,targetId:tuple.purposeKey==='branch_permit'?f.branch:f.company,table:'organization_nodes',versionColumn:'hierarchy_revision'};
 if(tuple.targetType==='organization_change'){
  const requester=await f.member('independent-organization-requester','hr_admin'),input=organizationNodeChangeSchema.parse({action:'create',kind:'branch',slug:`synthetic-${key()}`,displayName:'Synthetic new Branch',expectedVersion:0,reason:'Synthetic independent organization change'}),saved=await saveOrganizationNodeChange(db,requester,null,input,key(),true);
  return {...base,targetId:String(saved.id),table:'organization_node_changes',componentRef:'organization-change-review',bindingType:'organization_change_review',bindAction:(setId,command)=>decideOrganizationNodeChange(db,actor,String(saved.id),{expectedVersion:1,decision:'approve',reason:'Independent organization proof review',attachmentSetIds:[setId]},command)};
 }
 if(tuple.targetType==='iam_review'){
  await controlAccount(f.claimant);await controlAccount(actor,true);
  const proposed=await grantIamAccess(env.H001_DB,iamActorFromWork(actor),grantInput.parse({userId:f.claimant.userId,roleId:'finance_approver',scopeKind:'holding',validUntil:now+86400000,reason:'Synthetic independent IAM review artifact'}),key());
  return {...base,targetId:proposed.id,table:'iam_reviews',componentRef:tuple.componentKind==='action'?'iam_review:evidence':null,bindingType:tuple.componentKind==='action'?'iam_review_evidence':'record_document',...(tuple.componentKind==='action'?{bindAction:(setId:string,command:string)=>bindIamReviewEvidence(env.H001_DB,actor,proposed.id,{expectedVersion:1,attachmentSetIds:[setId],reason:'Sanitized review evidence; no access approved'},command)}:{})};
 }
 if(tuple.targetType==='iam_migration'){
  await controlAccount(actor,true);const connection=key(),id=key();
  // A declared directory snapshot owner is persisted; no live provider is called
  // and its migration remains awaiting independent review throughout this test.
  await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,?,'univention',?::jsonb,'draft',?,?)").bind(connection,f.company,'Declared fixture directory',JSON.stringify({mode:'ldaps',authority:'corp.test',secretProfile:'declared-fixture',ldapUrl:'ldaps://corp.test',baseDn:'dc=corp,dc=test'}),actor.userId,now).run();
  await db.prepare("INSERT INTO iam_migrations(id,company_id,connection_id,users_json,status,created_by,created_at) VALUES(?,?,?,?::jsonb,'awaiting_review',?,?)").bind(id,f.company,connection,JSON.stringify([{localUserId:f.claimant.userId,personId:`person-${f.claimant.userId}`,identityId:'declared-unlinked-directory-identity',grants:[]}]),actor.userId,now).run();
  return {...base,targetId:id,table:'iam_migrations',componentRef:tuple.componentKind==='action'?'iam_migration:evidence':null,bindingType:tuple.componentKind==='action'?'iam_migration_evidence':'record_document',...(tuple.componentKind==='action'?{bindAction:(setId:string,command:string)=>bindIamMigrationEvidence(env.H001_DB,actor,id,{expectedVersion:1,attachmentSetIds:[setId],reason:'Sanitized migration evidence; no migration executed'},command)}:{})};
 }
 if(tuple.targetType==='people_provision_operation'){
  await controlAccount(actor,true);const id=key();
  await db.prepare("INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at) VALUES(?,?,?::jsonb,?::jsonb,'pending_review',?,?,?,?,?)").bind(id,f.company,JSON.stringify({engagementIds:[`eng-${f.claimant.userId}`],sourceKind:'position',reason:'Declared pending synthetic provisioning'}),JSON.stringify({adapter:'declared fixture',providerAccepted:false}),actor.userId,actor.assignmentId,actor.authority!.sessionId,now,now).run();
  return {...base,targetId:id,table:'people_provision_operations',componentRef:tuple.componentKind==='action'?'people_provision_operation:evidence':null,bindingType:tuple.componentKind==='action'?'people_provision_operation_evidence':'record_document',...(tuple.componentKind==='action'?{bindAction:(setId:string,command:string)=>bindProvisioningEvidence(env.H001_DB,actor,id,{expectedVersion:1,attachmentSetIds:[setId],reason:'Sanitized provisioning evidence; no account provisioned'},command)}:{})};
 }
 if(tuple.targetType==='company_release_acceptance'){
  const id=key();await db.prepare('INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at) VALUES(?,?,?,?,0,1,?,?,?,?,?,?)').bind(id,f.company,'declared-engineering-fixture','declared-fixture-build','a'.repeat(64),JSON.stringify({scope:'isolated fixture',certification:'pending'}),actor.userId,'Synthetic artifact intake without certification',now,now).run();
  return {...base,targetId:id,table:'company_release_acceptances',componentRef:tuple.componentKind==='action'?'company_release_acceptance:evidence':null,bindingType:tuple.componentKind==='action'?'release_acceptance_evidence':'record_document',...(tuple.componentKind==='action'?{bindAction:(setId:string,command:string)=>bindReleaseEvidence(db,actor,id,{expectedVersion:1,attachmentSetIds:[setId],reason:'Sanitized supplemental evidence; criteria remain pending'},command)}:{})};
 }
 if(tuple.targetType==='company_provisioning'){
  await db.prepare("INSERT INTO company_provisioning(company_id,holding_id,owner_user_id,status,requested_at,completed_at,updated_at) VALUES(?,?,?,'active',?,?,?)").bind(f.company,`declared-holding-${key()}`,actor.userId,now,now,now).run();
  return {...base,targetId:f.company,idColumn:'company_id',table:'company_provisioning'};
 }
 if(tuple.targetType==='social_draft'||tuple.targetType==='social_post'){
  const draft=await savePostDraft(db,actor,null,postDraftSchema.parse({body:'Synthetic governed File Sources evidence',audience:{kind:'company'},purpose:tuple.purposeKey==='handover_file'?'handover':'update'}),key());
  if(tuple.targetType==='social_draft')return {...base,targetId:draft.id,table:'social_post_drafts'};
  const published=await publishPost(db,actor,draft.id,1,key());return {...base,targetId:published.id,table:'social_posts'};
 }
 if(tuple.targetType==='support_ticket'||tuple.targetType==='support_incident'){
  const colleague=await f.member('assigned-support-colleague','support_agent'),source=await supportSource(f),ticket=await createSupportTicket(actor,{...source,sourceVersion:1,title:'Synthetic File Sources Support',issue:'Review synthetic evidence only',ownerId:colleague.userId,dueDate:'2026-09-24'},key());
  if(tuple.targetType==='support_ticket')return {...base,targetId:String(ticket.id),table:'support_tickets',componentRef:`support-evidence:${tuple.purposeKey}`,bindingType:'support_action',bindAction:(setId,command)=>bindSupportEvidence(actor,String(ticket.id),{expectedVersion:1,attachmentSetIds:[setId],reason:'Reviewed synthetic Support action evidence'},command)};
  const incident=await createSupportIncident(actor,{ticketId:ticket.id,ticketVersion:1,title:'Synthetic engineering incident',impact:'Isolated synthetic Company only',severity:'high',ownerId:colleague.userId,dueDate:'2026-09-24'},key());
  await controlAccount(colleague);
  return {...base,actor:colleague,targetId:String(incident.id),table:'support_incidents',componentRef:'incident:open',bindingType:'support_incident_action',bindAction:(setId,command)=>changeSupportIncident(colleague,String(incident.id),{action:'start',expectedVersion:1,reason:'Investigate scanned synthetic evidence',attachmentSetIds:[setId]},command)};
 }
 if(tuple.targetType==='messaging_room'){
  await controlAccount(f.claimant);const room=await createMessagingTeam(db,actor,{name:'Synthetic engineering file team',members:[f.claimant.userId]},key()),fileId=key();
  return {...base,targetId:room.roomId,table:'messaging_rooms',componentRef:fileId,bindingType:'message_attachment',bindAction:async(setId,command,sourceId)=>{
   const source=(await db.prepare('SELECT original_name,media_type,bytes,object_key,checksum_sha256 FROM attachments WHERE id=?').bind(sourceId).first())!,link=String(await db.prepare('SELECT id FROM attachment_links WHERE set_id=? AND attachment_id=?').bind(setId,sourceId).first('id'));
   await db.prepare('INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,attachment_set_id,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING').bind(fileId,room.roomId,actor.userId,source.original_name,source.media_type,source.bytes,source.object_key,source.checksum_sha256,Date.now(),setId,link,sourceId).run();
   return sendMessagingMessage(db,actor,room.roomId,messageSchema.parse({body:'Synthetic scanned evidence attached to this real message',files:[fileId]}),command);
  }};
 }
 if(tuple.targetType==='customer_access_binding'){
  const source=await supportSource(f),id=key(),email=`synthetic-customer-${key()}@example.test`,userId=key();await db.prepare("UPDATE commerce_orders SET state='confirmed' WHERE id=?").bind(source.orderId).run();
  await db.prepare("INSERT INTO customer_access_bindings(id,company_id,customer_id,order_id,email,status,expires_at,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'approved',?,'Declared verified synthetic customer fixture',?,?,?)").bind(id,f.company,source.customerId,source.orderId,email,now+86400000,actor.userId,now,now).run();await db.prepare('INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES(?,?,?,?)').bind(id,userId,email,now).run();
  const session={userId,email,holdingId:'h001' as const,holdingSlug:'test',holdingName:'Test'},customer=customerPortalWorkContext(session,{id,company_id:f.company,branch_id:f.branch,department_id:null,version:1});await controlAccount(customer);
  return {...base,actor:customer,targetId:id,table:'customer_access_bindings',componentRef:'request_support',bindingType:'customer_portal_support_request',bindAction:(setId,command)=>customerPortalCommand(env.H001_DB,session,{bindingId:id,expectedBindingVersion:1,action:'request_support',note:'Synthetic scanned customer evidence',attachmentSetIds:[setId]},command),revoke:async()=>{await db.prepare("UPDATE customer_access_bindings SET status='revoked',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),id).run();}};
 }
 throw new Error(`WORKPLACE_ATTACHMENT_OWNER_MISSING:${tuple.id}`);
}

export async function legalAttachmentOwner(){
 const f=await expenseFixture(false),actor=await f.member('engineering-legal-author','legal_manager');await controlAccount(actor);
 await legalCommand(db,actor,{action:'settings',enabled:true},key());
 const created=await legalCommand(db,actor,{action:'create',item:{kind:'contract',title:'Synthetic Legal original owner',ownerId:actor.userId,data:{contractType:'Synthetic fixture',parties:'Fictional Company and counterparty'}}},key());
 return {f,actor,id:String(created.id)};
}
