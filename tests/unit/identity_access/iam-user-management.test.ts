import {describe,expect,it,vi} from 'vitest';
import {createUserManagementApplication,type UserManagementStore,type UserManagementPorts,type ManagementObject,type ManagementPolicyMatch} from "@erpx/server-identity_access/application/user-management";
import {createDirectoryFieldMappingApplication,type DirectoryFieldFacts,type DirectoryFieldMappingPorts} from "@erpx/server-identity_access/application/directory-field-mapping";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {attributeInput,groupInput,policyInput,type AttributeDefinition,type LocalGroup} from "@erpx/feature-identity_access/lib/user-management";
import type {IamActor} from "@erpx/server-identity_access/domain/iam-actor";
import type {DirectoryFieldState,DirectoryFieldAttribute,FieldSyncInput} from "@erpx/server-identity_access/domain/directory-field-mapping";
const actor:IamActor={userId:'author',assignmentId:'parent',companyId:'company',sessionId:'session',holdingId:'holding',scopeKind:'holding',scopeNodeId:null,branchId:null,departmentId:null,platform:false,role:'holding_owner',iam:{permissions:['iam.read','iam.manage','iam.roles.manage','people.custom_attributes.write','people.manage'],grantablePermissions:['module.people_payroll.read'],roleRevision:1,identityVersion:1,principalId:"author",personId:null,freshUntil:null}};
const group=(id='group',parentId:string|null=null):ManagementObject<LocalGroup>=>({id,version:1,definition:{...groupInput.parse({name:'Manual group',kind:'manual',parentId,expectedVersion:0,reason:'Group setup'}),id,version:1,members:[]}});
const attribute=(extra:Partial<AttributeDefinition>={}):ManagementObject<AttributeDefinition>=>({id:'field',version:1,definition:{...attributeInput.parse({label:'Employee note',target:'person',type:'text',expectedVersion:0,reason:'Employee setup'}),id:'field',version:1,...extra}});
function managementFixture(){
 const store:UserManagementStore={activeScopeNode:vi.fn(async()=>1),descendantScope:vi.fn(async()=>({ok:true})),person:vi.fn(async()=>({id:'person',legal_name:'Employee',user_id:'employee'})),personInScope:vi.fn(async()=>({ok:true})),group:vi.fn(async()=>group()),groups:vi.fn(async()=>[group()]),people:vi.fn(async()=>[{id:'person',name:'Employee',user_id:'employee',definition:{},groups_json:[null,'unit']}]),memberships:vi.fn(async()=>[]),nodes:vi.fn(async()=>[{id:'unit',parent_id:null,display_name:'Unit',kind:'branch'}]),attachments:vi.fn(async()=>[]),policies:vi.fn(async()=>[]),attributes:vi.fn(async()=>[attribute()]),values:vi.fn(async()=>[]),ancestry:vi.fn(async()=>[]),candidates:vi.fn(async()=>[]),lockObjectVersion:vi.fn(async()=>null),replaceOwnedAttachment:vi.fn(async()=>{}),replaceObject:vi.fn(async()=>{}),appendObjectHistory:vi.fn(async()=>{}),parentExpiry:vi.fn(async()=>({valid_until:100000000})),grantScopeNode:vi.fn(async()=>1),publishedGrantRole:vi.fn(async()=>({permissions_json:['module.people_payroll.read'],grantable_json:[],scopes_json:['holding']})),attributeRoleActive:vi.fn(async()=>1),attributeRuleUse:vi.fn(async()=>0),parentGroupScope:vi.fn(async()=>({ok:true})),childGroupScope:vi.fn(async()=>({ok:true})),memberInGroupScope:vi.fn(async()=>({ok:true})),addMember:vi.fn(async()=>{}),removeMember:vi.fn(async()=>{}),lockValues:vi.fn(async()=>null),inheritedValue:vi.fn(async()=>null),replaceValues:vi.fn(async()=>{}),appendValueHistory:vi.fn(async()=>{}),exclusionWithinTarget:vi.fn(async()=>({ok:true})),lockManagement:vi.fn(async()=>{}),rejectRevokedCandidates:vi.fn(async()=>{}),invalidateManagedAccess:vi.fn(async()=>{}),rejectReviewedCandidates:vi.fn(async()=>{}),eligiblePolicyMatches:vi.fn(async()=>[]),policyProposer:vi.fn(async()=>({id:'parent',scope_kind:'holding',scope_node_id:null,role:'holding_owner',grantable_json:['module.people_payroll.read']})),existingCandidate:vi.fn(async()=>0),recordBlockedCandidate:vi.fn(async()=>{}),insertGrantReview:vi.fn(async()=>{}),insertGrantCandidate:vi.fn(async()=>{}),appendGrantProposalAudit:vi.fn(async()=>{}),lockCandidate:vi.fn(async()=>null),candidateEligible:vi.fn(async()=>({ok:true})),activateCandidate:vi.fn(async()=>{}),lockCommand:vi.fn(async()=>{})};
 let id=0;const ports:UserManagementPorts={error:(...args)=>new BusinessRuleError(...args),now:()=>10000,id:()=>`new-${++id}`,hash:value=>JSON.stringify(value),authorize:vi.fn(()=>{}),fresh:vi.fn(async()=>{}),ruleError:(error):error is Error&{code:string}=>error instanceof BusinessRuleError,readTransaction:run=>run(store),previewTransaction:vi.fn(async run=>run(store)),command:vi.fn(async(_actor,_key,_operation,_payload,_permission,run)=>run(store)),workSession:run=>run({companies:async()=>['company'],transaction:(_company,run)=>run(store)}),validateGrant:vi.fn(async()=>{}),insertGrant:vi.fn(async()=> 'assignment')};
 return {store,ports,app:createUserManagementApplication(ports)};
}
const groupChange={operation:'group',input:{name:'New manual group',kind:'manual',parentId:null,scopeNodeId:null,rule:null,expectedVersion:0,archived:false,reason:'Independent group setup'}};
const match:ManagementPolicyMatch={id:'attachment',parent_id:'parent',author_id:'author',version:1,policy_version:1,person_id:'person',user_id:'employee',policy_data:{...policyInput.parse({name:'People reader',grants:[{roleId:'custom_reader',expectedRoleRevision:1,scopeKind:'holding',scopeNodeId:null,validUntil:50000}],expectedVersion:0,reason:'Read employee module'}),id:'policy',version:1}};
describe('User Management application boundary',()=>{
 it('authorizes the Company read before requesting sensitive person facts',async()=>{const f=managementFixture();vi.spyOn(f.ports,'authorize').mockImplementation(()=>{throw new BusinessRuleError(403,'ACCESS_REVOKED','Access revoked');});await expect(f.app.managementSnapshot(f.store,actor)).rejects.toMatchObject({code:'ACCESS_REVOKED'});expect(f.store.people).not.toHaveBeenCalled();});
 it('lets HR write owned person fields without reading IAM identities or granting IAM read',async()=>{
  const f=managementFixture(),hr:IamActor={...actor,role:'hr_admin',iam:{...actor.iam!,permissions:['people.custom_attributes.write']}};
  vi.spyOn(f.ports,'authorize').mockImplementation((current,permission)=>{if(!current.iam?.permissions.includes(permission))throw new BusinessRuleError(403,'PERMISSION_DENIED','Permission denied');});
  vi.spyOn(f.store,'attributes').mockResolvedValue([attribute({ownerRole:'hr_admin',readRoles:['hr_admin'],writeRoles:['hr_admin']})]);
  const change={operation:'values',input:{target:'person',objectId:'person',values:{field:'Private HR note'},expectedVersion:0,reason:'Record HR note'}};
  const preview=await f.app.managementCommand(hr,change,'preview',true);
  expect(preview.impact).toEqual({membersAdded:[],membersRemoved:[],accessRemoved:[],accessPending:[],accessBlocked:[],matchingPeople:[]});
  expect(await f.app.managementCommand(hr,change,'apply',false,preview.previewToken)).toMatchObject({id:'person'});
  expect(f.store.replaceValues).toHaveBeenCalledWith(expect.objectContaining({actor:hr,version:1,values:{field:'Private HR note'}}));
  for(const reader of [f.store.people,f.store.groups,f.store.memberships,f.store.policies,f.store.attachments,f.store.candidates])expect(reader).not.toHaveBeenCalled();
  await expect(f.app.managementSnapshot(f.store,hr)).rejects.toMatchObject({code:'PERMISSION_DENIED'});
  expect(hr.iam!.permissions).toEqual(['people.custom_attributes.write']);
 });
 it('rechecks owned field ACLs between HR preview and apply without writing stale values',async()=>{
  const f=managementFixture(),hr:IamActor={...actor,role:'hr_admin',iam:{...actor.iam!,permissions:['people.custom_attributes.write']}};
  vi.spyOn(f.store,'attributes').mockResolvedValue([attribute({ownerRole:'hr_admin',readRoles:['hr_admin'],writeRoles:['hr_admin']})]);
  const change={operation:'values',input:{target:'person',objectId:'person',values:{field:'Private HR note'},expectedVersion:0,reason:'Record HR note'}};
  const preview=await f.app.managementCommand(hr,change,'preview',true);
  vi.mocked(f.store.replaceValues).mockClear();
  vi.spyOn(f.store,'attributes').mockResolvedValue([attribute({ownerRole:'auditor',readRoles:['auditor'],writeRoles:['auditor']})]);
  await expect(f.app.managementCommand(hr,change,'apply',false,preview.previewToken)).rejects.toThrow('preview changed');
  expect(f.store.replaceValues).not.toHaveBeenCalled();
 });
 it('rejects platform workspaces and removes null engagement nodes',async()=>{const f=managementFixture();await expect(f.app.managementSnapshot(f.store,{...actor,platform:true})).rejects.toMatchObject({code:'SCOPE_FORBIDDEN'});expect(f.store.groups).not.toHaveBeenCalled();expect((await f.app.managementSnapshot(f.store,actor)).people[0].nodes).toEqual(['unit']);});
 it('hides field values and automatic rule conditions that refer to unreadable owned fields',async()=>{const f=managementFixture();vi.spyOn(f.store,'attributes').mockResolvedValue([attribute({readRoles:['hr_admin'],writeRoles:['hr_admin']})]);vi.spyOn(f.store,'values').mockResolvedValue([{company_id:'company',target:'person',object_id:'person',version:1,definition:{field:'Private HR note'}}]);const automatic=group();automatic.definition={...automatic.definition,kind:'automatic',rule:{mode:'all',conditions:[{field:'custom:field',op:'eq',value:'Private HR note'}]}};vi.spyOn(f.store,'groups').mockResolvedValue([automatic]);const view=await f.app.managementSnapshot(f.store,actor);expect(view.values[0].values).toEqual({});expect(view.groups[0].rule).toBeNull();});
 it('rejects a stale commit fingerprint before version writes and reconciliation',async()=>{const f=managementFixture();await expect(f.app.managementCommand(actor,groupChange,'command',false,'stale')).rejects.toThrow('preview changed');expect(f.store.lockCommand).toHaveBeenCalledOnce();expect(f.store.replaceObject).not.toHaveBeenCalled();expect(f.store.invalidateManagedAccess).not.toHaveBeenCalled();});
 it('keeps dry runs on the preview transaction and fixed observed history time',async()=>{const f=managementFixture();const result=await f.app.managementCommand(actor,groupChange,'preview',true);expect(result.id).toBe('new-1');expect(f.ports.previewTransaction).toHaveBeenCalledOnce();expect(f.ports.command).not.toHaveBeenCalled();expect(f.store.appendObjectHistory).toHaveBeenCalledWith(expect.objectContaining({table:'group',observedNow:10000}));expect(f.store.invalidateManagedAccess).toHaveBeenCalledOnce();});
 it('rejects cycles before saving and stale object revisions after a fresh preview',async()=>{const f=managementFixture();vi.spyOn(f.store,'group').mockResolvedValue(group('parent','parent'));await expect(f.app.managementCommand(actor,{...groupChange,input:{...groupChange.input,parentId:'parent'}},'preview',true)).rejects.toThrow('form a cycle');expect(f.store.replaceObject).not.toHaveBeenCalled();vi.spyOn(f.store,'lockObjectVersion').mockResolvedValue({version:2});await expect(f.app.managementCommand(actor,groupChange,'preview',true)).rejects.toThrow('Refresh and preview');});
 it('blocks manual membership writes when the scope or group version changed',async()=>{const f=managementFixture();await expect(f.app.managementCommand(actor,{operation:'membership',input:{groupId:'group',personIds:['person'],action:'add',expectedVersion:2,reason:'Group assignment'}},'preview',true)).rejects.toThrow('group changed');expect(f.store.addMember).not.toHaveBeenCalled();vi.spyOn(f.store,'personInScope').mockResolvedValue({ok:false});await expect(f.app.managementCommand({...actor,scopeKind:'branch',scopeNodeId:'unit'},{operation:'values',input:{target:'person',objectId:'person',values:{field:'Note'},expectedVersion:0,reason:'Employee note'}},'preview',true)).rejects.toMatchObject({code:'SCOPE_FORBIDDEN'});expect(f.store.replaceValues).not.toHaveBeenCalled();});
 it('rejects a field ACL expansion whose editors cannot view the definition',async()=>{const f=managementFixture();await expect(f.app.managementCommand(actor,{operation:'attribute',input:{label:'Private note',target:'person',type:'text',readRoles:['hr_admin'],writeRoles:['it_admin'],expectedVersion:0,reason:'Field ownership'}},'preview',true)).rejects.toMatchObject({code:'ATTRIBUTE_ACL_INVALID'});expect(f.store.replaceObject).not.toHaveBeenCalled();});
 it('requires fresh MFA only for committed policy changes',async()=>{const f=managementFixture(),policy={operation:'policy',input:{...match.policy_data as object,id:undefined}};await f.app.managementCommand(actor,policy,'preview',true);expect(f.ports.fresh).not.toHaveBeenCalled();vi.spyOn(f.ports,'fresh').mockRejectedValue(new BusinessRuleError(403,'MFA_REQUIRED','Recent MFA required'));await expect(f.app.managementCommand(actor,policy,'save',false,'token')).rejects.toMatchObject({code:'MFA_REQUIRED'});expect(f.ports.command).not.toHaveBeenCalled();});
 it('keeps blocked grant proposals as facts and cannot insert an assignment during reconciliation',async()=>{const f=managementFixture();vi.spyOn(f.store,'eligiblePolicyMatches').mockResolvedValue([match]);vi.spyOn(f.ports,'validateGrant').mockRejectedValue(new BusinessRuleError(403,'DELEGATION_EXCEEDED','Delegation changed'));await f.app.reconcileManagement(f.store,'company');expect(f.store.recordBlockedCandidate).toHaveBeenCalledWith(expect.objectContaining({blocked:'Delegation changed'}));expect(f.store.insertGrantReview).not.toHaveBeenCalled();expect(f.ports.insertGrant).not.toHaveBeenCalled();});
 it('proposes independently reviewed access through the supplied transaction and never renews expired policy grants',async()=>{const f=managementFixture();vi.spyOn(f.store,'eligiblePolicyMatches').mockResolvedValue([match]);await f.app.reconcileManagement(f.store,'company');expect(f.ports.validateGrant).toHaveBeenCalledWith(f.store,expect.objectContaining({assignmentId:'parent'}),expect.objectContaining({validUntil:50000}),'parent');expect(f.store.insertGrantReview).toHaveBeenCalledOnce();expect(f.ports.insertGrant).not.toHaveBeenCalled();vi.spyOn(f.ports,'now').mockReturnValue(50000);await f.app.reconcileManagement(f.store,'company');expect(f.store.insertGrantReview).toHaveBeenCalledOnce();});
 it('rechecks beneficiary eligibility before approval and activation',async()=>{const f=managementFixture();await expect(f.app.applyManagementReview(f.store,actor,{candidateId:'candidate',parentId:'parent'})).rejects.toThrow('Eligibility changed');expect(f.ports.insertGrant).not.toHaveBeenCalled();expect(f.store.activateCandidate).not.toHaveBeenCalled();});
});
function mappingFixture(){const state:DirectoryFieldState={version:1,connection_version:2,tested_version:2,inventory:{objects:[{id:'account',kind:'account',dn:'uid=employee,dc=fixture',version:'1',module:'users',name:'Employee',parentId:null,properties:{description:'Directory note'},memberIds:[]}],fields:[{name:'description',type:'text',read:true,write:true}],providerVersion:'fixture',observedAt:1,complete:true},definition:{mappings:[{direction:'read',field:'description',attributeId:'field'}]}};const field:DirectoryFieldAttribute=attribute();const facts:DirectoryFieldFacts={state:vi.fn(async()=>state),identities:vi.fn(async()=>[{id:'identity',external_id:'account'}]),person:vi.fn(async()=>({legal_name:'Employee'})),values:vi.fn(async()=>({version:3,definition:{field:'Old note'}})),attribute:vi.fn(async()=>field)};const ports:DirectoryFieldMappingPorts<string>={companyScope:vi.fn(()=>{}),authorize:vi.fn(()=>{}),hash:value=>JSON.stringify(value),readSnapshot:run=>run(facts),authorizeCommand:vi.fn(async(_actor,_key,_input,run)=>run(facts)),previewValues:vi.fn(async()=>({previewToken:'values-token'})),applyValues:vi.fn(async()=> 'applied'),previewDirectory:vi.fn(async()=>({token:'directory-token'})),submitDirectory:vi.fn(async()=> 'submitted')};return {state,field,facts,ports,app:createDirectoryFieldMappingApplication(ports)};}
const mappingInput:FieldSyncInput={connectionId:'connection',personId:'person',direction:'read',reason:'Verified mapped import'};
describe('Directory field mapping application',()=>{
 it('requires the current tested connector schema before reading identities',async()=>{const f=mappingFixture();f.state.tested_version=1;await expect(f.app.mappedFields(actor,mappingInput,'key',true)).rejects.toMatchObject({code:'DIRECTORY_TEST_REQUIRED'});expect(f.facts.identities).not.toHaveBeenCalled();});
 it('requires one explicitly verified linked account and rejects ambiguous identities',async()=>{const f=mappingFixture();vi.spyOn(f.facts,'identities').mockResolvedValue([{id:'a',external_id:'account'},{id:'b',external_id:'account'}]);await expect(f.app.mappedFields(actor,mappingInput,'key',true)).rejects.toMatchObject({code:'VERIFIED_IDENTITY_REQUIRED'});expect(f.facts.person).not.toHaveBeenCalled();});
 it('revalidates owned field ACLs rather than treating mappings as a permission grant',async()=>{const f=mappingFixture();f.field.definition.writeRoles=['hr_admin'];await expect(f.app.mappedFields(actor,mappingInput,'key',true)).rejects.toMatchObject({code:'ATTRIBUTE_WRITE_FORBIDDEN'});expect(f.ports.applyValues).not.toHaveBeenCalled();});
 it('rejects stale mapping previews before authorizing or applying values',async()=>{const f=mappingFixture(),preview=await f.app.mappedFields(actor,mappingInput,'key',true);expect(preview).toMatchObject({changes:[{label:'Employee note',from:'Old note',to:'Directory note'}]});await expect(f.app.mappedFields(actor,{...mappingInput,previewToken:'stale'},'key',false)).rejects.toMatchObject({code:'MAPPING_PREVIEW_CHANGED'});expect(f.ports.authorizeCommand).not.toHaveBeenCalled();expect(f.ports.applyValues).not.toHaveBeenCalled();});
 it('rechecks the authorization snapshot after the values preview before delegated mutation',async()=>{const f=mappingFixture(),prepared=await f.app.plan(f.facts,actor,mappingInput);vi.spyOn(f.ports,'previewValues').mockImplementation(async()=>{f.state.version=2;return {previewToken:'values-token'};});await expect(f.app.mappedFields(actor,{...mappingInput,previewToken:prepared.token},'key',false)).rejects.toMatchObject({code:'MAPPING_PREVIEW_CHANGED'});expect(f.ports.applyValues).not.toHaveBeenCalled();});
 it('delegates a verified import with current values version and the values preview token',async()=>{const f=mappingFixture(),prepared=await f.app.plan(f.facts,actor,mappingInput);expect(await f.app.mappedFields(actor,{...mappingInput,previewToken:prepared.token},'key',false)).toBe('applied');expect(f.ports.authorize).toHaveBeenCalledWith(actor,'people.manage');expect(f.ports.applyValues).toHaveBeenCalledWith(actor,expect.objectContaining({input:expect.objectContaining({expectedVersion:3,values:{field:'Directory note'}})}),'key','values-token');});
 it('keeps export in the independently reviewed directory lane with the native connector version',async()=>{const f=mappingFixture();f.state.definition.mappings=[{direction:'write',field:'description',attributeId:'field'}];const input={...mappingInput,direction:'write' as const},prepared=await f.app.plan(f.facts,actor,input);expect(await f.app.mappedFields(actor,{...input,previewToken:prepared.token},'key',false)).toBe('submitted');expect(f.ports.applyValues).not.toHaveBeenCalled();expect(f.ports.submitDirectory).toHaveBeenCalledWith(actor,expect.objectContaining({expectedVersion:2,action:{kind:'account.update',objectId:'account',fields:{description:'Old note'}}}),'directory-token','key');});
});

