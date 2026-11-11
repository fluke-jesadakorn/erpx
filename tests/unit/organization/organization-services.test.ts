import {afterEach,describe,expect,it,vi} from 'vitest';
import {createHoldingDirectoryApplication} from "@erpx/server-organization/application/holding-directory";
import {createMemberProfileApplication,type MemberProfilePorts} from "@erpx/server-organization/application/member-profiles";
import {profileInput} from "@erpx/server-organization/domain/member-profiles";
import {createOrganizationWorkApplication} from "@erpx/server-organization/application/organization-work";
import {createStaffDefaultApplication,type StaffDefaultPorts} from "@erpx/server-organization/application/staff-defaults";
import {leaveDefaultRecommendation,type DefaultRecommendation} from "@erpx/server-organization/domain/staff-defaults";
const actor={holdingId:'holding',companyId:'company',scope:{holdingId:'holding',companyId:'company'}};
function directoryFixture(){const ports={actor,canManagePeople:false,bindingId:vi.fn(async()=> 'holding'),assignmentActive:vi.fn(async()=>true),authorizeManagePeople:vi.fn(async()=>{}),directory:vi.fn(async()=>({companies:[],nodes:[],levels:[],positions:[],people:[]})),archived:vi.fn(async()=>[]),selection:vi.fn(async(_selection:string,archived:boolean)=>({archived}))};return {ports,app:createHoldingDirectoryApplication(ports)};}
function profileFixture(){const ports:MemberProfilePorts<{id:string;created_at:number}>={clock:{now:()=>42},actor:{holdingSlug:'holding slug',userId:'viewer'},activeMember:vi.fn(async()=>true),account:vi.fn(async(id)=>({id,name:'Member'})),profile:vi.fn(async()=>null),work:vi.fn(async()=>[{company_name:'Company',job_title:'Title'}]),postIds:vi.fn(async()=>Array.from({length:9},(_,i)=>({id:`post-${i}`}))),posts:vi.fn(async(ids:string[])=>ids.map((id:string,i:number)=>({id,created_at:100-i}))),replay:vi.fn(async()=>null),save:vi.fn(async(_key,_id,input)=>({version:input.version+1,bio:input.bio,links:input.links})),media:vi.fn(async()=>null)};return {ports,app:createMemberProfileApplication(ports)};}
function defaultFixture(){const ports:StaffDefaultPorts<string,{pay:unknown[];leave:unknown[]}>={clock:{now:()=>Date.parse('2026-10-06T12:00:00Z')},canSeePay:true,authorizePeople:vi.fn(async()=>{}),authorizeLeavePolicy:vi.fn(async()=>{}),authorizeCompanyLeavePolicy:vi.fn(async()=>{}),catalog:vi.fn(async()=>({pay:[],leave:[]})),replay:vi.fn(async()=>null),position:vi.fn(async()=>true),payGroup:vi.fn(async()=>true),payVersion:vi.fn(async()=>0),level:vi.fn(async()=>({level_number:3})),leaveTypes:vi.fn(async()=>[{id:'annual',code:'annual'},{id:'sick',code:'sick'}]),leaveVersion:vi.fn(async()=>0),savePay:vi.fn(async(m)=>({id:m.id,version:m.input.expectedVersion+1})),saveLeave:vi.fn(async(m)=>({id:m.id,version:m.input.expectedVersion+1,policyVersionId:`policy-${m.key}`})),priorCompensation:vi.fn(async()=>false),payDefault:vi.fn(async()=>({pay_group_id:'group',currency:'THB',monthly_amount_minor:10000,version:1})),leaveDefault:vi.fn(async()=>({policy_version_id:'policy',effective_from:'2026-01-01'})),existingLeave:vi.fn(async()=>null),stagePay:vi.fn(async()=>['pay-stage']),stageLeave:vi.fn(async()=>['leave-stage'])};return {ports,app:createStaffDefaultApplication(ports)};}
const recommendation:DefaultRecommendation={engagementId:'engagement',positionId:'position',levelId:'level-3',priorLevelId:'level-4',startDate:'2026-10-06',currency:'THB',acceptPay:true,acceptLeave:true,commandId:'command'};
describe('Organization directory and profile applications',()=>{
 it('requires the selected holding binding and scope before any directory read',async()=>{const f=directoryFixture();f.ports.bindingId=vi.fn(async()=> 'foreign');await expect(f.app.directory()).rejects.toMatchObject({code:'HOLDING_SCOPE_REQUIRED'});expect(f.ports.assignmentActive).not.toHaveBeenCalled();expect(f.ports.directory).not.toHaveBeenCalled();});
 it('rejects a revoked assignment before directory facts and preserves HR-only archive visibility',async()=>{const f=directoryFixture();f.ports.assignmentActive=vi.fn(async()=>false);await expect(f.app.directory()).rejects.toMatchObject({code:'HOLDING_DIRECTORY_FORBIDDEN'});expect(f.ports.directory).not.toHaveBeenCalled();f.ports.assignmentActive=vi.fn(async()=>true);expect(await f.app.selection('person',true)).toEqual({archived:false});f.ports.authorizeManagePeople=vi.fn(async()=>{throw new Error('HR authority required');});await expect(f.app.archived()).rejects.toThrow('HR authority required');expect(f.ports.archived).not.toHaveBeenCalled();});
 it('checks target membership before account, work and social facts',async()=>{const f=profileFixture();f.ports.activeMember=vi.fn(async()=>false);await expect(f.app.view('target')).rejects.toMatchObject({code:'PROFILE_NOT_FOUND'});expect(f.ports.account).not.toHaveBeenCalled();expect(f.ports.postIds).not.toHaveBeenCalled();});
 it('uses the ninth post only as a pagination sentinel and retains the eighth post cursor',async()=>{const f=profileFixture();const view=await f.app.view('target',120,'previous');expect(view.posts).toHaveLength(8);expect(view).toMatchObject({own:false,version:0,nextBefore:93,nextBeforeId:'post-7',work:[{company:'Company',title:'Title'}]});expect(f.ports.postIds).toHaveBeenCalledWith('target',120,'previous');expect(f.ports.posts).toHaveBeenCalledWith(Array.from({length:8},(_,i)=>`post-${i}`),20);});
 it('revalidates profile membership before returning a retained save receipt',async()=>{const f=profileFixture(),input={bio:'Profile',links:[],version:2};f.ports.replay=vi.fn(async()=>({version:3,bio:'Profile',links:[]}));expect(await f.app.save(input,'key')).toEqual({version:3,bio:'Profile',links:[]});expect(f.ports.activeMember).toHaveBeenCalledWith('viewer',42);expect(f.ports.save).not.toHaveBeenCalled();});
 it('keeps strict HTTPS profile links and unavailable media behavior',async()=>{expect(()=>profileInput.parse({bio:'',links:[{label:'Unsafe',url:'http://example.com'}],version:0})).toThrow('Use an HTTPS link');const f=profileFixture();await expect(f.app.media('target','avatar')).rejects.toMatchObject({code:'PROFILE_MEDIA_NOT_FOUND'});});
});
describe('Organization Work access',()=>{
 it('requires Holding scope before checking the fresh assignment and reading changes',async()=>{const fresh=vi.fn(async()=>true),rows=vi.fn(async()=>[]),app=createOrganizationWorkApplication({canManage:true,assignmentScope:'branch',scopeKind:'branch',companyId:'company-1',userId:'user',samePerson:()=>false,freshAssignment:fresh,exists:vi.fn(async()=>true),rows});expect(await app.work()).toEqual([]);expect(fresh).not.toHaveBeenCalled();expect(rows).not.toHaveBeenCalled();});
 it('does not turn a stale notification into an access grant',async()=>{const exists=vi.fn(async()=>true),app=createOrganizationWorkApplication({canManage:true,assignmentScope:'holding',scopeKind:'holding',companyId:'company-1',userId:'user',samePerson:()=>false,freshAssignment:vi.fn(async()=>false),exists,rows:vi.fn(async()=>[])});expect(await app.notificationTarget('change')).toBeNull();expect(exists).not.toHaveBeenCalled();});
 it('routes organization change work directly to its company and review section',async()=>{const app=createOrganizationWorkApplication({canManage:true,assignmentScope:'holding',scopeKind:'holding',companyId:'company-1',userId:'reviewer',samePerson:()=>false,freshAssignment:vi.fn(async()=>true),exists:vi.fn(async()=>true),rows:vi.fn(async()=>[{id:'change-1',company_id:'company-1',target_node_id:null,action:'create' as const,node_kind:'branch' as const,slug:'north',display_name:'North',branch_id:null,source_branch_id:null,expected_node_version:null,state:'submitted' as const,version:2,requested_by:'requester',reviewed_by:null,reason:'Open a northern branch.',review_reason:null,created_at:1,updated_at:2,reviewed_at:null,applied_at:null,same_requester:false}])});const item=(await app.work())[0];expect(item?.href).toBe('/people/organization?view=legacy&company=company-1&tab=changes&changeId=change-1#organization-change-change-1');expect(item?.canAct).toBe(true);});
});
describe('Organization staff default application',()=>{
 it('requires compensation authority before pay replay and mutable reads',async()=>{const f=defaultFixture();f.ports.canSeePay=false;await expect(f.app.savePay('position',{payGroupId:'group',currency:'THB',monthlyAmount:10000,expectedVersion:0},'key')).rejects.toMatchObject({code:'PAY_ACCESS_REQUIRED'});expect(f.ports.replay).not.toHaveBeenCalled();expect(f.ports.position).not.toHaveBeenCalled();});
 it('rejects changed position defaults before staging a replacement',async()=>{const f=defaultFixture();f.ports.payVersion=vi.fn(async()=>1);await expect(f.app.savePay('position',{payGroupId:'group',currency:'THB',monthlyAmount:10000,expectedVersion:0},'key')).rejects.toMatchObject({code:'DEFAULT_CHANGED'});expect(f.ports.savePay).not.toHaveBeenCalled();});
 it('requires current Company leave authority before returning an old receipt',async()=>{const f=defaultFixture();f.ports.authorizeCompanyLeavePolicy=vi.fn(async()=>{throw new Error('Company authority required');});await expect(f.app.saveLeave('level',{annualDays:10,sickDays:30,expectedVersion:0},'key')).rejects.toThrow('Company authority required');expect(f.ports.replay).not.toHaveBeenCalled();});
 it('does not replace existing compensation or a custom leave policy',async()=>{const f=defaultFixture();f.ports.priorCompensation=vi.fn(async()=>true);f.ports.existingLeave=vi.fn(async()=>({id:'custom',effective_from:'2026-01-01',effective_to:null,policy_key:'custom-approved'}));expect(await f.app.recommendation(recommendation)).toEqual([]);expect(f.ports.stagePay).not.toHaveBeenCalled();expect(f.ports.stageLeave).not.toHaveBeenCalled();});
 it('applies a changed Level leave default next year and ends the existing managed assignment',()=>{expect(leaveDefaultRecommendation(recommendation,{policy_version_id:'new-policy',effective_from:'2026-01-01'},{id:'old',effective_from:'2025-01-01',effective_to:null,policy_key:'level-4-default'})).toEqual({policyVersionId:'new-policy',effective:'2027-01-01',end:'2026-12-31',endAssignmentId:'old'});});
 it('preserves effective-date overlap guards and stages both recommendations without committing',async()=>{expect(()=>leaveDefaultRecommendation(recommendation,{policy_version_id:'new',effective_from:'2026-01-01'},{id:'overlap',effective_from:'2027-01-01',effective_to:null,policy_key:'legacy-untracked'})).toThrow('Review the current leave assignment');const f=defaultFixture();expect(await f.app.recommendation(recommendation)).toEqual(['pay-stage','leave-stage']);expect(f.ports.stageLeave).toHaveBeenCalledWith(recommendation,expect.objectContaining({effective:'2027-01-01'}),Date.parse('2026-10-06T12:00:00Z'));});
});

