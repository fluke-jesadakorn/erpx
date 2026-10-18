import { expect, it } from "vitest";
import { effectiveWorkModules } from "@/server/modules/identity_access/domain/work-modules";
import { effectiveAuthorityPermissions } from "@/server/modules/identity_access/application/authority";
import { legacyModuleCompatibility } from "@/server/modules/identity_access/domain/legacy-module-compatibility";
it("takes custom-role modules from the published policy instead of the assignment's old module list", () => {
  expect(effectiveWorkModules("custom_finance", ["finance","people_payroll","pos"], ["module.finance.read"], false)).toEqual(["finance"]);
});
it("preserves the current HR appointment policy and its module-level effect", () => {
  const permissions = ["people.read","people.manage","module.people_payroll.read","iam.read"];
  const without = effectiveAuthorityPermissions("hr_admin", permissions, false);
  expect(effectiveWorkModules("hr_admin", ["people_payroll","collaboration"], without, false)).toEqual(["collaboration"]);
  const withAppointment = effectiveAuthorityPermissions("finance_preparer", ["finance.read"], true);
  expect(effectiveWorkModules("finance_preparer", ["finance"], withAppointment, false)).toEqual(["finance","people_payroll"]);
});
it("preserves Beta module restrictions and its explicit People authority exception", () => {
  expect(effectiveWorkModules("holding_owner", ["finance","pos"], [], true)).toEqual([]);
  expect(effectiveWorkModules("holding_owner", ["finance","pos"], ["module.people_payroll.read"], true)).toEqual(["people_payroll"]);
});
it("requires published module and actual owner permissions before reviewing a historical Commerce group", () => {
  const permissions = ["commerce.read", "commerce.sales", "commerce.purchase", "inventory.read", "module.sales.read", "module.procurement.read", "module.inventory.read", "iam.read"];
  const original = ["commerce", "iam"];
  expect(legacyModuleCompatibility({ role: "holding_owner", permissions, assignmentScope: "holding" }, original)).toEqual([
    { label: "iam", modules: ["identity_access"], source: "published-permission-policy/v1" },
    { label: "commerce", modules: ["sales", "procurement", "inventory"], source: "published-permission-policy/v1" },
  ]);
  expect(original).toEqual(["commerce", "iam"]);
  expect(legacyModuleCompatibility({ role: "holding_owner", permissions: ["commerce.read"] }, ["commerce"])).toEqual([]);
  expect(legacyModuleCompatibility({ role: "holding_owner", permissions }, [])).toEqual([]);
  expect(legacyModuleCompatibility({ role: "holding_owner", permissions: [], assignmentScope: "holding" }, original)).toEqual([]);
});
it("does not use a Department's scope-ineligible Stock permission as compatibility evidence", () => {
  const result = legacyModuleCompatibility({ role: "custom_narrow", permissions: ["commerce.read", "inventory.read", "module.inventory.read"], assignmentScope: "department" }, ["commerce"]);
  expect(result).toEqual([]);
});
