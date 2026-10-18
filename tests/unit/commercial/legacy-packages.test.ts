import { expect, it } from "vitest";
import { moduleRegistry } from "@/data/module-registry";
import { legacyModulePackages, legacyPackagePreview } from "@/server/modules/commercial/domain/legacy-packages";
import type { LegacyCompanyModuleAccess } from "@/server/modules/identity_access/public/rules/legacy-module-access";
import type { CompanyPackageGrant } from "@/server/modules/commercial/domain/packages";
const facts = (modules: string[], beta = false): LegacyCompanyModuleAccess => ({ companyId: "c1", observedAt: 1000, revision: "authority-1", beta, witnesses: [{ assignmentId: "a1", assignmentVersion: 2, roleId: "custom_fixture", roleRevision: 3, scopeKind: "department", scopeNodeId: "d1", modules, validUntil: 5000, identityVersion: 1, hrAppointmentVersion: null, compatibility: [] }] });
const options = { startsAt: 1000, now: 1000, reason: "Preserve the witnessed Company scope", evidence: "baseline:fixture" };
it("classifies all 33 original modules once without using blanket grants", () => {
  expect(Object.keys(legacyModulePackages).sort()).toEqual(moduleRegistry.map(module => module.id).sort());
  const plan = legacyPackagePreview(facts(["finance"]), [], options);
  expect(plan.requiredPackages).toEqual(["finance","finance-advanced","work","work-messages","work-social"]);
  expect(plan.roles[0]).toMatchObject({ modulesBefore: ["finance"], modulesAfter: ["finance"], scopeNodeId: "d1", assignmentVersion: 2, roleRevision: 3, missingPurchasedCoverage: [] });
  expect(plan.authorityChanges).toEqual([]); expect(plan.rolloutChanges).toEqual([]); expect(plan.pricingChanges).toEqual([]);
});
it("includes Projects/Learning reference foundations without buying People", () => {
  const plan = legacyPackagePreview(facts(["projects","learning"]), [], options);
  expect(plan.requiredPackages).toEqual(["learn","projects","projects-costs","work","work-messages","work-social"]);
  expect(plan.requiredPackages).not.toContain("people");
});
it("retains redundant old assignment labels without treating them as extra Modules or expanding the role", () => {
  const canonical = ["identity_access", "people_payroll", "sales", "procurement", "inventory"];
  const input = facts([...canonical, "iam", "people", "commerce"]);
  const plan = legacyPackagePreview(input, [], options);
  expect(plan.legacyModules).toEqual(canonical.sort());
  expect(plan.redundantLegacyLabels).toEqual(["commerce", "iam", "people"]);
  expect(plan.roles[0].modulesBefore).toEqual(input.witnesses[0].modules);
  expect(plan.roles[0].modulesAfter).toEqual(input.witnesses[0].modules);
  expect(plan.roles[0].missingPurchasedCoverage).toEqual([]);
  expect(() => legacyPackagePreview(facts(["commerce", "sales"]), [], options)).toThrow("UNMAPPED_LEGACY_MODULE");
});
it("uses independently witnessed Company coverage for old group labels without changing either role's authority", () => {
  const original = facts(["commerce"]), canonical = facts(["sales", "procurement", "inventory"]);
  const input = { ...original, witnesses: [original.witnesses[0], { ...canonical.witnesses[0], assignmentId: "a2" }] };
  const result = legacyPackagePreview(input, [], options), withoutLabel = legacyPackagePreview(canonical, [], options);
  expect(result.requiredPackages).toEqual(withoutLabel.requiredPackages);
  expect(result.roles[0].modulesAfter).toEqual(["commerce"]);
  expect(result.roles[0].purchaseCoverageModules).toEqual(["inventory", "procurement", "sales"]);
  expect(result.roles[0].missingPurchasedCoverage).toEqual([]);
  expect(result.authorityChanges).toEqual([]);
});
it("reviews a historical group from current published owner evidence without granting unrelated packages or changing Module IDs", () => {
  const input = facts(["commerce", "iam"]);
  input.witnesses[0].compatibility = [
    { label: "commerce", modules: ["sales", "procurement", "inventory"], source: "published-permission-policy/v1" },
    { label: "iam", modules: ["identity_access"], source: "published-permission-policy/v1" },
  ];
  const result = legacyPackagePreview(input, [], options);
  expect(result.requiredPackages).toEqual(["sales", "supply", "supply-advanced", "work", "work-messages", "work-social"]);
  expect(result.resolvedLegacyLabels).toEqual(["commerce", "iam"]);
  expect(result.redundantLegacyLabels).toEqual([]);
  expect(result.roles[0].modulesBefore).toEqual(["commerce", "iam"]);
  expect(result.roles[0].modulesAfter).toEqual(["commerce", "iam"]);
  expect(result.roles[0].compatibilityEvidence).toEqual(input.witnesses[0].compatibility);
  expect(result.roles[0].missingPurchasedCoverage).toEqual([]);
  expect(result.authorityChanges).toEqual([]);
  input.witnesses[0].compatibility[0].modules = ["sales", "legal"];
  expect(() => legacyPackagePreview(input, [], options)).toThrow("UNMAPPED_LEGACY_MODULE:commerce");
});
it("preserves the Beta workflow ceiling instead of licensing its raw historical module list", () => {
  const plan = legacyPackagePreview(facts(["finance","pos","people_payroll"], true), [], options);
  expect(plan.requiredPackages).toEqual(["people","people-expenses","work","work-messages","work-social"]);
  expect(plan.requiredPackages).not.toContain("people-payroll"); expect(plan.requiredPackages).not.toContain("finance");
});
it("does not reactivate, extend or overwrite an existing reviewed grant", () => {
  const current: CompanyPackageGrant = { companyId: "c1", packageId: "finance", packageVersion: 1, state: "draining", version: 7, startsAt: 0, endsAt: 2000, reason: "Operator decision", evidence: "audit:original" };
  const plan = legacyPackagePreview(facts(["finance"]), [current], options);
  expect(plan.blockers).toContainEqual({ packageId: "finance", code: "EXISTING_GRANT_REQUIRES_REVIEW" });
  expect(plan.after.find(grant => grant.packageId === "finance")).toEqual(current);
  expect(plan.changes.some(change => change.grant.packageId === "finance")).toBe(false);
});
it("keeps a new add-on within the already purchased base period", () => {
  const current: CompanyPackageGrant = { companyId: "c1", packageId: "finance", packageVersion: 1, state: "active", version: 7, startsAt: 0, endsAt: 2000, reason: "Reviewed finite term", evidence: "audit:original" };
  const plan = legacyPackagePreview(facts(["finance"]), [current], options);
  expect(plan.blockers).toEqual([]);
  expect(plan.changes.find(change => change.grant.packageId === "finance-advanced")?.grant.endsAt).toBe(2000);
  expect(plan.after.find(grant => grant.packageId === "finance")).toEqual(current);
});
it("rejects stale or unmapped facts instead of silently losing a legacy module", () => {
  expect(() => legacyPackagePreview(facts(["unknown"]), [], options)).toThrow("UNMAPPED_LEGACY_MODULE");
  expect(() => legacyPackagePreview(facts(["pos"]), [], { ...options, now: 31001 })).toThrow("STALE_LEGACY_PREVIEW");
  expect(legacyPackagePreview({ ...facts([]), witnesses: [] }, [], options).requiredPackages).toEqual([]);
});