// Standard IAM product presentation retains native bodies/keys through original JSON.
import {readFileSync as managementRead} from "node:fs";
import {createRequire as managementRequire} from "node:module";
import {runInNewContext as managementEvaluate} from "node:vm";
import * as managementReact from "react";
import {createManagementWorkspaceCommands, type ManagementPreview} from "@erpx/feature-identity_access/lib/management-workspace-command";
import {managementChange as workspaceChange, type ManagementSnapshot} from "@erpx/feature-identity_access/lib/user-management";
import type {workFetch as managementWorkFetch} from "@erpx/feature-work_projection/lib/work-fetch";
import * as managementContext from "@erpx/feature-work_projection/lib/context-switch";
const managementImpact = {membersAdded: [], membersRemoved: [], accessRemoved: [], accessPending: [], accessBlocked: [], matchingPeople: []};
const managementSnapshot = (): ManagementSnapshot => ({attributes: [], groups: [], policies: [], attachments: [], values: [], people: [], nodes: [], candidates: [], canManage: true, canManageGroups: true, canDefine: true, canManagePeople: true, canManagePersonAttributes: true, canManageOrganization: true, canReview: true});
const managementChange = () => workspaceChange.parse({operation: "group", input: {name: "Native group", kind: "manual", parentId: null, scopeNodeId: null, rule: null, expectedVersion: 0, archived: false, reason: "Reviewed group setup"}});
function managementDeferred<Value>() {let resolve!: (value: Value) => void; const promise = new Promise<Value>(done => {resolve = done;}); return {promise, resolve};}
const managementTick = async () => {for (let index = 0; index < 12; index++) await Promise.resolve();};
function managementBrowser() {
  const node = {dataset: {workUser: "native-author", workCompany: "company", workAssignment: "assignment", workRevision: "revision-1", workStale: "false"}, isConnected: true};
  const events: Event[] = [], target = new EventTarget(), dispatch = target.dispatchEvent.bind(target);
  vi.stubGlobal("window", Object.assign(target, {location: {origin: "https://console.example.test"}, dispatchEvent: (event: Event) => {events.push(event); return dispatch(event);}}));
  vi.stubGlobal("document", {querySelector: (selector: string) => selector === "[data-work-revision]" ? node : null, activeElement: null});
  return {node, events, cleanup: () => vi.unstubAllGlobals()};
}
function managementReply(data: unknown, duringJson?: () => void | Promise<void>, status = 200) {
  const response = Response.json(status === 200 ? {data} : {error: data}, {status});
  const original = response.json.bind(response);
  response.json = async () => {await duringJson?.(); return original();}; return response;
}
function managementCommandsFixture() {
  const browser = managementBrowser(), live = {api: "/api/v1/holdings/native/people/access", source: "snapshot-1", draft: 1}, receipt = {id: "native-created-group", previewToken: "native-preview", impact: managementImpact};
  const send = vi.fn<typeof managementWorkFetch>(async (url) => String(url).endsWith("/management") ? managementReply(managementSnapshot()) : managementReply(receipt));
  const commands = createManagementWorkspaceCommands(() => live, send), preview: ManagementPreview = {change: managementChange(), previewToken: receipt.previewToken, impact: managementImpact, key: "retained-native-key", draft: live.draft};
  return {...browser, live, receipt, send, commands, preview};
}
it("withholds management acknowledgement when Work changes during ORIGINAL save JSON and recovers the exact first body/key under fresh authority", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockResolvedValueOnce(managementReply(f.receipt, () => {f.node.dataset.workRevision = "revision-2";}));
    await expect(f.commands.save(f.preview)).rejects.toMatchObject({code: "WORK_CONTEXT_CHANGED"});
    expect(f.commands.hasPending()).toBe(true); expect(f.events).toHaveLength(0);
    f.live.source = "current-history-after-save";
    expect((await f.commands.retry()).unchanged).toBe(true);
    expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);
    expect(new Headers(f.send.mock.calls[1][1]?.headers).get("Idempotency-Key")).toBe("retained-native-key");
    expect(f.send.mock.calls.every(call => call[3] === "caller")).toBe(true);
    expect(f.events.filter(event => event.type === "erpx:record-command-saved")).toHaveLength(1);
  } finally {f.cleanup();}
});
it("retains management uncertainty across a fresh 403 and blocks replacing the first request with a newer form", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockRejectedValueOnce(Error("Connection lost after commit")); await expect(f.commands.save(f.preview)).rejects.toThrow("Connection lost");
    const capturedGroup = f.preview.change as Extract<ManagementPreview["change"], {operation: "group"}>; // The fixture above constructs this exact native variant.
    await expect(f.commands.save({...f.preview, key: "newer-key", change: {...capturedGroup, input: {...capturedGroup.input, reason: "Newer change"}}})).rejects.toThrow("Recover the original");
    f.send.mockResolvedValueOnce(managementReply({code: "FORBIDDEN", message: "Current role denied"}, undefined, 403));
    await expect(f.commands.retry()).rejects.toThrow("Current role denied"); expect(f.commands.hasPending()).toBe(true);
    await f.commands.retry();
    expect(f.send.mock.calls.slice(0, 3).map(call => new Headers(call[1]?.headers).get("Idempotency-Key"))).toEqual([f.preview.key, f.preview.key, f.preview.key]);
  } finally {f.cleanup();}
});
it("does not acknowledge an invalid ORIGINAL management receipt even when its response clone contains the expected receipt", async () => {
  const f = managementCommandsFixture(); try {
    const response = managementReply({...f.receipt, previewToken: "another-preview"}); response.clone = () => managementReply(f.receipt);
    f.send.mockResolvedValueOnce(response); await expect(f.commands.save(f.preview)).rejects.toThrow("receipt is incomplete");
    expect(f.commands.hasPending()).toBe(true); expect(f.events).toHaveLength(0);
  } finally {f.cleanup();}
});
it("keeps newer management edits and the original submitted form revision through an acknowledged save", async () => {
  const f = managementCommandsFixture(); try {
    const version = {value: "edit-1"}, form = {isConnected: true, getAttribute: () => version.value} as unknown as HTMLFormElement;
    f.preview.completion = {form, formEditVersion: version.value};
    f.send.mockResolvedValueOnce(managementReply(f.receipt, () => {f.live.draft++; version.value = "edit-2";}));
    expect((await f.commands.save(f.preview)).unchanged).toBe(false);
    expect(f.commands.hasPending()).toBe(false);
    const saved = f.events.find(event => event.type === "erpx:record-command-saved") as CustomEvent;
    expect(saved.detail).toEqual({form, formEditVersion: "edit-1"});
  } finally {f.cleanup();}
});
it("fences the ORIGINAL refreshed management snapshot and retries only that read after already verified command receipts", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockResolvedValueOnce(managementReply(f.receipt)).mockResolvedValueOnce(managementReply(managementSnapshot(), () => {f.live.source = "updated-server-source";}));
    await expect(f.commands.save(f.preview)).rejects.toMatchObject({code: "WORK_CONTEXT_CHANGED"}); expect(f.events).toHaveLength(0);
    await f.commands.retry(); expect(f.send.mock.calls.filter(call => call[1]?.method === "POST")).toHaveLength(1);
    expect(f.events.filter(event => event.type === "erpx:record-command-saved")).toHaveLength(1);
  } finally {f.cleanup();}
});
it("refuses another Company or assignment without reissuing an unresolved management request", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockRejectedValueOnce(Error("Unknown outcome")); await expect(f.commands.save(f.preview)).rejects.toThrow("Unknown outcome");
    f.node.dataset.workCompany = "another-company"; await expect(f.commands.retry()).rejects.toMatchObject({code: "WORK_CONTEXT_CHANGED"});
    expect(f.send).toHaveBeenCalledTimes(1); expect(f.commands.hasPending()).toBe(true);
  } finally {f.cleanup();}
});
it("does not preview or announce a saved draft that changed during ORIGINAL preview decoding", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockResolvedValueOnce(managementReply(f.receipt, () => {f.live.draft++;}));
    await expect(f.commands.preview(managementChange())).rejects.toThrow("draft changed");
    expect(f.events).toHaveLength(0); expect(f.commands.hasPending()).toBe(false);
  } finally {f.cleanup();}
});
it("retains exact independent review IDs, versions, reasons and keys across a later unknown batch outcome", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockResolvedValueOnce(managementReply({id: "first-review", status: "approved"})).mockRejectedValueOnce(Error("Second response lost"));
    await expect(f.commands.review([{id: "first-review", version: 4}, {id: "second-review", version: 7}], "approve", f.live.draft)).rejects.toThrow("Second response lost");
    f.send.mockResolvedValueOnce(managementReply({id: "second-review", status: "approved"})); await f.commands.retry();
    expect(f.send.mock.calls[1].slice(0, 2)).toEqual(f.send.mock.calls[2].slice(0, 2));
    expect(JSON.parse(String(f.send.mock.calls[2][1]?.body))).toEqual({expectedVersion: 7, decision: "approve", reason: "User management: approve reviewed access"});
    expect(f.send.mock.calls.filter(call => String(call[0]).endsWith("first-review"))).toHaveLength(1);
  } finally {f.cleanup();}
});

