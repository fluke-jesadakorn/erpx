/** Normal identities and native owner records for one exact disposable cohort. */
import {randomBytes} from 'node:crypto';
import {chmod,writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {hashPassword} from 'better-auth/crypto';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import type {FileStore} from "@erpx/server-runtime/adapters/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {seedExpenseCompany} from "../../expenses/company";
import {moduleRegistry} from "@erpx/catalogue/module-registry";
import {businessRoles} from "@erpx/feature-identity_access/lib/business-roles";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {projectSchema,saveTask,taskSchema} from '@erpx/server-operations/public/staff-tasks';
import {seedProjectFor} from '../../../tests/integration/projects/project-actual-fixtures';
import {prepareGuidedIntake} from "@erpx/server-operations/public/guided-intake";
import {guidedIntakeInputSchema} from "@erpx/feature-shared/lib/ai/guided-intake";
import {refreshRecentUsageProjections} from "@erpx/server-commercial/public/usage-projection";
import {createMarketingAsset,createMarketingCandidate,changeMarketing} from "@erpx/server-marketing/public/marketing";
import {createAuth} from "@erpx/server-identity_access/public/auth";
import {beginMfa,totp,verifyMfa} from "@erpx/server-identity_access/public/iam-mfa";
import type {IamActor} from "@erpx/server-identity_access/public/iam-context";
import {connectionInput,saveConnection} from "@erpx/server-identity_access/public/iam-directories";
import {selectWorkRole} from "@erpx/server-work_projection/public/work-role-store";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet,createAttachmentPublication,approveAttachmentPublication} from "@erpx/server-files/public/attachments/service";
import {savePostDraft,publishPost,postDraftSchema,postView} from '@erpx/server-collaboration/public/social-home';
import {createMessagingRoom,sendMessagingMessage,roomSchema,messageSchema} from '@erpx/server-collaboration/public/messaging';
import {collaborationWorkspace} from "@erpx/server-collaboration/public/collaboration-work";
import {operationsExceptionDetail} from "@erpx/server-operations/public/operations-exceptions";