import {workforceSelfServiceEntry} from '@/server/modules/people_payroll/public/rules/self-service-product-entry';
import {recordSections,defaultRecordSection} from '@/lib/records/record-route-definitions';
it('classifies only the original personal Leave and Overtime Work pages without changing their People owner or granting a management prefix',()=>{
 expect(workforceSelfServiceEntry('leave')).toEqual({kind:'leave',view:'list'});expect(workforceSelfServiceEntry('leave/new')).toEqual({kind:'leave',view:'new'});expect(workforceSelfServiceEntry('overtime')).toEqual({kind:'overtime',view:'list'});expect(workforceSelfServiceEntry('overtime/new')).toEqual({kind:'overtime',view:'new'});
 for(const suffix of ['', '/overview']){expect(workforceSelfServiceEntry('tasks/people_payroll/native-leave'+suffix)).toEqual({kind:'leave-record',id:'native-leave'});expect(workforceSelfServiceEntry('overtime/native-overtime'+suffix)).toEqual({kind:'overtime-record',id:'native-overtime'});}
 expect(recordSections.leave??[defaultRecordSection('leave')]).toEqual(['overview']);expect(recordSections.overtime??[defaultRecordSection('overtime')]).toEqual(['overview']);
 for(const path of ['tasks','tasks/people_payroll','tasks/finance/private','leave/admin','leave/foreign','leave/new/overview','overtime/private/history','overtime/private/approve','tasks/people_payroll/private/files','tasks/people_payroll/new','tasks/people_payroll/..','tasks/people_payroll/private?foreign=1','overtime/private#secret','/leave','https://foreign.test/leave'])expect(workforceSelfServiceEntry(path)).toBeNull();
});