type ManagementPresenterProps = {initial: ManagementSnapshot; roles: []; base: string; api: string; accounts: {meta: object; initialEntries: object}; compactConsole: boolean; initialView: "groups"; views: ["groups"]};
function managementPresenter(send: typeof managementWorkFetch, original = false) {
  const require = managementRequire(import.meta.url), ts = require("typescript") as typeof import("typescript"), states: unknown[] = [], effects = new Map<number, {dependencies?: unknown[]; cleanup?: () => void}>();
  let cursor = 0;
  const useState = (initial: unknown) => {const index = cursor++; if (!(index in states)) states[index] = typeof initial === "function" ? initial() : initial; return [states[index], (value: unknown) => {states[index] = typeof value === "function" ? value(states[index]) : value;}];};
  const useRef = (value: unknown) => {const [ref] = useState(() => ({current: value})); return ref;};
  const effect = (run: () => void | (() => void), dependencies?: unknown[]) => {const index = cursor++, prior = effects.get(index); if (!prior || !dependencies || dependencies.some((value, key) => value !== prior.dependencies?.[key])) {prior?.cleanup?.(); effects.set(index, {dependencies, cleanup: run() || undefined});}};
  const primitives = new Proxy<Record<string, unknown>>({}, {get: (_target, name) => {
    const tag = ({Button: "button", Field: "label", TextInput: "input", Select: "select", TextArea: "textarea", UiActionLink: "a", ActionLink: "a", FormLayout: "form", Panel: "section", InlineNotice: "div", Badge: "span"} as Record<string, string>)[String(name)] ?? "div";
    return Object.assign((props: Record<string, unknown>) => managementReact.createElement(tag, {...props, "data-test-appearance": props.appearance}, props.children as managementReact.ReactNode), {displayName: String(name)});
  }});
  const drawer = (props: {open?: boolean; retainFormContents?: boolean; children?: managementReact.ReactNode}) => props.open || props.retainFormContents ? managementReact.createElement("aside", props, props.children) : null;
  const evaluatedModule = {exports: {} as Record<string, unknown>};
  const sourcePath = original ? "packages/feature-people_payroll/src/components/user-management.tsx" : "packages/feature-identity_access/src/components/user-management-workspace.tsx";
  const source = managementRead(original ? sourcePath : `${process.env.ERPX_MANAGEMENT_SOURCE_ROOT ? process.env.ERPX_MANAGEMENT_SOURCE_ROOT + "/" : ""}${sourcePath}`, "utf8"), code = ts.transpileModule(source, {compilerOptions: {esModuleInterop: false, jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  const localRequire = (name: string) => {
    if (name === "react") return {...managementReact, useState, useRef, useEffect: effect, useLayoutEffect: effect};
    if (name === "react/jsx-runtime") return require(name);
    if (name.endsWith("@erpx/ui/components/foundation")) return primitives;
    if (name.endsWith("@erpx/feature-work_projection/lib/work-fetch")) return {workFetch: send};
    if (name.endsWith("@erpx/feature-identity_access/lib/management-workspace-command")) return {createManagementWorkspaceCommands: (current: Parameters<typeof createManagementWorkspaceCommands>[0]) => createManagementWorkspaceCommands(current, send)};
    if (name.endsWith("@erpx/feature-work_projection/lib/context-switch")) return managementContext;
    if (name.endsWith("@erpx/feature-shared/lib/workspace-route-state")) return {useWorkspaceRouteState: () => ({query: new URLSearchParams(), update: () => {}})};
    if (name.endsWith("/user-management")) return require("@erpx/feature-identity_access/lib/user-management");
    if (name.endsWith("@erpx/ui/components/draft-form-drawer")) return {DraftFormDrawer: drawer};
    if (name.endsWith("@erpx/ui/components/dialog")) return {Dialog: (props: Parameters<typeof drawer>[0]) => props.open ? drawer(props) : null};
    if (name.endsWith("/workspace")) return {WorkspaceCollection: primitives.Box, WorkspaceDetail: (props: Parameters<typeof drawer>[0]) => props.open ? drawer(props) : null};
    if (name.endsWith(".module.css")) return {default: new Proxy({}, {get: (_target, property) => String(property)})};
    if (name.endsWith("@erpx/workplace/components/workspace-link") || name === "next/link") return {default: Object.assign((props: Record<string, unknown>) => managementReact.createElement("a", props, props.children as managementReact.ReactNode), {displayName: "DestinationLink"})};
    if (name === "lucide-react") return new Proxy({}, {get: () => () => null});
    return new Proxy({}, {get: () => primitives.Box});
  };
  managementEvaluate(code, {require: localRequire, exports: evaluatedModule.exports, module: evaluatedModule, crypto, URLSearchParams, WeakRef, FormData, window, document, requestAnimationFrame: (run: () => void) => run()});
  let props: ManagementPresenterProps = {initial: managementSnapshot(), roles: [], base: "/holdings/native", api: "/api/v1/holdings/native/people/access", accounts: {meta: {}, initialEntries: {}}, compactConsole: true, initialView: "groups", views: ["groups"]};
  const presenter = evaluatedModule.exports[original ? "UserManagement" : "UserManagementWorkspace"] as (props: ManagementPresenterProps) => managementReact.ReactNode;
  type Node = managementReact.ReactElement<Record<string, unknown>>;
  const collect = (value: managementReact.ReactNode, result: Node[] = []): Node[] => {managementReact.Children.forEach(value, child => {if (!managementReact.isValidElement<Record<string, unknown>>(child)) return; result.push(child); if (typeof child.type === "function") collect((child.type as (props: Record<string, unknown>) => managementReact.ReactNode)(child.props), result); else collect(child.props.children as managementReact.ReactNode, result);}); return result;};
  return {render: () => {cursor = 0; return collect(presenter(props));}, props: (next: ManagementPresenterProps) => {props = next;}, getProps: () => props, close: () => {effects.forEach(item => item.cleanup?.());}};
}
it("mounts the actual standard IAM collection from its SSR snapshot without a mount GET and opts every control into standard appearance", () => {
  const browser = managementBrowser(), send = vi.fn<typeof managementWorkFetch>(); const component = managementPresenter(send);
  try {
    const nodes = component.render(); expect(send).not.toHaveBeenCalled();
    const controls = nodes.filter(node => typeof node.type === "function" && ["Button", "Field", "TextInput", "Select", "TextArea", "ActionLink"].includes((node.type as {displayName?: string}).displayName ?? ""));
    expect(controls.length).toBeGreaterThan(2); expect(controls.every(node => node.props.appearance === "standard")).toBe(true);
    const create = nodes.find(node => node.props.children && String(node.props.children).includes("New ") && typeof node.props.onClick === "function");
    expect(create).toBeDefined(); (create!.props.onClick as () => void)();
    const editor = component.render().find(node => typeof node.type === "function" && node.type.name === "ManagementEditor"); expect(editor).toBeDefined();
    const drawerNode = component.render().find(node => node.type === "aside" && node.props.retainFormContents === true);
    expect(drawerNode?.props.appearance).toBe("standard"); expect(drawerNode?.props.open).toBe(true);
  } finally {component.close(); browser.cleanup();}
});
it("keeps a refreshed owner from clearing an uncertain original management save", async () => {
  const browser = managementBrowser(), held = managementDeferred<void>(), saved = {id: "native-group", previewToken: "native-preview", impact: managementImpact};
  const send = vi.fn<typeof managementWorkFetch>(async (url) => String(url).endsWith("/management") ? managementReply(managementSnapshot()) : managementReply(saved, String(url).endsWith("/save") ? () => held.promise : undefined));
  const original = process.env.ERPX_MANAGEMENT_BASELINE === "1";
  const values = new Map([ ["name", "Native group"], ["kind", "manual"], ["reason", "Reviewed native group"] ]);
  vi.stubGlobal("FormData", class {get(name: string) {return values.get(name) ?? null;} getAll(name: string) {return values.has(name) ? [values.get(name)!] : [];}});
  const component = managementPresenter(send, original);
  try {
    let nodes = component.render(); const create = nodes.find(node => typeof node.props.onClick === "function" && String(node.props.children).includes("New "))!;
    (create.props.onClick as () => void)(); nodes = component.render();
    const form = {isConnected: true, getAttribute: () => "first-form", setAttribute: () => {}} as unknown as HTMLFormElement;
    const editor = nodes.find(node => node.type === "form")!; (editor.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: form}); await managementTick();
    nodes = component.render(); const confirm = nodes.find(node => typeof node.props.onClick === "function" && node.props.children === "Confirm changes")!;
    (confirm.props.onClick as () => void)(); await managementTick();
    component.props({...component.getProps(), initial: {...managementSnapshot(), canManageGroups: false}}); component.render();
    held.resolve(); await managementTick(); nodes = component.render();
    expect(nodes.some(node => typeof node.type === "function" && node.type.name === "ManagementEditor")).toBe(true);
    expect(nodes.some(node => node.props.children === "Recover original request")).toBe(true);
    expect(managementContext.hasUnsavedWork()).toBe(true);
  } finally {component.close(); browser.cleanup();}
});
it("uses refreshed SSR management permissions without a mount fetch or restoring an old editable collection", () => {
  const browser = managementBrowser(), send = vi.fn<typeof managementWorkFetch>(), component = managementPresenter(send);
  try {
    expect(component.render().some(node => typeof node.props.onClick === "function" && String(node.props.children).includes("New "))).toBe(true);
    component.props({...component.getProps(), initial: {...managementSnapshot(), canManage: false, canManageGroups: false, canDefine: false}});
    expect(component.render().some(node => typeof node.props.onClick === "function" && String(node.props.children).includes("New "))).toBe(false);
    expect(send).not.toHaveBeenCalled();
  } finally {component.close(); browser.cleanup();}
});

it("keeps browsing filters out of unsaved work while a real management form edit registers a draft", () => {
  const browser = managementBrowser(), send = vi.fn<typeof managementWorkFetch>(), component = managementPresenter(send);
  try {
    let nodes = component.render();
    const workspace = nodes.find(node => node.type === "div" && node.props["data-ui-appearance"] === "standard" && typeof node.props.onChangeCapture === "function")!;
    const browsing = {target: {closest: () => null}};
    (workspace.props.onChangeCapture as (event: unknown) => void)(browsing);
    expect(managementContext.hasUnsavedWork()).toBe(false);
    const search = nodes.find(node => node.type === "input" && node.props["aria-label"] === "Search groups")!;
    (search.props.onChange as (event: unknown) => void)({target: {value: "Filtered group"}});
    (workspace.props.onChangeCapture as (event: unknown) => void)(browsing);
    expect(managementContext.hasUnsavedWork()).toBe(false);
    nodes = component.render();
    const create = nodes.find(node => typeof node.props.onClick === "function" && String(node.props.children).includes("New "))!;
    (create.props.onClick as () => void)(); nodes = component.render();
    const form = nodes.find(node => node.type === "form" && "data-iam-management-draft" in node.props)!;
    expect(form).toBeDefined();
    const root = nodes.find(node => node.type === "div" && node.props["data-ui-appearance"] === "standard" && typeof node.props.onChangeCapture === "function")!;
    (root.props.onChangeCapture as (event: unknown) => void)({target: {closest: (selector: string) => selector === "form[data-iam-management-draft]" ? form : null}});
    expect(managementContext.hasUnsavedWork()).toBe(true);
    const drawer = component.render().find(node => node.type === "aside" && node.props.retainFormContents === true)!;
    expect(drawer.props.dirty).toBe(true);
  } finally {component.close(); browser.cleanup();}
});
it("retains the partial native review cursor after a later definitive 403 and recovers only that original failed review", async () => {
  const f = managementCommandsFixture(); try {
    f.send.mockResolvedValueOnce(managementReply({id: "first-review", status: "approved"})).mockResolvedValueOnce(managementReply({code: "FORBIDDEN", message: "Current reviewer unavailable"}, undefined, 403));
    await expect(f.commands.review([{id: "first-review", version: 4}, {id: "second-review", version: 7}], "approve", f.live.draft)).rejects.toThrow("Current reviewer unavailable");
    expect(f.commands.hasPending()).toBe(true); expect(f.events).toHaveLength(0);
    f.live.source = "current-review-history";
    f.send.mockResolvedValueOnce(managementReply({id: "second-review", status: "approved"}));
    expect((await f.commands.retry()).data).toEqual(managementSnapshot());
    expect(f.send.mock.calls[1].slice(0, 2)).toEqual(f.send.mock.calls[2].slice(0, 2));
    expect(JSON.parse(String(f.send.mock.calls[2][1]?.body))).toEqual({expectedVersion: 7, decision: "approve", reason: "User management: approve reviewed access"});
    expect(f.send.mock.calls.filter(call => String(call[0]).endsWith("first-review"))).toHaveLength(1);
    expect(f.commands.hasPending()).toBe(false);
    expect(f.events.filter(event => event.type === "erpx:record-command-saved")).toHaveLength(1);
  } finally {f.cleanup();}
});