import {createWorkspaceChoiceApplication,type WorkspaceChoicePorts} from "@erpx/server-organization/application/workspace-choices";
import {createDepartmentOverrideApplication,type DepartmentOverridePorts} from "@erpx/server-organization/application/department-decisions";
import {overtimeOverrideAction} from "@erpx/server-organization/domain/department-decisions";
import {createDepartmentAuthorityApplication,type DepartmentAuthorityPorts} from "@erpx/server-organization/application/department-authority";
import {appointmentEffectiveAt,authorityPower,authorityReportingChain} from "@erpx/server-organization/domain/department-authority";
import type {StaffPerson} from "@erpx/server-operations/public/staff-policy";
const staff=(id:string,level:number,manager:string|null=null):StaffPerson=>({person_id:id,user_id:`user-${id}`,legal_name:id,engagement_id:`engagement-${id}`,engagement_version:2,company_id:'company',branch_id:null,department_id:'dept',worker_type:'employee',start_date:'2026-01-01',end_date:null,manager_person_id:manager,level_id:`level-${level}`,level_name:`Level ${level}`,level_rank:level,level_number:level,position_id:`position-${id}`});
function authorityFixture(){const people=[staff('requester',3,'manager'),staff('manager',2),staff('hr',2)],ports:DepartmentAuthorityPorts<Record<string,unknown>>={clock:{now:()=>1_000_000},actor:{companyId:'company',userId:'user-owner',role:'holding_owner',assignmentScope:'holding'},personForActor:vi.fn(async()=> 'owner'),currentPowerKinds:vi.fn(async()=>[]),seat:vi.fn(async(kind,scope)=>kind==='head'?{company_id:'company',kind,scope_id:scope,version:1,position_id:'position-requester',person_id:'requester',effective_at:1}:null),overview:vi.fn(async()=>({seats:[],history:[],policies:[],departments:[]})),activeStaff:vi.fn(async(userId)=>people.find(p=>p.user_id===userId)??staff('owner',1)),staffPeople:vi.fn(async()=>people),replay:vi.fn(async()=>null),departmentActive:vi.fn(async()=>true),positionValid:vi.fn(async()=>true),appointeeUser:vi.fn(async()=> 'user-owner'),selectorUser:vi.fn(async()=> 'user-manager'),flowVersion:vi.fn(async()=>1),policy:vi.fn(async()=>null),legacyHead:vi.fn(async()=>null),legacyHrUsers:vi.fn(async()=>['user-hr']),change:vi.fn(async(input,_key,_power,scope)=>({kind:input.kind,scopeId:scope,version:input.expectedVersion+1,personId:input.personId,positionId:input.positionId})),publish:vi.fn(async(input)=>({departmentId:input.departmentId,version:input.expectedVersion+1,steps:input.steps}))};return {people,ports,app:createDepartmentAuthorityApplication(ports)};}
describe('Organization workspace choices and department routing',()=>{
 it('does not read brands or nodes for assignments whose role no longer permits the scope',async()=>{const ports:WorkspaceChoicePorts={clock:{now:()=>42},userId:vi.fn(async()=> 'user'),holdings:vi.fn(async()=>[{id:'holding',slug:'holding',displayName:'Holding'}]),assignments:vi.fn(async()=>[{role:'revoked-role',company_id:'company',company_name:'Company',scope_kind:'holding' as const,scope_node_id:null}]),roleScopes:vi.fn(async()=>new Map()),nodes:vi.fn(async()=>[]),brands:vi.fn(async()=>[])};expect(await createWorkspaceChoiceApplication(ports).all()).toEqual([]);expect(ports.nodes).not.toHaveBeenCalled();expect(ports.brands).not.toHaveBeenCalled();expect(ports.assignments).toHaveBeenCalledWith('holding','user',undefined,42);});
 it('does not load another Holding while selecting an assignment',async()=>{const ports:WorkspaceChoicePorts={clock:{now:()=>42},userId:vi.fn(async()=> 'user'),holdings:vi.fn(async()=>[{id:'holding',slug:'holding',displayName:'Holding'}]),assignments:vi.fn(async()=>[]),roleScopes:vi.fn(async()=>new Map()),nodes:vi.fn(async()=>[]),brands:vi.fn(async()=>[])};expect(await createWorkspaceChoiceApplication(ports).forAssignment('foreign','assignment')).toEqual([]);expect(ports.assignments).not.toHaveBeenCalled();});
 it('keeps the retained overtime operation on lost-response replay after state has advanced',()=>{expect(overtimeOverrideAction('approve','awaiting_verification','overtime.authorize')).toBe('authorize');expect(overtimeOverrideAction('reject','pending_authorization',null)).toBe('reject');expect(overtimeOverrideAction('reject','awaiting_verification',null)).toBe('return');});
 it('blocks unavailable department overrides before delegated mutations',async()=>{const decideExpense=vi.fn(async()=> 'saved'),ports={receiptOperation:vi.fn(async()=>null),approval:vi.fn(async()=>({canOverride:false,currentApprover:null})),decideExpense} as unknown as DepartmentOverridePorts<string>,app=createDepartmentOverrideApplication(ports);await expect(app.decide({kind:'expense',id:'expense',expectedVersion:1,outcome:'approve',reason:'Independent review'},'key')).rejects.toMatchObject({code:'OVERRIDE_UNAVAILABLE'});expect(decideExpense).not.toHaveBeenCalled();});
 it('delegates a retained override receipt before reconsidering the now completed approval step',async()=>{const approval=vi.fn(async()=>{throw new Error('completed');}),decideExpense=vi.fn(async()=> 'retained'),ports={receiptOperation:vi.fn(async()=> 'expense.approve'),approval,decideExpense} as unknown as DepartmentOverridePorts<string>;expect(await createDepartmentOverrideApplication(ports).decide({kind:'expense',id:'expense',expectedVersion:1,outcome:'approve',reason:'Independent review'},'key')).toBe('retained');expect(approval).not.toHaveBeenCalled();});
 it('limits appointment power to owner, CEO for HR/head and HR for head',()=>{expect(authorityPower(false,'person',['hr'],'ceo')).toBeNull();expect(authorityPower(false,'person',['ceo'],'ceo')).toBeNull();expect(authorityPower(false,'person',['ceo'],'hr')).toBe('ceo');expect(authorityPower(false,'person',['hr'],'head')).toBe('hr');expect(authorityPower(true,null,[],'ceo')).toBe('owner');});
 it('returns an appointment receipt before stale seat validation and preserves scope validation',async()=>{const f=authorityFixture(),input={kind:'ceo' as const,departmentId:null,positionId:'position',personId:'owner',expectedVersion:2,reason:'Owner appointment'};f.ports.replay=vi.fn(async()=>({kind:'ceo',scopeId:'company',version:3,personId:'owner',positionId:'position'})) as typeof f.ports.replay;expect(await f.app.change(input,'key')).toMatchObject({version:3});expect(f.ports.seat).not.toHaveBeenCalled();await expect(f.app.change({...input,departmentId:'dept'},'key')).rejects.toMatchObject({code:'APPOINTMENT_SCOPE'});});
 it('requires a current seat version and a recent effective time before the atomic appointment port',async()=>{const f=authorityFixture();await expect(f.app.change({kind:'head',departmentId:'dept',positionId:null,personId:null,expectedVersion:0,reason:'Head change'},'key')).rejects.toMatchObject({code:'VERSION_CONFLICT'});expect(f.ports.change).not.toHaveBeenCalled();expect(()=>appointmentEffectiveAt({kind:'hr',departmentId:null,positionId:null,personId:null,expectedVersion:0,reason:'HR appointment',effectiveAt:1_000_001},1_000_000)).toThrow('Appointments take effect now');});
 it('escalates a self-review to a higher manager and snapshots semantic concurrency checks',async()=>{const f=authorityFixture(),route=await f.app.departmentRoute('user-requester');expect(route).toMatchObject({blockedReason:null,steps:[{userId:'user-manager',snapshot:{originalPersonId:'requester',selectedPersonId:'manager'}}],checks:[{kind:'staff-engagement',id:'engagement-manager',companyId:'company',version:2}]});expect(JSON.stringify(route.checks)).not.toContain('SELECT');});
 it('does not fall back to an independent person when a reporting cycle exists',async()=>{const f=authorityFixture();f.people[1].manager_person_id='requester';expect(()=>authorityReportingChain(f.people,'requester')).toThrow('reporting cycle');expect(await f.app.departmentRoute('user-requester')).toMatchObject({steps:[],blockedReason:'A reporting cycle must be corrected before submission.'});});
 it('requires an independent HR step after department review and preserves unavailable route blockers',async()=>{const f=authorityFixture();f.ports.legacyHrUsers=vi.fn(async()=>['user-manager','user-requester']);expect(await f.app.peopleRoute('user-requester')).toMatchObject({steps:[],blockedReason:'Appoint an independent HR reviewer for this request.'});f.ports.legacyHrUsers=vi.fn(async()=>['user-hr']);expect((await f.app.peopleRoute('user-requester')).steps.map(s=>s.userId)).toEqual(['user-manager','user-hr']);});
 it('rechecks a legacy head seat and position selector against current staff facts',async()=>{const f=authorityFixture();expect(await f.app.stepEligible('user-requester',{selectedPersonId:'requester',originalPersonId:'requester',departmentId:'dept'})).toBe(true);expect(await f.app.stepEligible('user-requester',{selectedPersonId:'requester',selector:{kind:'position',positionId:'former'}})).toBe(false);expect(await f.app.hrEligible('user-hr','hr')).toBe(false);});
});