// The normal owner parity case proves source-free Home updates and account
// conversations predate a collaboration Module; purchase coverage preserves both.
it('preserves original account Home Social and conversations with their normal Work dependency and finite term',()=>{
 const current:CompanyPackageGrant={companyId:'c1',packageId:'work',packageVersion:1,state:'active',version:7,startsAt:0,endsAt:2000,reason:'Reviewed original finite Work period',evidence:'audit:existing-work'};
 const plan=legacyPackagePreview(facts([]),[current],options);expect(plan.requiredPackages).toEqual(['work','work-messages','work-social']);expect(plan.roles[0]).toMatchObject({modulesBefore:[],modulesAfter:[],missingPurchasedCoverage:[]});expect(plan.changes).toHaveLength(2);expect(plan.changes[0]).toMatchObject({expectedVersion:0,grant:{packageId:'work-messages',endsAt:2000}});expect(plan.changes[1]).toMatchObject({expectedVersion:0,grant:{packageId:'work-social',endsAt:2000}});expect(plan.after.find(grant=>grant.packageId==='work')).toEqual(current);expect(plan.authorityChanges).toEqual([]);expect(plan.rolloutChanges).toEqual([]);expect(plan.requiredPackages).not.toContain('people');expect(plan.requiredPackages).toContain('work-social');expect(plan.requiredPackages).not.toContain('work-routines');
 expect(legacyPackagePreview({...facts([]),witnesses:[]},[],options).requiredPackages).toEqual([]);
});

it('preserves only owner-witnessed non-Beta standalone routine access without licensing no-staff accounts or private orphan templates',()=>{
 for(const access of [{activeStaff:true,retainedTemplates:0},{activeStaff:false,retainedTemplates:1}]){
  const input=facts([]);input.witnesses[0].routineAccess={source:'operations/original-routines/v1',revision:'opaque-original-owner-witness',...access};
  const plan=legacyPackagePreview(input,[],options);expect(plan.requiredPackages).toEqual(['work','work-messages','work-routines','work-social']);expect(plan.roles[0]).toMatchObject({modulesBefore:[],modulesAfter:[],missingPurchasedCoverage:[]});expect(plan.authorityChanges).toEqual([]);expect(plan.rolloutChanges).toEqual([]);expect(plan.requiredPackages).not.toContain('projects');expect(plan.requiredPackages).not.toContain('people');expect(plan.requiredPackages).toContain('work-social');
  expect(legacyPackagePreview({...input,beta:true},[],options).requiredPackages).not.toContain('work-routines');
 }
 const input=facts([]);input.witnesses[0].routineAccess={source:'operations/original-routines/v1',activeStaff:false,retainedTemplates:0,revision:'opaque-no-readable-source'};expect(legacyPackagePreview(input,[],options).requiredPackages).toEqual(['work','work-messages','work-social']);
 const base:CompanyPackageGrant={companyId:'c1',packageId:'work',packageVersion:1,state:'active',version:3,startsAt:0,endsAt:2000,reason:'Original finite term',evidence:'audit:existing'};input.witnesses[0].routineAccess.activeStaff=true;expect(legacyPackagePreview(input,[base],options).changes.find(change=>change.grant.packageId==='work-routines')?.grant.endsAt).toBe(2000);
});
