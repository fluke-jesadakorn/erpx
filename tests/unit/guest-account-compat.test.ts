import { expect, it } from "vitest";
import { canonicalGuestAccount, legacyGuestAccount, guestAccountOperation, guestAccountUrl, parseGuestAccountJson } from "../../src/lib/guest-account-compat";
import { builtInPermissions, hasPermission } from "../../src/lib/permissions";
import type { PermissionActor } from "../../src/lib/iam";

it("reads historical references without changing IDs, money or the frozen payload",()=>{
  const current={kind:"guest_account",operation:"guest_account.close",input:{guestAccountId:"historical-account-id",toGuestAccountId:"other-account",amount:10700},guestAccount:{due:10700,deposit:3000},guestAccounts:["historical-account-id"]};
  const bytes=JSON.stringify(legacyGuestAccount(current));
  expect(parseGuestAccountJson(bytes)).toEqual(current);
  expect(JSON.stringify(legacyGuestAccount(current))).toBe(bytes);
  expect(guestAccountOperation(legacyGuestAccount({operation:current.operation}).operation)).toBe(current.operation);
  const oldKey=Object.keys(legacyGuestAccount({guestAccountId:"old"}))[0];
  expect(()=>canonicalGuestAccount({[oldKey]:"old",guestAccountId:"different"})).toThrow("GUEST_ACCOUNT_REFERENCE_CONFLICT");
});
it("redirects historical URLs with the original selectors and canonical response names",()=>{
  const section=legacyGuestAccount({section:"guest-accounts"}).section;
  const key=Object.keys(legacyGuestAccount({guestAccount:"account-id"}))[0];
  expect(guestAccountUrl(`/holdings/acme/pms/${section}?property=hotel&${key}=account-id`)).toBe("/holdings/acme/pms/guest-accounts?property=hotel&guestAccount=account-id");
  expect(guestAccountUrl(`/acme/acme-co/holding/${section}?${key}=account-id`,true)).toBe("/acme/acme-co/holding/guest-accounts?guestAccount=account-id");
});
it("separates built-in operational and accounting assignments and preserves approved custom grants",()=>{
  for(const role of ["finance_preparer","finance_approver","treasury","auditor"]){
    const permissions=builtInPermissions(role);
    for(const forbidden of ["pms.read","pms.cashier","pms.approve","pos.read","pos.configure","pos.approve"])expect(permissions).not.toContain(forbidden);
  }
  expect(builtInPermissions("pos_manager")).toContain("pos.configure");
  expect(builtInPermissions("pos_manager")).not.toContain("pos.accounting.prepare");
  for(const role of ["pos_cashier","pos_server"])expect(builtInPermissions(role)).not.toContain("pos.kitchen");
  const actor={userId:"one-person",role:"custom",permissions:["pms.cashier","pos.configure"],assignmentScope:"holding"} as PermissionActor;
  expect(hasPermission(actor,"pms.cashier")).toBe(true); expect(hasPermission(actor,"pos.configure")).toBe(true);
  expect(hasPermission({...actor,permissions:["pos.configure"]},"pms.cashier")).toBe(false);
  expect(hasPermission({...actor,assignmentScope:"branch",permissions:["pos.accounting.prepare"]},"pos.accounting.prepare")).toBe(false);
});

it("replays a historical receipt after the migration changes context revision, with current authority still required",async()=>{
  const {createCommandApplication}=await import("../../src/server/modules/workflow/application/commands");
  const actor={userId:"operator",assignmentId:"hotel-cashier",role:"pms_cashier",companyId:"company",branchId:null,departmentId:null,authority:{sessionId:"session",revision:2,assignmentVersion:1}};
  const previous={...actor,authority:{...actor.authority,revision:1}},input={guestAccountId:"original-id"},operation="pms.guest_account.close";
  const historical=legacyGuestAccount({operation,input,result:{guestAccountId:"original-id",version:9}});
  const request_hash=JSON.stringify({actor:actor.userId,assignment:actor.assignmentId,role:actor.role,authority:previous.authority,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation:historical.operation,recordId:"original-id",input:historical.input});
  let current=true,writes=0;
  const ports={repository:{receipt:async()=>({request_hash,result_json:JSON.stringify(historical.result),operation:historical.operation,record_id:"original-id",actor_id:actor.userId,authority_json:JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,...previous.authority,companyId:actor.companyId,branchId:null,departmentId:null})}),clearAssertions:()=>"cleanup",insertReceipt:()=>{writes++;return "receipt";},captureAssignmentAuthority:()=>"capture",audit:()=>"audit",outbox:()=>"event"},clock:{now:()=>100},identifiers:{next:()=>"authority-check"},digest:async(value:unknown)=>JSON.stringify(value),workContextGuards:()=>["context"],iamMutationGuards:()=>["grants"],assignmentGuard:()=>"assignment",prepareOwnerStatements:async()=>[],batch:async()=>{if(!current)throw Object.assign(new Error("expired"),{code:"23514"});},activeContext:async()=>null,activeGrant:async()=>null};
  const app=createCommandApplication(ports);
  expect(await app.replayCommand(actor,"historical-key",operation,"original-id",input)).toEqual({guestAccountId:"original-id",version:9});
  expect(writes).toBe(0);
  await expect(app.replayCommand({...actor,companyId:"other-company"},"historical-key",operation,"original-id",input)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  await expect(app.replayCommand(actor,"historical-key",operation,"original-id",{guestAccountId:"different-id"})).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  current=false;
  await expect(app.replayCommand(actor,"historical-key",operation,"original-id",input)).rejects.toMatchObject({code:"WORK_CONTEXT_CHANGED"});
});