import {createOrganizationApplication,type OrganizationPorts} from "@erpx/server-organization/application/organization";
import {organizationNodeChangeSchema,validateTemplate} from "@erpx/server-organization/domain/organization";
import {createPositionMoveApplication,type PositionMovePorts} from "@erpx/server-organization/application/position-moves";
import {createOrganizationDraftApplication,type OrganizationDraftPorts} from "@erpx/server-organization/application/organization-drafts";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {emptyDraft} from "@erpx/feature-organization/lib/organization-draft";
const organizationActor={companyId:'company',userId:'reviewer',assignmentScope:'holding',branchId:null,departmentId:null,scope:{kind:'holding'}} as WorkContext;
const nodeChange=(requested_by:string):import("@erpx/feature-organization/lib/organization").OrganizationNodeChange=>({id:'change',target_node_id:'department',action:'relocate',node_kind:'department',slug:'source',display_name:'Department',branch_id:'destination-branch',source_branch_id:'source-branch',expected_node_version:2,state:'submitted',version:1,requested_by,reason:'Move reviewed department',reviewed_by:null,review_reason:null,reviewed_at:null,applied_at:null,created_at:1,updated_at:1});
function nodeFixture(){const p={clock:{now:()=>42},authorize:vi.fn((a:WorkContext)=>a),replay:vi.fn(async()=>null),branchActive:vi.fn(async()=>true),node:vi.fn(async()=>({id:'department',kind:'department' as const,slug:'source',display_name:'Source department',branch_id:'source-branch',status:'active' as const,hierarchy_revision:2})),reservedDepartment:vi.fn(async()=>false),hasChildren:vi.fn(async()=>false),slugConflict:vi.fn(async()=>false),change:vi.fn(async():Promise<import("@erpx/feature-organization/lib/organization").OrganizationNodeChange|null>=>null),saveChange:vi.fn(async()=>({id:'change',version:1,state:'submitted'})),samePerson:vi.fn(()=>false),independentReviewer:vi.fn(async()=>true),attachmentSets:vi.fn(async()=>[]),prepareReviewBinding:vi.fn(async()=>({guards:[],steps:[]})),commitReview:vi.fn(async()=>({id:'change',version:2,state:'applied',nodeId:'node'}))} satisfies Partial<OrganizationPorts<string>>;return {p,app:createOrganizationApplication(p as unknown as OrganizationPorts<string>)};}
const relocate=organizationNodeChangeSchema.parse({action:'relocate',targetNodeId:'department',kind:'department',displayName:'Source department',branchId:'destination-branch',expectedNodeVersion:2,expectedVersion:0,reason:'Move reviewed department'});
describe('Organization catalog and atomic change applications',()=>{
 it('rejects reserved departments before constructing an organization change',async()=>{const f=nodeFixture();f.p.reservedDepartment=vi.fn(async()=>true);await expect(f.app.saveChange(organizationActor,null,relocate,'key',true)).rejects.toMatchObject({code:'ORGANIZATION_DRAFT_RESERVED'});expect(f.p.saveChange).not.toHaveBeenCalled();});
 it('validates hierarchy revisions and child units before relocation staging',async()=>{const f=nodeFixture();await expect(f.app.saveChange(organizationActor,null,{...relocate,expectedNodeVersion:1},'key',true)).rejects.toMatchObject({code:'NODE_VERSION_CHANGED'});f.p.hasChildren=vi.fn(async()=>true);await expect(f.app.saveChange(organizationActor,null,relocate,'key',true)).rejects.toMatchObject({code:'RELOCATION_HAS_CHILDREN'});expect(f.p.saveChange).not.toHaveBeenCalled();});
 it('rechecks Company authority before returning a save receipt',async()=>{const f=nodeFixture();await expect(f.app.saveChange({...organizationActor,assignmentScope:'branch'},null,relocate,'key',true)).rejects.toMatchObject({code:'COMPANY_AUTHORITY_REQUIRED'});expect(f.p.replay).not.toHaveBeenCalled();});
 it('requires an independent reviewer before reading or staging attachments',async()=>{const f=nodeFixture();f.p.change=vi.fn(async()=>nodeChange('same-person'));f.p.independentReviewer=vi.fn(async()=>false);await expect(f.app.reviewChange(organizationActor,'change',{expectedVersion:1,decision:'approve',reason:'Independent review',attachmentSetIds:['proof']},'key')).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});expect(f.p.attachmentSets).not.toHaveBeenCalled();expect(f.p.commitReview).not.toHaveBeenCalled();});
 it('rejects proof from a different purpose before binding any file',async()=>{const f=nodeFixture();f.p.change=vi.fn(async()=>nodeChange('requester'));f.p.attachmentSets=vi.fn(async()=>[{id:'proof',purpose_key:'expense',component_kind:'action',component_ref:'organization-change-review'}]) as typeof f.p.attachmentSets;await expect(f.app.reviewChange(organizationActor,'change',{expectedVersion:1,decision:'reject',reason:'Independent rejection',attachmentSetIds:['proof']},'key')).rejects.toMatchObject({code:'ORGANIZATION_ATTACHMENT_PURPOSE'});expect(f.p.prepareReviewBinding).not.toHaveBeenCalled();expect(f.p.commitReview).not.toHaveBeenCalled();});
 it('preserves unique template keys and more senior reporting positions',()=>{expect(()=>validateTemplate([{key:'ceo',title:'CEO',levelNumber:1,reportsToKey:null,isCeo:true,rosterRequired:false},{key:'ceo',title:'Duplicate',levelNumber:2,reportsToKey:'ceo',isCeo:false,rosterRequired:false}])).toThrow('keys must be unique');expect(()=>validateTemplate([{key:'ceo',title:'CEO',levelNumber:1,reportsToKey:null,isCeo:true,rosterRequired:false},{key:'peer',title:'Peer',levelNumber:1,reportsToKey:'ceo',isCeo:false,rosterRequired:false}])).toThrow('more senior');});
});
describe('Organization scheduled applications',()=>{
 it('returns a retained position-move receipt inside its provided transaction before expired preview checks',async()=>{const receipt={id:'move',version:1,status:'scheduled' as const,effectiveAt:100},replay=vi.fn(async()=>receipt),authorize=vi.fn(),clock={now:()=>1000};const p={clock,authorize,replay,transaction:async<T>(run:(ports:PositionMovePorts)=>Promise<T>)=>run(child)} as unknown as PositionMovePorts;const child:PositionMovePorts={...p,replay:replay as unknown as PositionMovePorts['replay']};expect(await createPositionMoveApplication(p).submit(organizationActor,{positionId:'position',expectedVersion:1,targetKind:'level',targetId:'level-2',effectiveAt:100,timezone:'UTC',managerAssignments:[],reason:'Reviewed position'},'token',500,'key')).toEqual(receipt);expect(replay).toHaveBeenCalledWith(organizationActor,'key','organization.position-move.submit','position-move-key',expect.any(Object));});
 it('rejects expired position previews before reading any organization facts',async()=>{const roster=vi.fn(async()=>({people:[],positions:[]})),p={clock:{now:()=>1000},authorize:vi.fn(),replay:vi.fn(async()=>null),roster,transaction:async<T>(run:(ports:PositionMovePorts)=>Promise<T>)=>run(p)} as unknown as PositionMovePorts;await expect(createPositionMoveApplication(p).submit(organizationActor,{positionId:'position',expectedVersion:1,targetKind:'level',targetId:'level-2',effectiveAt:2000,timezone:'UTC',managerAssignments:[],reason:'Reviewed position'},'token',999,'key')).rejects.toMatchObject({code:'POSITION_MOVE_PREVIEW_EXPIRED'});expect(roster).not.toHaveBeenCalled();});
 it('blocks scheduled work after assignment revocation before changing payroll date or People facts',async()=>{const setPayrollDate=vi.fn(async()=>{}),needsReview=vi.fn(async()=>{}),row={id:'move',actor_json:organizationActor,definition:{},preview:{affectedCount:2}},child={claim:vi.fn(async()=>row),assignmentValid:vi.fn(async()=>false),setPayrollDate} as unknown as PositionMovePorts,p={clock:{now:()=>42},transaction:async<T>(run:(ports:PositionMovePorts)=>Promise<T>)=>run(child),needsReview} as unknown as PositionMovePorts;expect(await createPositionMoveApplication(p).drain()).toBe(0);expect(setPayrollDate).not.toHaveBeenCalled();expect(needsReview).toHaveBeenCalledWith('move','POSITION_MOVE_AUTHORITY_CHANGED: The authorizing HR role changed. Review the move again.',42);});
 it('requires Company scope before any draft/private request reads',async()=>{const draft=vi.fn(async()=>null),p={authorize:vi.fn(),draft} as unknown as OrganizationDraftPorts;await expect(createOrganizationDraftApplication(p).get({...organizationActor,branchId:'branch'})).rejects.toMatchObject({code:'COMPANY_AUTHORITY_REQUIRED'});expect(draft).not.toHaveBeenCalled();});
 it('checks saved draft versions before reading replacement source snapshots',async()=>{const data=vi.fn(),draft=vi.fn(async()=>({id:'draft',state:'draft',version:3,definition:emptyDraft(),base_snapshot:{}})),p={authorize:vi.fn(),draft,requests:vi.fn(async()=>[]),recent:vi.fn(async()=>[]),data,transaction:async<T>(run:(ports:OrganizationDraftPorts)=>Promise<T>)=>run(p)} as unknown as OrganizationDraftPorts;await expect(createOrganizationDraftApplication(p).save(organizationActor,'draft',2,emptyDraft())).rejects.toMatchObject({code:'DRAFT_VERSION_CHANGED'});expect(data).not.toHaveBeenCalled();});
 it('waits for a department dependency before reading People placements or scheduling an update',async()=>{const data=vi.fn(),setState=vi.fn(),row={id:'request',change_id:null,definition:emptyDraft(),baseline:{},dependencies:[{oldId:'department',branchId:'branch',changeId:'change'}],actor_json:organizationActor,effective_at:100},child={claim:vi.fn(async()=>row),changeState:vi.fn(async()=>({state:'submitted'})),data,setState} as unknown as OrganizationDraftPorts,p={clock:{now:()=>42},scheduled:vi.fn(async()=>[{id:'request'}]),transaction:async<T>(run:(ports:OrganizationDraftPorts)=>Promise<T>)=>run(child)} as unknown as OrganizationDraftPorts;expect(await createOrganizationDraftApplication(p).drain()).toBe(0);expect(data).not.toHaveBeenCalled();expect(setState).not.toHaveBeenCalled();});
});

