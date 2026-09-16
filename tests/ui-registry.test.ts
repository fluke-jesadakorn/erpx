import { describe, it, expect } from "vitest";
import {
  moduleRegistry,
  moduleAvailability,
  capabilitySupport,
  liveDestination,
} from "../src/data/module-registry";
import { queueFor, type WorkRow } from "../src/lib/work-queue";
import { modulesForRole } from "../src/lib/business-roles";
import { hasModuleAccess } from "../src/data/module-access";

describe("Enterprise UI coverage and routing", () => {
  it("matches module access by exact assignment module id", () => {
    const allowed = ["people_payroll", "finance"];
    expect(hasModuleAccess(allowed, "people_payroll")).toBe(true);
    expect(hasModuleAccess(allowed, "people")).toBe(false);
    expect(hasModuleAccess(allowed, "Finance")).toBe(false);
  });
  it("limits legacy broad module grants to the selected business role",()=>{
    expect(modulesForRole("people_manager",["finance","people_payroll","sales"])).toEqual(["people_payroll"]);
    expect(modulesForRole("finance_approver",["finance","people_payroll"])).toEqual(["finance"]);
    expect(modulesForRole("sales_operator",["sales"])).toEqual(["sales"]);
    expect(modulesForRole("holding_owner",["people_payroll","finance"])).toEqual(["people_payroll","finance"]);
  });
  it("covers all module capabilities without ambiguous section routes", () => {
    expect(moduleRegistry).toHaveLength(31);
    expect(
      moduleRegistry
        .flatMap((module) => module.sections)
        .filter((section) => section.capability),
    ).toHaveLength(171);
    expect(
      new Set(
        moduleRegistry.flatMap((module) =>
          module.sections.map((section) => section.kind),
        ),
      ).size,
    ).toBe(9);
    for (const definition of moduleRegistry) {
      expect(
        new Set(definition.sections.map((section) => section.id)).size,
      ).toBe(definition.sections.length);
      for (const primary of definition.primary)
        expect(
          definition.sections.some((section) => section.id === primary),
        ).toBe(true);
    }
  });
  it("does not advertise unwired foundation modules as live", () => {
    for (const id of [
      "organization",
      "platform_administration",
    ])
      expect(
        moduleAvailability(
          moduleRegistry.find((definition) => definition.id === id)!,
        ),
      ).toBe("preview");
    for (const id of ["identity_access", "finance", "people_payroll", "sales", "inventory", "procurement"]) {
      expect(moduleAvailability(moduleRegistry.find(definition => definition.id === id)!)).toBe("partial");
    }
    expect(liveDestination("crm", "leads")).toBeUndefined();
    expect(liveDestination("inventory", "reservations")).toBeUndefined();
  });
  it("retains action-level capability metadata for IAM and backend compatibility", () => {
    for (const definition of moduleRegistry) {
      for (const section of definition.sections.filter(
        (item) => item.capability,
      )) {
        expect(
          capabilitySupport(definition.id, section.id).actions.length,
        ).toBeGreaterThan(0);
      }
    }
    expect(
      capabilitySupport("collaboration", "feature-comments-moderation").actions,
    ).toContainEqual({
      label: "Moderate team content",
      availability: "preview",
    });
  });
});
const request: WorkRow = {
  id: "leave",
  number: "LV-01",
  state: "draft",
  created_by: "employee",
  days: 2,
  start_date: "2026-09-14",
  updated_at: 1,
  handover_note: "Cover deliveries",
};
describe("Work responsibility", () => {
  it("keeps another person’s draft out of my actionable queue", () => {
    expect(queueFor(request, "employee", false)).toBe("todo");
    expect(queueFor(request, "manager", true)).toBeNull();
  });
  it("does not turn my own request into an approval when I change role", () => {
    const pending = { ...request, state: "pending_approval" };
    expect(queueFor(pending, "employee", true)).toBe("waiting");
    expect(queueFor(pending, "manager", true)).toBe("todo");
    expect(queueFor(pending, "viewer", false)).toBe("waiting");
  });
  it("keeps all terminal outcomes in completed work", () => {
    for (const state of ["approved", "rejected", "cancelled"])
      expect(queueFor({ ...request, state }, "employee", false)).toBe(
        "completed",
      );
  });
});
