import { expect, it } from "vitest";
import { builtInPermissions } from "@erpx/identity-access-ui/lib/permissions";
it("separates the default HR and IT permission sets", () => {
  expect(builtInPermissions("hr_admin").filter(p=>p.startsWith("iam.") && p!=="iam.self")).toEqual([]);
  for(const role of ["hr_admin","it_admin","holding_owner"])expect(builtInPermissions(role)).toEqual(expect.arrayContaining(["people.relationships.read","people.relationships.manage"]));
  expect(builtInPermissions("it_admin")).toEqual(expect.arrayContaining(["iam.read","iam.manage","iam.roles.manage","iam.access.apply"]));
  expect(builtInPermissions("it_admin")).not.toContain("people.documents.read");
  expect(builtInPermissions("it_admin")).not.toContain("people.compensation.read");
  expect(builtInPermissions("it_admin")).not.toContain("iam.attributes.manage");
});
it("checks each new form submission while accepting every capture path of its verified portal replay",async()=>{
 const {verifiedFormSubmissions}=await import("@erpx/identity-access-ui/lib/verified-form-submit");
 const guard=verifiedFormSubmissions(),form={},original={},replayed={};
 expect(guard.accepts(form,original)).toBe(false);
 guard.replay(form,()=>{expect(guard.accepts(form,replayed)).toBe(true);expect(guard.accepts(form,replayed)).toBe(true);});
 expect(guard.accepts(form,{})).toBe(false);
 const otherForm={};expect(guard.accepts(otherForm,{})).toBe(false);
});
it("does not leave approval for a later submit when native form validation cancels a replay",async()=>{
 const {verifiedFormSubmissions}=await import("@erpx/identity-access-ui/lib/verified-form-submit");
 const guard=verifiedFormSubmissions(),form={};guard.replay(form,()=>{});
 expect(guard.accepts(form,{})).toBe(false);
 expect(()=>guard.replay(form,()=>{throw Error("invalid submitter");})).toThrow("invalid submitter");
 expect(guard.accepts(form,{})).toBe(false);
});