import {createProfileMediaCommandApplication,type ProfileMediaCommandPorts} from "@erpx/server-organization/application/profile-media-command";
import type {ProfileMediaResult} from "@erpx/server-organization/domain/profile-media-command";
function mediaCommandFixture(){const calls:string[]=[],ports:ProfileMediaCommandPorts={actor:{holdingId:'holding',userId:'member'},clock:{now:()=>42},identifiers:{next:()=> 'object-id'},readyUpload:vi.fn(async()=>{calls.push('active-and-files');}),readyRemoval:vi.fn(async()=>{calls.push('active');}),readyStoredRemoval:vi.fn(async()=>{calls.push('stored-resource');}),readBody:vi.fn(async()=>{calls.push('body');return new Uint8Array([1,2]).buffer;}),detectedMediaType:vi.fn((_bytes,declared)=>declared),digest:vi.fn(async()=>{calls.push('digest');return 'digest';}),scan:vi.fn(async()=>{calls.push('scan');return {clean:true};}),preview:vi.fn(async()=>{calls.push('preview');return {bytes:new Uint8Array([3,4]),mediaType:'image/webp'};}),store:vi.fn(async()=>{calls.push('store');return true;}),removeStored:vi.fn(async()=>{calls.push('delete');}),replayUpload:vi.fn(async():Promise<ProfileMediaResult|null>=>{calls.push('upload-replay');return null;}),replayRemoval:vi.fn(async():Promise<ProfileMediaResult|null>=>{calls.push('remove-replay');return null;}),imageAtVersion:vi.fn(async()=>({key:'existing-image'})),commitUpload:vi.fn(async(_key,_input,_object,_type,next)=>{calls.push('upload-commit');return {version:next};}),commitRemoval:vi.fn(async(_key,_input,_object,next)=>{calls.push('remove-commit');return {version:next};})};return {ports,calls,app:createProfileMediaCommandApplication(ports)};}
const profileUpload={kind:'avatar' as const,contentType:'image/png; charset=binary',contentLength:'2',version:'0',commandId:'receipt-key'};
describe('Profile media commands preserve live membership and file compensation',()=>{
 it('stages the sanitized image and original receipt input with supplied identifiers and clock',async()=>{const f=mediaCommandFixture();expect(await f.app.upload(profileUpload)).toEqual({version:1});expect(f.calls).toEqual(['active-and-files','body','digest','upload-replay','scan','preview','store','upload-commit']);expect(f.ports.commitUpload).toHaveBeenCalledWith('receipt-key',{kind:'avatar',expectedVersion:0,digest:'digest'},'holding/profiles/member/avatar/object-id.webp','image/webp',1,42);expect(f.ports.store).toHaveBeenCalledWith('holding/profiles/member/avatar/object-id.webp',new Uint8Array([3,4]),'image/webp','digest');});
 it('rechecks membership and content digest before replay, then skips scanner and storage',async()=>{const f=mediaCommandFixture();f.ports.replayUpload=vi.fn(async()=>({version:7}));expect(await f.app.upload(profileUpload)).toEqual({version:7});expect(f.calls).toEqual(['active-and-files','body','digest']);expect(f.ports.scan).not.toHaveBeenCalled();expect(f.ports.store).not.toHaveBeenCalled();});
 it('removes the newly stored image after a late journal failure and preserves the original failure',async()=>{const f=mediaCommandFixture(),failure=new Error('late outbox');f.ports.commitUpload=vi.fn(async()=>{throw failure;});f.ports.removeStored=vi.fn(async()=>{throw new Error('cleanup failure');});await expect(f.app.upload(profileUpload)).rejects.toBe(failure);expect(f.ports.removeStored).toHaveBeenCalledWith('holding/profiles/member/avatar/object-id.webp');});
 it('rejects unsafe content or scanner failure before storage or commit',async()=>{const f=mediaCommandFixture();f.ports.detectedMediaType=()=>null;await expect(f.app.upload(profileUpload)).rejects.toMatchObject({code:'PROFILE_IMAGE_TYPE'});expect(f.ports.replayUpload).not.toHaveBeenCalled();f.ports.detectedMediaType=(_bytes,type)=>type;f.ports.scan=vi.fn(async()=>{throw new Error('scanner offline');});await expect(f.app.upload(profileUpload)).rejects.toMatchObject({status:503,code:'PROFILE_SCANNER_UNAVAILABLE'});expect(f.ports.store).not.toHaveBeenCalled();f.ports.scan=vi.fn(async()=>({clean:false}));await expect(f.app.upload(profileUpload)).rejects.toMatchObject({status:422,code:'PROFILE_IMAGE_REJECTED'});});
 it('checks revoked membership before malformed input or either receipt',async()=>{const f=mediaCommandFixture();f.ports.readyUpload=vi.fn(async()=>{throw new Error('revoked');});await expect(f.app.upload({...profileUpload,contentLength:'invalid'})).rejects.toThrow('revoked');expect(f.ports.readBody).not.toHaveBeenCalled();expect(f.ports.replayUpload).not.toHaveBeenCalled();f.ports.readyRemoval=f.ports.readyUpload;await expect(f.app.remove({kind:'avatar',version:'1',commandId:'receipt-key'})).rejects.toThrow('revoked');expect(f.ports.replayRemoval).not.toHaveBeenCalled();});
 it('retains avatar and cover limits and optimistic header validation',async()=>{const f=mediaCommandFixture();await expect(f.app.upload({...profileUpload,contentLength:String(2*1024*1024+1)})).rejects.toMatchObject({code:'PROFILE_IMAGE_SIZE'});await expect(f.app.upload({...profileUpload,version:'-1'})).rejects.toMatchObject({code:'PROFILE_VERSION_REQUIRED'});await f.app.upload({...profileUpload,kind:'cover',contentLength:String(2*1024*1024+1)});expect(f.ports.readBody).toHaveBeenCalledWith(2*1024*1024+1);});
 it('removal replays before a source read and deletes only after successful commit',async()=>{const f=mediaCommandFixture(),input={kind:'avatar' as const,version:'1',commandId:'remove-key'};f.ports.replayRemoval=vi.fn(async()=>({version:2}));expect(await f.app.remove(input)).toEqual({version:2});expect(f.ports.imageAtVersion).not.toHaveBeenCalled();expect(f.ports.removeStored).not.toHaveBeenCalled();f.ports.replayRemoval=vi.fn(async()=>null);f.ports.commitRemoval=vi.fn(async()=>{throw new Error('late outbox');});await expect(f.app.remove(input)).rejects.toThrow('late outbox');expect(f.ports.removeStored).not.toHaveBeenCalled();f.ports.commitRemoval=vi.fn(async()=>({version:2}));f.ports.removeStored=vi.fn(async()=>{throw new Error('physical delete unavailable');});expect(await f.app.remove(input)).toEqual({version:2});expect(f.ports.removeStored).toHaveBeenCalledWith('existing-image');});
});

