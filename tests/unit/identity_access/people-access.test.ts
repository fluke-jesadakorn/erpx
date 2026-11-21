import { expect, it } from "vitest";
import { builtInPermissions } from "@erpx/feature-identity_access/lib/permissions";
import { requireAccessDeadline, requireDelegatedRole, requireExternalReady } from "@erpx/server-identity_access/domain/people-access";
import type { AccessSubject } from "@erpx/feature-identity_access/lib/people-access";
const external: AccessSubject = { id:"external",kind:"external",name:"Vendor",email:"vendor@example.invalid",userId:null,personId:null,departmentId:null,organizationId:"vendor",teamId:null,sponsorPersonId:"employee",expiresAt:2000,status:"active",version:1 };
it("separates the default HR and IT permission sets", () => {
  expect(builtInPermissions("hr_admin").filter(p=>p.startsWith("iam.") && p!=="iam.self")).toEqual([]);
  for(const role of ["hr_admin","it_admin","holding_owner"])expect(builtInPermissions(role)).toEqual(expect.arrayContaining(["people.relationships.read","people.relationships.manage"]));
  expect(builtInPermissions("it_admin")).toEqual(expect.arrayContaining(["iam.read","iam.manage","iam.roles.manage","iam.access.apply"]));
  expect(builtInPermissions("it_admin")).not.toContain("people.documents.read");
  expect(builtInPermissions("it_admin")).not.toContain("people.compensation.read");
  expect(builtInPermissions("it_admin")).not.toContain("iam.attributes.manage");
});
it("requires an organization, employee sponsor and future external expiry", () => {
  expect(()=>requireExternalReady(external,1000)).not.toThrow();
  for(const field of ["organizationId","sponsorPersonId","expiresAt"] as const) expect(()=>requireExternalReady({...external,[field]:null},1000)).toThrow();
  expect(()=>requireExternalReady({...external,status:"suspended"},1000)).toThrow();
  expect(()=>requireExternalReady(external,2000)).toThrow();
});
it("bounds a grant by both the relationship and the authorizing assignment", () => {
  const grant={operation:"grant" as const,subjectId:external.id,roleId:"role",scopeKind:"department" as const,scopeNodeId:"department",validUntil:1500,reason:"Reviewed preview"};
  expect(()=>requireAccessDeadline(grant,external,1600,1000)).not.toThrow();
  expect(()=>requireAccessDeadline({...grant,validUntil:1700},external,1600,1000)).toThrow();
  expect(()=>requireAccessDeadline({...grant,validUntil:2100},external,3000,1000)).toThrow();
});
it("cannot publish ownership, recovery or Platform access through IT", () => {
  for(const permission of ["company.owner","iam.recovery","platform.iam.manage"]) expect(()=>requireDelegatedRole([permission],[permission])).toThrow();
  expect(()=>requireDelegatedRole(["finance.read"],["iam.self"])).toThrow();
});

it("checks each new form submission while accepting every capture path of its verified portal replay",async()=>{
 const {verifiedFormSubmissions}=await import("@erpx/feature-identity_access/lib/verified-form-submit");
 const guard=verifiedFormSubmissions(),form={},original={},replayed={};
 expect(guard.accepts(form,original)).toBe(false);
 guard.replay(form,()=>{expect(guard.accepts(form,replayed)).toBe(true);expect(guard.accepts(form,replayed)).toBe(true);});
 expect(guard.accepts(form,{})).toBe(false);
 const otherForm={};expect(guard.accepts(otherForm,{})).toBe(false);
});
it("does not leave approval for a later submit when native form validation cancels a replay",async()=>{
 const {verifiedFormSubmissions}=await import("@erpx/feature-identity_access/lib/verified-form-submit");
 const guard=verifiedFormSubmissions(),form={};guard.replay(form,()=>{});
 expect(guard.accepts(form,{})).toBe(false);
 expect(()=>guard.replay(form,()=>{throw Error("invalid submitter");})).toThrow("invalid submitter");
 expect(guard.accepts(form,{})).toBe(false);
});