type Input={run:string;directory:string;control:PostgresDatabase;h001:PostgresDatabase;h002:PostgresDatabase;h003:PostgresDatabase;h001Files:FileStore;h002Files:FileStore;h003Files:FileStore};
export type WorkplacePlatformUatUser={key:string;name:string;email:string;userId:string;assignmentId:string;companyId:string;holdingSlug:'simulation'|'erpx';role:string;platformRole?:'platform_admin'|'platform_auditor';platformAssignmentId?:string};
const key=()=>crypto.randomUUID();
async function privateJson(path:string,value:unknown){await writeFile(path,JSON.stringify(value,null,2)+'\n',{mode:0o600});await chmod(path,0o600);}
function exact(db:PostgresDatabase,run:string,binding:string){const url=new URL(String(db.pool.options.connectionString));if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('EXACT_WORKPLACE_PLATFORM_UAT_DATABASE_REQUIRED');}
export async function seedEngineeringWorkplacePlatformBrowser(input:Input){
 if(!/^[a-f0-9]{12}$/.test(input.run))throw new Error('EXACT_WORKPLACE_PLATFORM_UAT_RUN_REQUIRED');
 for(const [binding,db] of [['CONTROL_DB',input.control],['H001_DB',input.h001],['H002_DB',input.h002],['H003_DB',input.h003]] as const)exact(db,input.run,binding);
 const password=`Engineering-UAT-${randomBytes(18).toString('hex')}`,passwordHash=await hashPassword(password),now=Date.now(),users:WorkplacePlatformUatUser[]=[],actors:Record<string,WorkContext>={};
 const workplace=await seedExpenseCompany(input.h001,input.h001Files),realm=await seedExpenseCompany(input.h003,input.h003Files);
 await input.h001.prepare("UPDATE organization_nodes SET display_name='Workplace UAT Company · fictional' WHERE id=?").bind(workplace.company).run();
 await input.h003.prepare("UPDATE organization_nodes SET display_name='ERPX UAT Realm · fictional' WHERE id=?").bind(realm.company).run();
 await input.h003.prepare("UPDATE beta_bootstrap SET holding_id='h003' WHERE singleton=1").run();
 await input.control.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h003','erpx','ERPX engineering Realm','H003_DB','H003_READ_DB','H003_FILES','active')").run();
 async function register(name:string,fixture:typeof workplace,db:PostgresDatabase,holdingId:'h001'|'h003',role='holding_owner',platformRole?:'platform_admin'|'platform_auditor'){
  const actor=await fixture.member(`workplace-uat-${name}`,role,role==='holding_owner'?10:1),holdingSlug=holdingId==='h001'?'simulation':'erpx',modules=role==='holding_owner'?moduleRegistry.map(module=>module.id):businessRoles[role as keyof typeof businessRoles]??[];
  Object.assign(actor,{name:`UAT ${name.replaceAll('-',' ')}`,email:`workplace-${input.run}-${name}@simulation.test`,holdingId,holdingSlug,holdingName:'Synthetic engineering Holding',modules,scope:{kind:'holding',holdingId,holdingSlug,holdingName:'Synthetic engineering Holding',companyId:fixture.company}});
  await db.batch([db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),actor.assignmentId),db.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(actor.name,actor.name,actor.email,fixture.company,actor.userId)]);
  await input.control.batch([
   input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,actor.email,now,now),
   input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,passwordHash,now,now),
   input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,actor.name,now),
  ]);
  actor.iam=await loadIamAuthority(db,fixture.company,actor.userId,role);actors[name]=actor;
  const user:WorkplacePlatformUatUser={key:name,name:actor.name,email:actor.email,userId:actor.userId,assignmentId:actor.assignmentId,companyId:fixture.company,holdingSlug,role};
  if(platformRole){const assignmentId=key();await input.control.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,?,'[]',?,?)").bind(assignmentId,actor.userId,platformRole,now-1,now+86400000).run();Object.assign(user,{platformRole,platformAssignmentId:assignmentId});}
  users.push(user);return actor;
 }
 const author=await register('author',workplace,input.h001,'h001'),reviewer=await register('reviewer',workplace,input.h001,'h001'),operator=await register('operator',workplace,input.h001,'h001');
 await register('employee',workplace,input.h001,'h001','employee');
 const iamSubject=await register('iam-subject',workplace,input.h001,'h001','employee');
 await register('platform-author',realm,input.h003,'h003','holding_owner','platform_admin');await register('platform-reviewer',realm,input.h003,'h003','holding_owner','platform_admin');await register('platform-auditor',realm,input.h003,'h003','holding_owner','platform_auditor');await register('platform-temporary',realm,input.h003,'h003');
 const proposalSource=await saveTask(input.h001,operator,null,taskSchema.parse({title:'Fictional reviewed-proposal source',description:'Explicit no-network provider fixture; human reviews all fields before creating work',assigneeUserId:operator.userId}),key());
 // Project membership gives each intended reviewer real access to the shared
 // showcase sources. The separate private proposal source stays unchanged.
 const showcaseProject=await seedProjectFor(input.h001,author,null,projectSchema.parse({title:'Fictional delivery review project',description:'Declared engineering participants share native evidence for human delivery review',memberUserIds:[reviewer.userId,operator.userId]}),key());
 const task=await saveTask(input.h001,author,null,taskSchema.parse({title:'Fictional shipment exception source',description:'Native task source; no real customer incident',projectId:String(showcaseProject.id),assigneeUserId:operator.userId,dueAt:'2026-10-20T00:00:00+07:00'}),key());
 const showcaseSource=await saveTask(input.h001,author,null,taskSchema.parse({title:'Fictional shared delivery review source',description:'Native project evidence readable only by its owner, members and task participants',projectId:String(showcaseProject.id),assigneeUserId:operator.userId}),key());
 const showcaseTarget=await saveTask(input.h001,author,null,taskSchema.parse({title:'Fictional shared delivery confirmation',description:'Second native task in the same participant-controlled project',projectId:String(showcaseProject.id),assigneeUserId:reviewer.userId}),key());
 // Fixture state for the all-module showcase. Brand identity commands are owned
 // by the native command/HTTP path; this unreviewed draft is seeded directly so
 // the retained Brand read surface has an unpublished draft to display.
 const brandDraftId=`brand-draft-fixture-${key()}`,brandIdentity={identity:{shortName:'Fictional Workplace',tagline:'Declared engineering example awaiting independent publication'},visual:{accentColor:'#2864a0',logoPublicationId:null}};
 await input.h001.batch([
  input.h001.prepare("INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,created_by,updated_by,created_at,updated_at) VALUES(?,?,0,?,?,?,?,?)").bind(brandDraftId,workplace.company,JSON.stringify(brandIdentity),author.userId,author.userId,now,now),
  input.h001.prepare("INSERT INTO brand_identity_events(id,company_id,record_id,action,version,actor_id,reason,details_json,created_at) VALUES(?,?,?,'draft',1,?,?,?,?)").bind(`brand-event-${brandDraftId}`,workplace.company,brandDraftId,author.userId,'Native unused identity draft fixture; no publication',JSON.stringify({identity:brandIdentity,basedRevision:0}),now),
 ]);
 const brandDraft={id:brandDraftId,status:'draft',version:1};
 const socialDraft=await savePostDraft(input.h001,author,null,postDraftSchema.parse({body:'Fictional delivery team update: the native exception source is ready for human review. Only the selected engineering participants can read this post.',purpose:'handover',audience:{kind:'participants',userIds:[reviewer.userId,operator.userId]},sources:[{kind:'staff_task',id:String(showcaseSource.id),version:1}]}),key()),socialPost=await publishPost(input.h001,author,String(socialDraft.id),1,key());
 const room=await createMessagingRoom(input.h001,author,roomSchema.parse({kind:'group',name:'Fictional delivery review thread',members:[reviewer.userId,operator.userId]}),key()),message=await sendMessagingMessage(input.h001,reviewer,room.id,messageSchema.parse({body:'Independent fictional reviewer: retain the source evidence and confirm the delivery before closing the exception.'}),key());
 // Native Collaboration owner rows for the all-module showcase; the native
 // relation/action command suites own authority and transitions, while the
 // retained workspace read below still has to return these exact rows.
 const relationKey=key(),relationReason='Declared fictional native source relation for the human review handoff',cardKey=key(),cardReason='Human-owned action linked to the current native task source',cardDueAt=now+3*86400000,[relationSource,relationTarget]=[String(showcaseSource.id),String(showcaseTarget.id)].sort(),relationId=`record-relation-${relationKey}`,cardId=`action-card-${cardKey}`;
 await input.h001.batch([
  input.h001.prepare("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,'staff_task',?,1,'staff_task',?,1,'related',?,?,?,?)").bind(relationId,workplace.company,relationSource,relationTarget,relationReason,author.userId,now,now),
  input.h001.prepare("INSERT INTO collaboration_relation_events VALUES(?,?,1,'create',?,?,'active',?)").bind(`record-relation-event-${relationKey}`,relationId,author.userId,relationReason,now),
  input.h001.prepare("INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,'staff_task',?,1,'Fictional delivery review action',?,?,?,?,?)").bind(cardId,workplace.company,String(showcaseSource.id),operator.userId,cardDueAt,author.userId,now,now),
  input.h001.prepare("INSERT INTO collaboration_action_events VALUES(?,?,1,'create',?,?,'open',?,?,?)").bind(`action-card-event-${cardKey}`,cardId,author.userId,cardReason,operator.userId,cardDueAt,now),
  input.h001.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'New cross-record action card: review the linked source',updated_at FROM collaboration_action_cards WHERE id=?").bind(`action-card-notice-${cardKey}`,cardId),
 ]);
 const relation={id:relationId},card={id:cardId};
 const exceptionSource=await saveTask(input.h001,author,null,taskSchema.parse({title:'Fictional showcase delivery discrepancy',description:'Separate native source for the preloaded Operations example',projectId:String(showcaseProject.id),assigneeUserId:operator.userId}),key());
 // Operations policy/exception rows are seeded as owner facts here; the native
 // Operations command and its rust-operations-commands-postgres suite own the
 // lifecycle after this fictional browser fixture is read back.
 const queuePolicyId=`operations-policy-${key()}`,operationalExceptionId=`operations-exception-${key()}`,exceptionDueAt=now+240*60000,sourceReference=JSON.stringify({kind:'staff_task',id:String(exceptionSource.id),version:1}),policySnapshot=JSON.stringify({id:queuePolicyId,area:'fictional_showcase',name:'Fictional delivery exception policy',revision:1,sourceKinds:['staff_task'],targetMinutes:240,version:2});
 await input.h001.batch([
  input.h001.prepare("INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,?,1,?,?,'published',?,?,2,?,?)").bind(queuePolicyId,workplace.company,'fictional_showcase','Fictional delivery exception policy',JSON.stringify(['staff_task']),240,author.userId,reviewer.userId,now,now),
  input.h001.prepare("INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,status,due_at,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'open',?,1,?,?,?)").bind(operationalExceptionId,workplace.company,workplace.branch,null,'fictional_showcase','staff_task',String(exceptionSource.id),1,sourceReference,queuePolicyId,policySnapshot,'Fictional delivery confirmation missing','Declared fictional discrepancy awaiting acknowledgement and documented customer confirmation','normal',operator.userId,exceptionDueAt,author.userId,now,now),
  input.h001.prepare("INSERT INTO operations_exception_events(id,exception_id,version,action,actor_id,reason,status,details_json,created_at) VALUES(?,?,1,'open',?,?,'open',?,?)").bind(`operations-event-${key()}`,operationalExceptionId,author.userId,'Declared fictional discrepancy awaiting acknowledgement and documented customer confirmation','{}',now),
 ]);
 const operationalException={id:operationalExceptionId},queuePolicy={id:queuePolicyId};
 for(const participant of [author,reviewer,operator]){
  for(const source of [task,showcaseSource,showcaseTarget,exceptionSource])await getTask(input.h001,participant,String(source.id));
  const post=await postView(input.h001,participant,String(socialPost.id)),workspace=await collaborationWorkspace(participant),exception=await operationsExceptionDetail(participant,String(operationalException.id));
  if(!post.linkedSources.some(source=>source.available&&source.id===showcaseSource.id)||!workspace.relations.some(row=>row.id===relation.id)||!workspace.cards.some(row=>row.id===card.id)||exception.row.source.id!==exceptionSource.id)throw new Error('NATIVE_SHOWCASE_PARTICIPANT_READBACK_REQUIRED');
 }
 const prompt='Only propose explicitly supplied task fields. Preserve the source reference and wait for a person to review and submit.',agentAt=Date.now(),agentId=`agent-${key()}`,versionId=`agent-version-${key()}`,assignmentId=`agent-assignment-${key()}`;
 await input.h001.batch([
  input.h001.prepare("INSERT INTO agent_catalog(id,company_id,name,purpose,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,?,'approved',?,?,1,?,?)").bind(agentId,workplace.company,'Engineering proposal assistant','Propose task fields for explicit human review',author.userId,reviewer.userId,agentAt,agentAt),
  input.h001.prepare("INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,prohibited_actions_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,1,?,?,'[\"approve\",\"sign\",\"pay\"]','approved',?,?,1,?,?)").bind(versionId,agentId,prompt,'[\"/home\",\"/staff-tasks\"]',author.userId,reviewer.userId,agentAt,agentAt),
  input.h001.prepare("INSERT INTO agent_assignments(id,company_id,agent_version_id,pool_id,user_id,scope_path,status,created_by,revoked_by,version,created_at,updated_at) VALUES(?,?,?,NULL,?,'/','active',?,NULL,1,?,?)").bind(assignmentId,workplace.company,versionId,operator.userId,author.userId,agentAt,agentAt),
 ]);
 const agent={id:agentId},version={id:versionId},assignment={id:assignmentId};
 const proposalId=key(),proposalInput=guidedIntakeInputSchema.parse({message:'Create a Fictional proposal task for human review',contextPath:`/staff-tasks/${proposalSource.id}`});
 let approvedPromptUsed=false;
 const proposal=await prepareGuidedIntake(input.h001,operator,{AI_BASE_URL:'https://opencode.ai/zen/go/v1',AI_MODEL:'muse-spark-1.3-contributor',AI_API_KEY:'declared-no-network-engineering-fixture'},proposalInput,proposalId,async(_endpoint,messages)=>{
  approvedPromptUsed=messages.some(message=>typeof message.content==='string'&&message.content.includes(prompt));if(!approvedPromptUsed)throw new Error('APPROVED_PROPOSAL_PROMPT_NOT_USED');
  const content=messages.at(-1)!.content;if(typeof content!=='string')throw new Error('DECLARED_PROPOSAL_SOURCE_REQUIRED');const sources=(JSON.parse(content) as {sources:{id:string}[]}).sources;
  return {result:{intent:'task',summary:'Declared fixture proposal; review the fields before submitting',draft:{title:'Fictional proposal task',description:'Human review required; no command was submitted',approverId:'untrusted-field-must-be-dropped'},sourceIds:sources.map(source=>source.id).slice(0,1)},inputTokens:12,outputTokens:8};
 });
 const proposalEvidence=await input.h001.prepare('SELECT private_session_id FROM ai_workspace_turns WHERE id=?').bind(proposalId).first<{private_session_id:string}>();
 const content=await createMarketingAsset(author,{kind:'content',name:'Fictional equipment inquiry content',configuration:{reference:'Rights-cleared repository ERPX icon fixture; no connected advertising provider'},preview:{headline:'Fictional workplace equipment demonstration',summary:'A fictional equipment team can submit an inquiry for human review. This native form and sanitized repository icon use no advertising provider.'}},key());
 const image=await readFile(join(process.cwd(),'public/icon-192.png')),imageSet=await createAttachmentSet(input.h001,author,{targetType:'marketing_asset',targetId:String(content.id),componentKind:'record',purposeKey:'creative',expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(input.h001,author,imageSet.id,{filename:'declared-repository-erpx-icon.png',mediaType:'image/png',bytes:image.byteLength,caption:'Repository ERPX icon, used only in this fictional engineering fixture'},key()),uploaded=await completeAttachmentUpload(input.h001,input.h001Files,author,reserved.uploadIntentId,Uint8Array.from(image).buffer);
 if(!await drainAttachmentScans(input.h001,input.h001Files,uploaded.sourceId))throw new Error('NATIVE_MARKETING_CREATIVE_SCAN_REQUIRED');
 await bindRecordAttachmentSet(input.h001,author,imageSet.id,1,key());
 const media=await createAttachmentPublication(input.h001,input.h001Files,author,{sourceLinkId:reserved.attachmentLinkId,targetType:'marketing_asset',targetId:String(content.id),altText:'Declared ERPX icon for the fictional equipment inquiry',ordinal:0},key());if(!media)throw new Error('NATIVE_MARKETING_PUBLICATION_REQUIRED');
 await approveAttachmentPublication(input.h001,reviewer,String(media.id),key());await changeMarketing(reviewer,'asset',String(content.id),{action:'activate',expectedVersion:2,reason:'Independent review of declared content, repository image rights and sanitized public derivative'},key());
 const form=await createMarketingAsset(author,{kind:'form',name:'Fictional workplace equipment inquiry',configuration:{consentRequired:true,contentAssetId:String(content.id),contentVersion:3,mediaPublicationId:String(media.id)},preview:{headline:'Tell the fictional Sales team what you need',summary:'Engineering sample only; no real outreach'}},key());
 await changeMarketing(reviewer,'asset',String(form.id),{action:'activate',expectedVersion:1,reason:'Independent review of the fictional consent fields and inquiry preview'},key());
 const candidate=await createMarketingCandidate(author,{formId:form.id,formVersion:2,branchId:workplace.branch,contactName:'Fictional Bangkok Equipment Team',contactDetail:'equipment-inquiry@simulation.test',consentEvidence:'Declared fictional consent fixture; not a real prospect or outreach authorization',attribution:{campaign:'Engineering equipment demonstration',source:'Declared fictional form fixture'},brief:'Fictional team requests two equipment sets for a reviewable owner handoff'},key());
 const marketingDraft=await createMarketingAsset(author,{kind:'content',name:'Fictional equipment campaign draft',configuration:{fixture:'Declared engineering content; no connected advertising provider'},preview:{headline:'Fictional workplace equipment demonstration',description:'Draft remains unpublished until a separate reviewer approves'}},key());
 // Sensitive native IAM seeds use a real local credential sign-in and the
 // regular authenticator enrollment/verification service, never fabricated
 // session or verified-MFA rows. Their private secret is retained for the
 // operator's normal browser verification after the compiled cohort starts.
 const origin=process.env.ERPX_PUBLIC_ORIGIN??'http://127.0.0.1:3104';
 if(!['http://127.0.0.1:3102','http://127.0.0.1:3104'].includes(origin))throw new Error('OWNED_BROWSER_ORIGIN_REQUIRED');
 const auth=await createAuth(origin),login=await auth.api.signInEmail({body:{email:author.email,password},headers:new Headers({Origin:origin})});
 const nativeSession=await input.control.prepare('SELECT id,"expiresAt" FROM session WHERE token=? AND "userId"=?').bind(login.token,author.userId).first<{id:string;expiresAt:string}>();
 if(!nativeSession)throw new Error('NORMAL_IAM_SEED_SESSION_REQUIRED');
 const nativeGrant=await input.h001.prepare('SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=?').bind(author.assignmentId,author.userId,workplace.company).first<{version:number}>();
 if(!nativeGrant)throw new Error('NORMAL_IAM_SEED_ASSIGNMENT_REQUIRED');
 const selection=await selectWorkRole(input.h001,{sessionId:nativeSession.id,userId:author.userId,companyId:workplace.company,assignmentId:author.assignmentId,assignmentVersion:nativeGrant.version,revision:0,expiresAt:Date.parse(nativeSession.expiresAt)});
 const iamActor:IamActor={...author,sessionId:nativeSession.id,authority:{sessionId:nativeSession.id,revision:selection.revision,assignmentVersion:nativeGrant.version},iam:await loadIamAuthority(input.h001,workplace.company,author.userId,author.role,nativeSession.id),scopeKind:'holding',scopeNodeId:null,platform:false};
 const enrollment=await beginMfa(iamActor),counter=Math.floor(Date.now()/30000);await verifyMfa(iamActor,totp(enrollment.secret,counter));
 for(const project of ['desktop','mobile'])await privateJson(join(input.directory,`workplace-platform-mfa-${project}.json`),{[author.userId]:{secret:enrollment.secret,counter}});
 const directoryInput=connectionInput.parse({expectedVersion:0,name:'Declared fictional directory · provider unavailable',provider:'active_directory',config:{mode:'ldaps',secretProfile:'engineering_no_provider_account',authority:'Declared engineering fixture; no live provider account',ldapUrl:'ldaps://directory.engineering.invalid:636',baseDn:'DC=engineering,DC=invalid'}});
 const directoryConnection=await saveConnection(input.h001,iamActor,directoryInput,key());
 // Native owner rows only: the access-review command itself is native and
 // cannot be driven from this TypeScript fixture.
 const campaign={id:`access-review-${key()}`,itemCount:0};
 await input.h001.prepare("INSERT INTO iam_access_review_campaigns(id,company_id,name,due_at,status,version,created_by,reason,created_at,updated_at) VALUES(?,?,?,?,'open',1,?,?,?,?)")
  .bind(campaign.id,workplace.company,'Fictional Company access review',now+7*86400000,author.userId,'Review the native frozen assignment source snapshots for this fictional Company',now,now).run();
 const campaignItems=await input.h001.prepare("SELECT id,user_id,role,scope_kind,scope_node_id,source,valid_until,version FROM assignments WHERE company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,?,?) ORDER BY user_id,role,id")
  .bind(workplace.company,now,workplace.company,now).all<{id:string;user_id:string;role:string;scope_kind:string;scope_node_id:string|null;source:string;valid_until:number|null;version:number}>();
 for(const [index,item] of campaignItems.results.entries())
  await input.h001.prepare("INSERT INTO iam_access_review_items(id,campaign_id,company_id,assignment_id,assignment_version,user_id,role,scope_kind,scope_node_id,source,valid_until) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
   .bind(`access-review-item-${campaign.id}-${index}`,campaign.id,workplace.company,item.id,item.version,item.user_id,item.role,item.scope_kind,item.scope_node_id,item.source,item.valid_until).run();
 campaign.itemCount=campaignItems.results.length;
 const deadLetterEventId=`engineering-dead-letter-${key()}`;
 await input.h001.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at,attempts,last_error,dead_letter_at) VALUES(?,?,'engineering.fixture.retry','engineering_fixture',?,1,'{}',?,5,'DECLARED_ENGINEERING_FAULT',?)").bind(deadLetterEventId,key(),workplace.company,now,now).run();
 const targetMonth=new Date(Date.now()+7*3600000).toISOString().slice(0,7);
 const meterRefresh=await refreshRecentUsageProjections(input.control,[{id:'h001',slug:'simulation',displayName:'Fictional H001'},{id:'h002',slug:'riverside-lantern',displayName:'Fictional H002'}]);
 if(meterRefresh.unavailable.length)throw new Error('NATIVE_ENGINEERING_METERS_UNAVAILABLE');
 const guide={run:input.run,companyId:workplace.company,realmCompanyId:realm.company,holdingId:'h001',realmHoldingId:'h003',task:{id:task.id,version:task.version},agent:{id:agent.id,versionId:version.id,assignmentId:assignment.id,prompt,allowedSources:['/home','/staff-tasks']},proposal:{id:proposalId,input:proposalInput,result:proposal,sessionId:proposalEvidence!.private_session_id,sourceId:proposalSource.id,approvedPromptUsed,provider:'Declared no-network generation adapter; no provider acceptance'},marketing:{formId:form.id,formVersion:2,candidateId:candidate.id,draftId:marketingDraft.id,contentId:content.id,contentVersion:3,publicationId:media.id,creativeSetId:imageSet.id,provider:'No advertising provider account; fictional consent, reviewed native content and scanned repository icon fixture only'},iam:{connectionId:directoryConnection.id,connectionInput:directoryInput,campaignId:campaign.id,itemCount:campaign.itemCount,subjectUserId:iamSubject.userId,subjectAssignmentId:iamSubject.assignmentId,provider:'No directory provider account or credential profile; draft and fail-before-I/O recovery fixture only'},deadLetter:{id:deadLetterEventId,holdingId:'h001',attempts:5,lastError:'DECLARED_ENGINEERING_FAULT'},targetMonth,scope:'Isolated engineering only; fictional Companies and normal authentication; certification remains pending'};
 const demoGuide=Object.assign(guide,{brand:{draftId:brandDraft.id,status:'draft',published:false},social:{draftId:socialDraft.id,postId:socialPost.id,audience:'Selected fictional author, reviewer and operator only'},collaboration:{roomId:room.id,messageId:message.id,relationId:relation.id,actionCardId:card.id,projectId:showcaseProject.id,sourceId:showcaseSource.id,targetId:showcaseTarget.id,participantReadback:['author','reviewer','operator']},operations:{exceptionId:operationalException.id,sourceId:exceptionSource.id,policyId:queuePolicy.id,policyVersion:2,status:'open'}});
 await privateJson(join(input.directory,'workplace-platform-credentials.json'),{run:input.run,password,users});await privateJson(join(input.directory,'workplace-platform-guide.json'),demoGuide);
 return demoGuide;
}