it('propagates post-commit profile storage readiness errors while swallowing only physical deletion failures',async()=>{const f=mediaCommandFixture();f.ports.readyStoredRemoval=vi.fn(async()=>{throw new Error('holding resource unavailable');});await expect(f.app.remove({kind:'avatar',version:'1',commandId:'remove-key'})).rejects.toThrow('holding resource unavailable');expect(f.ports.commitRemoval).toHaveBeenCalledOnce();expect(f.ports.removeStored).not.toHaveBeenCalled();f.ports.replayRemoval=vi.fn(async()=>({version:2}));expect(await f.app.remove({kind:'avatar',version:'1',commandId:'remove-key'})).toEqual({version:2});expect(f.ports.readyStoredRemoval).toHaveBeenCalledTimes(1);});

import {
  createNativeOrganizationClient,
  NativeOrganizationError,
  nativeOrganizationConfiguration,
  nativeOrganizationDetail,
  nativeOrganizationFailure,
  nativeOrganizationOverview,
} from "../../../packages/feature-organization/src/adapters/server/native-organization";
import {
  nativeOrganizationAccessPage,
  nativeOrganizationPositionMoveCommand,
  nativeOrganizationPositionMoveList,
  nativeOrganizationPage,
  nativeOrganizationSelector,
  nativeOrganizationWorkspace,
  nativePositionMoveSelector,
} from "../../../packages/feature-organization/src/lib/native-organization";
import type { OrganizationAccessData } from "../../../packages/feature-organization/src/lib/organization-access";
import type { OrganizationOverview } from "../../../packages/feature-organization/src/lib/organization-domain";

const nativeActor = {
  userId: "user",
  holdingId: "h001",
  holdingSlug: "acme",
  companyId: "company",
  branchId: null,
  departmentId: null,
  assignmentId: "assignment",
  role: "custom_organization",
  assignmentScope: "holding",
  scope: { kind: "holding" as const },
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ["people"],
  iam: { permissions: ["organization.manage"] },
} as unknown as WorkContext;
const nativeConfig = {
  privateBaseUrl: "http://127.0.0.1:3199",
  holdings: [{ origin: "https://acme.example.test", holdingSlug: "acme", holdingId: "h001" }],
  timeoutMs: 5000,
};
const nativeHeaders = { get: (name: string) => ({ host: "acme.example.test", cookie: "session=value" })[name] ?? null };

function nativeTransport(payload: unknown, status = 200) {
  return vi.fn(async (method: string, url: URL, headers: Headers, body: string | undefined, signal: AbortSignal) => {
    void method; void url; void headers; void body; void signal;
    return new Response(JSON.stringify(payload), {
      status,
      headers: { "content-type": "application/json; charset=utf-8" },
    });
  });
}

const workspacePayload = {
  data: {
    nodes: [
      {
        id: "branch",
        kind: "branch",
        slug: "north",
        display_name: "North",
        parent_id: "company",
        company_id: "company",
        branch_id: "branch",
        status: "active",
        hierarchy_revision: 1,
      },
    ],
    nodeChanges: [],
    changeQueue: { recordId: null, state: "open", query: "", page: 1, pageSize: 20, total: 0, pages: 1 },
    authorizedChannelNodes: ["branch"],
    channels: [{ node_id: "branch", room_id: "org-channel-branch", name: "North", kind: "branch", status: "active" }],
    positions: [
      {
        id: "position-branch",
        company_id: "company",
        branch_id: "branch",
        department_id: null,
        title: "North lead",
        level_id: "level-1",
        level_number: 1,
        reports_to_position_id: null,
        requirements: "",
        is_ceo: 1,
        roster_required: 0,
        active: 1,
        version: 1,
      },
    ],
    levels: [{ id: "level-1", name: "CEO", level_number: 1 }],
    templates: [
      {
        id: "template-company",
        name: "Standard",
        version: 1,
        positions: [{ key: "ceo", title: "CEO", levelNumber: 1, reportsToKey: null, isCeo: true }],
      },
    ],
    actorId: "user",
    canManage: true,
    canManageUnits: true,
    canManageSpending: true,
  },
};

