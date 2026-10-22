import { expect, it } from "vitest";
import { builtInPermissions } from "@erpx/feature-identity_access/lib/permissions";
import { requireAccessDeadline, requireDelegatedRole, requireExternalReady } from "@erpx/server-identity_access/domain/people-access";
import type { AccessSubject } from "@erpx/feature-identity_access/lib/people-access";
const external: AccessSubject = { id:"external",kind:"external",name:"Vendor",email:"vendor@example.invalid",userId:null,personId:null,departmentId:null,organizationId:"vendor",teamId:null,sponsorPersonId:"employee",expiresAt:2000,status:"active",version:1 };
it("separates the default HR and IT permission sets", () => {
  expect(builtInPermissions("hr_admin").filter(p=>p.startsWith("iam.") && p!=="iam.self")).toEqual([]);
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