describe("Native organization transport", () => {
  it("maps the current node-change and setup selectors and leaves unported organization paths to the retained owner", () => {
    expect(nativeOrganizationSelector(["organization", "changes", "submit"])).toEqual({ kind: "save", id: null, submit: true });
    expect(nativeOrganizationSelector(["organization", "changes", "change-1", "draft"])).toEqual({ kind: "save", id: "change-1", submit: false });
    expect(nativeOrganizationSelector(["organization", "changes", "preview"])).toEqual({ kind: "preview" });
    expect(nativeOrganizationSelector(["organization", "changes", "change-1", "review"])).toEqual({ kind: "review", id: "change-1" });
    expect(nativeOrganizationSelector(["organization", "changes", "change-1", "cancel"])).toEqual({ kind: "cancel", id: "change-1" });
    expect(nativeOrganizationSelector(["organization", "levels"])).toEqual({ kind: "levels" });
    expect(nativeOrganizationSelector(["organization", "templates"])).toEqual({ kind: "templateSave", id: null });
    expect(nativeOrganizationSelector(["organization", "templates", "template-1"])).toEqual({ kind: "templateSave", id: "template-1" });
    expect(nativeOrganizationSelector(["organization", "templates", "template-1", "apply"])).toEqual({ kind: "templateApply", id: "template-1" });
    expect(nativeOrganizationSelector(["organization"])).toBeUndefined();
    expect(nativeOrganizationSelector(["organization", "drafts"])).toBeUndefined();
    expect(nativeOrganizationSelector(["organization", "position-moves", "submit"])).toBeUndefined();
  });

  it("rejects reserved, non-loopback and duplicate Holding configuration before any socket", () => {
    expect(() => nativeOrganizationConfiguration("{")).toThrow(NativeOrganizationError);
    expect(() => createNativeOrganizationClient({ ...nativeConfig, privateBaseUrl: "http://127.0.0.1:3100" })).toThrow(NativeOrganizationError);
    expect(() => createNativeOrganizationClient({ ...nativeConfig, privateBaseUrl: "http://192.168.1.1:3199" })).toThrow(NativeOrganizationError);
    expect(() =>
      createNativeOrganizationClient({
        ...nativeConfig,
        holdings: [...nativeConfig.holdings, ...nativeConfig.holdings],
      }),
    ).toThrow(NativeOrganizationError);
  });

  it("validates the exact node/tree/channel workspace and forwards the current session and CAS headers", async () => {
    const transport = nativeTransport(workspacePayload);
    const client = createNativeOrganizationClient(nativeConfig, transport);
    const data = await client.workspace(nativeHeaders, nativeActor, "changeState=open");
    expect(data.nodes[0]).toMatchObject({ id: "branch", display_name: "North", hierarchy_revision: 1 });
    expect(data.channels).toEqual([{ node_id: "branch", room_id: "org-channel-branch", name: "North", kind: "branch", status: "active" }]);
    expect(data.positions[0]).toMatchObject({ id: "position-branch", level_number: 1, is_ceo: 1 });
    expect(data.levels).toEqual([{ id: "level-1", name: "CEO", level_number: 1 }]);
    expect(data.templates[0]).toMatchObject({ id: "template-company", version: 1 });
    expect(data.canManageSpending).toBe(true);
    expect(data.canManageUnits).toBe(true);
    expect(transport).toHaveBeenCalledWith(
      "GET",
      expect.objectContaining({ pathname: "/api/v1/holdings/acme/people/organization" }),
      expect.any(Headers),
      undefined,
      expect.any(AbortSignal),
    );
    const headers = transport.mock.calls[0]![2] as Headers;
    expect(headers.get("cookie")).toBe("session=value");
    expect(headers.get("x-erpx-context-assignment")).toBe("assignment");
    expect(headers.get("x-erpx-context-revision")).toBe("7");
  });

  it("keeps Organization-owned page facts native and only composes the retained People projection", async () => {
    const native = await createNativeOrganizationClient(nativeConfig, nativeTransport(workspacePayload)).workspace(
      nativeHeaders,
      nativeActor,
      "",
    );
    const people = [
      {
        person_id: "person-1",
        user_id: "user-1",
        legal_name: "Person One",
        email: null,
        engagement_id: "engagement-1",
        engagement_version: 1,
        person_version: 1,
        company_id: "company",
        branch_id: "branch",
        department_id: null,
        position_id: "position-branch",
        manager_person_id: null,
        level_id: "level-1",
        level_number: 1,
        level_name: "CEO",
        status: "active",
        person_status: "active",
        start_date: "2026-01-01",
        end_date: null,
        job_title: "CEO",
        issues: [],
      },
    ];
    expect(
      nativeOrganizationOverview(native, {
        people,
        defaults: { pay: [], leave: [] },
        monthlyPayGroups: [{ id: "pay-group", name: "Monthly", currency: "THB" }],
        spendingTemplate: undefined,
      }),
    ).toMatchObject({
      positions: [expect.objectContaining({ id: "position-branch" })],
      people,
      nodes: [expect.objectContaining({ id: "branch" })],
      levels: [{ id: "level-1", name: "CEO", level_number: 1 }],
      templates: [expect.objectContaining({ id: "template-company" })],
      nodeChanges: [],
      changeQueue: { state: "open" },
      authorizedChannelNodes: ["branch"],
      channels: [expect.objectContaining({ room_id: "org-channel-branch" })],
      actorId: "user",
      canManage: true,
      canManageUnits: true,
      canManageSpending: true,
      defaults: { pay: [], leave: [] },
      monthlyPayGroups: [{ id: "pay-group", name: "Monthly", currency: "THB" }],
    });
  });

  it("accepts a retained command receipt and maps a stale native conflict to the route error contract", async () => {
    const command = createNativeOrganizationClient(
      nativeConfig,
      nativeTransport({ data: { id: "org-change-key", version: 2, state: "submitted" } }),
    );
    expect(await command.change(nativeHeaders, nativeActor, { path: "/changes", body: { action: "create" }, idempotencyKey: "organization-key-0001" })).toEqual({
      id: "org-change-key",
      version: 2,
      state: "submitted",
    });
    const stale = createNativeOrganizationClient(nativeConfig, nativeTransport(null, 409));
    await expect(
      stale.change(nativeHeaders, nativeActor, { path: "/changes/change-1/review", body: {}, idempotencyKey: "organization-key-0002" }),
    ).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_UNAVAILABLE", status: 409 });
    expect(nativeOrganizationFailure(new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE", 409))).toMatchObject({ status: 409, code: "CONFLICT" });
    expect(nativeOrganizationFailure(new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE", 401))).toMatchObject({ status: 401 });
    expect(nativeOrganizationFailure(new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE", 422))).toMatchObject({ status: 422, code: "NATIVE_ORGANIZATION_REQUEST_INVALID" });
    expect(nativeOrganizationFailure(new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE"))).toMatchObject({ status: 503 });
  });

  it("fails closed on malformed or over-broad native workspace replies", async () => {
    const malformed = createNativeOrganizationClient(nativeConfig, nativeTransport({ data: { nodes: [], nodeChanges: [] } }));
    await expect(malformed.workspace(nativeHeaders, nativeActor, "")).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_RESPONSE_INVALID" });
    const badPosition = createNativeOrganizationClient(
      nativeConfig,
      nativeTransport({ data: { ...workspacePayload.data, positions: [{ id: "position" }] } }),
    );
    await expect(badPosition.workspace(nativeHeaders, nativeActor, "")).rejects.toMatchObject({
      code: "NATIVE_ORGANIZATION_RESPONSE_INVALID",
    });
    const badSpendingFlag = createNativeOrganizationClient(
      nativeConfig,
      nativeTransport({ data: { ...workspacePayload.data, canManageSpending: "yes" } }),
    );
    await expect(badSpendingFlag.workspace(nativeHeaders, nativeActor, "")).rejects.toMatchObject({
      code: "NATIVE_ORGANIZATION_RESPONSE_INVALID",
    });
    const wrongHost = createNativeOrganizationClient(nativeConfig, nativeTransport(workspacePayload));
    await expect(
      wrongHost.workspace({ get: () => "other.example.test" }, nativeActor, ""),
    ).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_AUTHORITY_INVALID" });
    expect(
      nativeOrganizationFailure(new NativeOrganizationError("NATIVE_ORGANIZATION_RESPONSE_INVALID")),
    ).toMatchObject({ status: 502, code: "NATIVE_ORGANIZATION_RESPONSE_INVALID" });
  });
});

const pagePeople: OrganizationAccessData["people"] = [
  {
    person_id: "person-1",
    user_id: "user-1",
    legal_name: "Person One",
    email: "private@example.test",
    engagement_id: "engagement-1",
    engagement_version: 1,
    person_version: 1,
    company_id: "company",
    branch_id: "branch",
    department_id: null,
    position_id: "position-branch",
    manager_person_id: null,
    level_id: "level-1",
    level_number: 1,
    level_name: "CEO",
    status: "active",
    person_status: "active",
    start_date: "2026-01-01",
    end_date: null,
    job_title: "North lead",
    issues: [],
  },
];

const retainedAccess = (): OrganizationAccessData => ({
  positions: [
    {
      id: "retained-position",
      company_id: "company",
      branch_id: "branch",
      department_id: null,
      title: "Retained position",
      level_id: "level-retained",
      level_number: 9,
      reports_to_position_id: null,
      requirements: "",
      is_ceo: 0,
      roster_required: 0,
      active: 1,
      version: 1,
    },
  ],
  people: pagePeople,
  nodes: [
    {
      id: "retained-company",
      kind: "company",
      slug: "retained",
      display_name: "Retained company",
      branch_id: null,
      status: "active",
      hierarchy_revision: 1,
    },
  ],
  levels: [{ id: "level-retained", name: "Retained level", level_number: 9 }],
  canManage: true,
  canMovePeople: true,
  actorId: nativeActor.userId,
});

async function nativeWorkspace(payload: unknown = workspacePayload) {
  return createNativeOrganizationClient(nativeConfig, nativeTransport(payload)).workspace(
    nativeHeaders,
    nativeActor,
    "",
  );
}

describe("Native organization page read", () => {
  it("uses the native tree, positions and levels while retaining only People rows and the move ceiling", async () => {
    const native = await nativeWorkspace();
    const retained = retainedAccess();
    const page = await nativeOrganizationPage(
      nativeActor,
      "acme",
      {},
      nativeHeaders,
      retained,
      async () => native,
    );
    expect(page?.access).toMatchObject({
      people: retained.people,
      canManage: true,
      canMovePeople: true,
      actorId: "user",
    });
    expect(page?.access.people).toBe(retained.people);
    expect(page?.access.nodes).toEqual([
      {
        id: "branch",
        kind: "branch",
        slug: "north",
        display_name: "North",
        branch_id: "branch",
        status: "active",
        hierarchy_revision: 1,
      },
    ]);
    expect(page?.access.positions.map((position) => position.id)).toEqual(["position-branch"]);
    expect(page?.access.levels).toEqual([{ id: "level-1", name: "CEO", level_number: 1 }]);
    expect(page?.overview).toMatchObject({
      nodes: [expect.objectContaining({ id: "branch" })],
      positions: [expect.objectContaining({ id: "position-branch" })],
      levels: [{ id: "level-1", name: "CEO", level_number: 1 }],
      channels: [expect.objectContaining({ room_id: "org-channel-branch" })],
      canManage: true,
    });
    expect(page?.overview.people).toBe(retained.people);
  });

  it("keeps retained detail defaults while replacing every Organization-owned overview fact", async () => {
    const native = await nativeWorkspace();
    const retained = retainedAccess();
    const page = await nativeOrganizationPage(
      nativeActor,
      "acme",
      {},
      nativeHeaders,
      retained,
      async () => native,
    );
    const retainedDetail: OrganizationOverview = {
      defaults: { pay: [], leave: [] },
      monthlyPayGroups: [{ id: "pay-group", name: "Monthly", currency: "THB" }],
      positions: retained.positions,
      people: retained.people,
      nodes: retained.nodes,
      levels: retained.levels,
      templates: [],
      nodeChanges: [],
      changeQueue: { recordId: null, state: "open", query: "", page: 1, pageSize: 20, total: 0, pages: 1 },
      canManageUnits: false,
      actorId: "user",
      canManage: true,
    };
    const detail = nativeOrganizationDetail(page!.overview, retainedDetail, page!.access.canManage);
    expect(detail.defaults).toEqual({ pay: [], leave: [] });
    expect(detail.monthlyPayGroups).toEqual([{ id: "pay-group", name: "Monthly", currency: "THB" }]);
    expect(detail.nodes).toEqual([expect.objectContaining({ id: "branch" })]);
    expect(detail.positions).toEqual([expect.objectContaining({ id: "position-branch" })]);
    expect(detail.people).toBe(retained.people);
    expect(detail.canManage).toBe(true);
    expect(nativeOrganizationDetail(page!.overview, retainedDetail, false).canManage).toBe(false);
  });

  it("filters inactive and out-of-scope native positions without filtering authoritative People rows", async () => {
    const native = await nativeWorkspace();
    const position = native.positions[0]!;
    const page = await nativeOrganizationPage(
      { ...nativeActor, branchId: "branch" },
      "acme",
      {},
      nativeHeaders,
      retainedAccess(),
      async () => ({
        ...native,
        positions: [
          position,
          { ...position, id: "inactive", active: 0 },
          { ...position, id: "other-branch", branch_id: "branch-2" },
          { ...position, id: "company-wide", branch_id: null },
          { ...position, id: "department-position", department_id: "department-1" },
        ],
      }),
    );
    expect(page?.access.positions.map((item) => item.id)).toEqual([
      "position-branch",
      "department-position",
    ]);
    expect(page?.overview.positions.map((item) => item.id)).toEqual([
      "position-branch",
      "department-position",
    ]);
    expect(page?.overview.positions.some((item) => item.id === "other-branch")).toBe(false);
    expect(page?.access.people).toBe(pagePeople);
    const departmentPage = await nativeOrganizationPage(
      { ...nativeActor, branchId: "branch", departmentId: "department-1" },
      "acme",
      {},
      nativeHeaders,
      retainedAccess(),
      async () => ({
        ...native,
        positions: [position, { ...position, id: "department-position", department_id: "department-1" }],
      }),
    );
    expect(departmentPage?.access.positions.map((item) => item.id)).toEqual(["department-position"]);
  });

  it("never raises the retained Company-manage ceiling from native data", async () => {
    const native = await nativeWorkspace();
    const page = await nativeOrganizationPage(
      nativeActor,
      "acme",
      {},
      nativeHeaders,
      { ...retainedAccess(), canManage: false, canMovePeople: false },
      async () => native,
    );
    expect(page?.access.canManage).toBe(false);
    expect(page?.access.canMovePeople).toBe(false);
    expect(page?.overview.canManage).toBe(false);
  });

  it("returns undefined, without composing, when the native transport is not configured", async () => {
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => undefined),
    ).resolves.toBeUndefined();
  });

  it("propagates a configured native failure instead of returning retained data", async () => {
    const failure = new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE");
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => {
        throw failure;
      }),
    ).rejects.toBe(failure);
  });

  it("rejects a native actor, Company or duplicate identity mismatch before composing the page", async () => {
    const native = await nativeWorkspace();
    const rejected = { code: "NATIVE_ORGANIZATION_RESPONSE_INVALID" };
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => ({
        ...native,
        actorId: "other-user",
      })),
    ).rejects.toMatchObject(rejected);
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => ({
        ...native,
        nodes: native.nodes.map((node) => ({ ...node, company_id: "other-company" })),
      })),
    ).rejects.toMatchObject(rejected);
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => ({
        ...native,
        positions: native.positions.map((position) => ({ ...position, company_id: "other-company" })),
      })),
    ).rejects.toMatchObject(rejected);
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess(), async () => ({
        ...native,
        levels: [...native.levels, ...native.levels],
      })),
    ).rejects.toMatchObject(rejected);
  });
});

describe("Native organization page configuration selection", () => {
  const configured = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  afterEach(() => {
    if (configured === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
  });

  it("returns undefined before any transport work when configuration is absent", async () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    await expect(
      nativeOrganizationWorkspace(nativeActor, "acme", {}, nativeHeaders),
    ).resolves.toBeUndefined();
    await expect(
      nativeOrganizationPage(nativeActor, "acme", {}, nativeHeaders, retainedAccess()),
    ).resolves.toBeUndefined();
  });

  it("fails closed before any transport work when configured configuration is malformed", async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = "{";
    await expect(
      nativeOrganizationWorkspace(nativeActor, "acme", {}, nativeHeaders),
    ).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_CONFIGURATION_INVALID" });
  });

  it("rejects a changed Holding before any transport work", async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify(nativeConfig);
    await expect(
      nativeOrganizationWorkspace(nativeActor, "other", {}, nativeHeaders),
    ).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_CONTEXT_CHANGED" });
  });
});

describe("Native organization access and position-move transport", () => {
  const accessPayload = {
    data: {
      positions: [
        {
          id: "position-lead",
          company_id: "company",
          branch_id: null,
          department_id: null,
          title: "North lead",
          level_id: "level-1",
          level_number: 1,
          reports_to_position_id: null,
          requirements: "",
          is_ceo: 1,
          roster_required: 0,
          active: 1,
          version: 1,
        },
      ],
      people: [
        {
          person_id: "person-lead",
          user_id: "user",
          legal_name: "North Lead",
          email: null,
          engagement_id: "engagement-lead",
          engagement_version: 1,
          person_version: 1,
          company_id: "company",
          branch_id: null,
          department_id: null,
          position_id: "position-lead",
          manager_person_id: null,
          level_id: "level-1",
          level_number: 1,
          level_name: "Level 1",
          status: "active",
          person_status: "active",
          start_date: "",
          end_date: null,
          job_title: "North lead",
          issues: [],
        },
      ],
      nodes: [
        {
          id: "company",
          kind: "company",
          slug: "acme",
          display_name: "Acme",
          branch_id: null,
          status: "active",
          hierarchy_revision: 1,
        },
      ],
      levels: [{ id: "level-1", name: "Level 1", level_number: 1 }],
      canManage: true,
      canMovePeople: true,
      actorId: "user",
    },
  };
  const movePreview = {
    previewToken: "preview-token",
    expiresAt: 2_000,
    ready: true,
    blockers: [],
    before: { branchId: null, departmentId: null, levelId: "level-1", reportsToPositionId: null },
    after: { branchId: null, departmentId: null, levelId: "level-2", reportsToPositionId: null },
    people: [],
    policyChanges: { added: [], removed: [], retained: [] },
    affectedCount: 0,
    effectiveAt: 1_000,
    timezone: "UTC",
  };
  const moveRecord = {
    id: "position-move-key",
    company_id: "company",
    position_id: "position-lead",
    status: "scheduled",
    version: 1,
    effective_at: 1_000,
    definition: {
      positionId: "position-lead",
      expectedVersion: 1,
      targetKind: "level",
      targetId: "level-2",
      effectiveAt: 1_000,
      timezone: "UTC",
      managerAssignments: [],
      reason: "Reviewed position move",
    },
    preview: movePreview,
    created_by: "user",
    created_at: 1,
    updated_at: 1,
    applied_at: null,
    last_error: null,
  };

  function capture(payload: unknown, status = 200) {
    const calls: { method: string; url: URL; headers: Headers; body: string | undefined }[] = [];
    const transport = vi.fn(
      async (method: string, url: URL, headers: Headers, body: string | undefined, signal: AbortSignal) => {
        void signal;
        calls.push({ method, url, headers, body });
        return new Response(JSON.stringify(payload), {
          status,
          headers: { "content-type": "application/json; charset=utf-8" },
        });
      },
    );
    return { calls, transport };
  }

  it("reads the exact native access projection with forwarded context headers", async () => {
    const { calls, transport } = capture(accessPayload);
    const data = await createNativeOrganizationClient(nativeConfig, transport).access(
      nativeHeaders,
      nativeActor,
    );
    expect(data).toEqual(accessPayload.data);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.method).toBe("GET");
    expect(calls[0]!.url.toString()).toBe("http://127.0.0.1:3199/api/v1/holdings/acme/people/access");
    expect(calls[0]!.headers.get("host")).toBe("acme.example.test");
    expect(calls[0]!.headers.get("cookie")).toBe("session=value");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("company");
    expect(calls[0]!.headers.get("x-erpx-context-assignment")).toBe("assignment");
    expect(calls[0]!.headers.get("x-erpx-context-revision")).toBe("7");
    expect(calls[0]!.headers.get("x-erpx-context-assignment-version")).toBe("3");
  });

  it("fails closed on a malformed or over-broad access reply", async () => {
    const malformed = createNativeOrganizationClient(
      nativeConfig,
      capture({ data: { ...accessPayload.data, canManage: "yes" } }).transport,
    );
    await expect(malformed.access(nativeHeaders, nativeActor)).rejects.toMatchObject({
      code: "NATIVE_ORGANIZATION_RESPONSE_INVALID",
    });
    const extra = createNativeOrganizationClient(
      nativeConfig,
      capture({ data: { ...accessPayload.data, payroll: [] } }).transport,
    );
    await expect(extra.access(nativeHeaders, nativeActor)).rejects.toMatchObject({
      code: "NATIVE_ORGANIZATION_RESPONSE_INVALID",
    });
  });

  it("lists moves and returns one exact record for an id selector", async () => {
    const list = capture({ data: [moveRecord] });
    const client = createNativeOrganizationClient(nativeConfig, list.transport);
    await expect(client.positionMoves(nativeHeaders, nativeActor)).resolves.toEqual([moveRecord]);
    expect(list.calls[0]!.url.toString()).toBe(
      "http://127.0.0.1:3199/api/v1/holdings/acme/people/organization/position-moves",
    );
    const one = capture({ data: [moveRecord] });
    const record = await createNativeOrganizationClient(nativeConfig, one.transport).positionMoves(
      nativeHeaders,
      nativeActor,
      "position-move-key",
    );
    expect(record).toEqual(moveRecord);
    expect(one.calls[0]!.url.toString()).toBe(
      "http://127.0.0.1:3199/api/v1/holdings/acme/people/organization/position-moves/position-move-key",
    );
  });

  it("previews without an idempotency key and submits and cancels through exact routes", async () => {
    const preview = capture({ data: movePreview });
    await expect(
      createNativeOrganizationClient(nativeConfig, preview.transport).previewPositionMove(
        nativeHeaders,
        nativeActor,
        moveRecord.definition,
      ),
    ).resolves.toEqual(movePreview);
    expect(preview.calls[0]!.method).toBe("POST");
    expect(preview.calls[0]!.url.toString()).toBe(
      "http://127.0.0.1:3199/api/v1/holdings/acme/people/organization/position-moves/preview",
    );
    expect(preview.calls[0]!.headers.get("idempotency-key")).toBeNull();
    expect(preview.calls[0]!.headers.get("content-type")).toBe("application/json");

    const submitted = capture({ data: { id: "position-move-key", version: 1, status: "scheduled", effectiveAt: 1_000 } });
    await expect(
      createNativeOrganizationClient(nativeConfig, submitted.transport).submitPositionMove(
        nativeHeaders,
        nativeActor,
        {
          body: { ...moveRecord.definition, previewToken: "preview-token", expiresAt: 2_000 },
          idempotencyKey: "organization-key-0001",
          origin: "https://acme.example.test",
        },
      ),
    ).resolves.toEqual({ id: "position-move-key", version: 1, status: "scheduled", effectiveAt: 1_000 });
    expect(submitted.calls[0]!.url.toString()).toBe(
      "http://127.0.0.1:3199/api/v1/holdings/acme/people/organization/position-moves/submit",
    );
    expect(submitted.calls[0]!.headers.get("idempotency-key")).toBe("organization-key-0001");
    expect(submitted.calls[0]!.headers.get("origin")).toBe("https://acme.example.test");
    expect(JSON.parse(submitted.calls[0]!.body!)).toMatchObject({ previewToken: "preview-token", expiresAt: 2_000 });

    const cancelled = capture({ data: { id: "position-move-key", version: 2, status: "cancelled" } });
    await expect(
      createNativeOrganizationClient(nativeConfig, cancelled.transport).cancelPositionMove(
        nativeHeaders,
        nativeActor,
        { id: "position-move-key", expectedVersion: 1, idempotencyKey: "organization-key-0002" },
      ),
    ).resolves.toEqual({ id: "position-move-key", version: 2, status: "cancelled" });
    expect(cancelled.calls[0]!.url.toString()).toBe(
      "http://127.0.0.1:3199/api/v1/holdings/acme/people/organization/position-moves/position-move-key/cancel",
    );
    expect(JSON.parse(cancelled.calls[0]!.body!)).toEqual({ expectedVersion: 1 });
  });

  it("rejects a malformed move list and an unsafe cancellation before transport", async () => {
    const malformed = createNativeOrganizationClient(
      nativeConfig,
      capture({ data: [{ ...moveRecord, status: "unknown" }] }).transport,
    );
    await expect(malformed.positionMoves(nativeHeaders, nativeActor)).rejects.toMatchObject({
      code: "NATIVE_ORGANIZATION_RESPONSE_INVALID",
    });
    const transport = capture({ data: [] }).transport;
    await expect(
      createNativeOrganizationClient(nativeConfig, transport).cancelPositionMove(nativeHeaders, nativeActor, {
        id: "position-move-key",
        expectedVersion: 0,
        idempotencyKey: "organization-key-0003",
      }),
    ).rejects.toMatchObject({ code: "NATIVE_ORGANIZATION_REQUEST_INVALID" });
    expect(transport).not.toHaveBeenCalled();
  });

  it("selects only the exact People catch-all position-move routes", () => {
    expect(nativePositionMoveSelector(["organization", "position-moves"])).toEqual({ kind: "list" });
    expect(nativePositionMoveSelector(["organization", "position-moves", "position-move-key"])).toEqual({
      kind: "list",
      id: "position-move-key",
    });
    expect(nativePositionMoveSelector(["organization", "position-moves", "preview"])).toEqual({ kind: "preview" });
    expect(nativePositionMoveSelector(["organization", "position-moves", "submit"])).toEqual({ kind: "submit" });
    expect(
      nativePositionMoveSelector(["organization", "position-moves", "position-move-key", "cancel"]),
    ).toEqual({ kind: "cancel", id: "position-move-key" });
    expect(nativePositionMoveSelector(["organization", "changes", "submit"])).toBeUndefined();
    expect(nativePositionMoveSelector(["organization", "position-moves", "position-move-key", "other"])).toBeUndefined();
  });

  it("keeps retained access when unconfigured and surfaces a configured failure", async () => {
    const configured = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      const retained = vi.fn(async () => retainedAccess());
      await expect(
        nativeOrganizationAccessPage(nativeActor, "acme", nativeHeaders, retained),
      ).resolves.toEqual(retainedAccess());
      expect(retained).toHaveBeenCalledOnce();
      const failure = new NativeOrganizationError("NATIVE_ORGANIZATION_UNAVAILABLE");
      const never = vi.fn(async () => retainedAccess());
      await expect(
        nativeOrganizationAccessPage(nativeActor, "acme", nativeHeaders, never, async () => {
          throw failure;
        }),
      ).rejects.toBe(failure);
      expect(never).not.toHaveBeenCalled();
    } finally {
      if (configured === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    }
  });

  it("keeps the retained read for an actor outside the native people.manage gate", async () => {
    const configured = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    try {
      process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify(nativeConfig);
      const retained = vi.fn(async () => retainedAccess());
      await expect(
        nativeOrganizationAccessPage(nativeActor, "acme", nativeHeaders, retained),
      ).resolves.toEqual(retainedAccess());
      expect(retained).toHaveBeenCalledOnce();
    } finally {
      if (configured === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    }
  });

  it("returns undefined from the move wrappers when the native transport is not configured", async () => {
    const configured = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    try {
      delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      await expect(
        nativeOrganizationPositionMoveList(nativeActor, "acme", undefined, nativeHeaders),
      ).resolves.toBeUndefined();
      await expect(
        nativeOrganizationPositionMoveCommand(
          nativeActor,
          "acme",
          ["organization", "position-moves", "preview"],
          moveRecord.definition,
          "organization-key-0004",
          nativeHeaders,
        ),
      ).resolves.toBeUndefined();
      await expect(
        nativeOrganizationPositionMoveCommand(
          nativeActor,
          "acme",
          ["organization", "changes", "submit"],
          {},
          "organization-key-0005",
          nativeHeaders,
        ),
      ).resolves.toBeUndefined();
    } finally {
      if (configured === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = configured;
    }
  });
});
