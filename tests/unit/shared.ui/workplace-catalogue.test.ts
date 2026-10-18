import { expect, it, vi } from "vitest";
import { moduleRegistry } from "../../../src/data/module-registry";
import { availableNavigationAppIds, workplaceModuleDestinations } from "../../../src/lib/work_projection/workplace-catalogue";
import { workplaceNavigation } from "../../../src/lib/work_projection/workplace-navigation";
import { frontWorkReturn } from "../../../src/lib/work_projection/front-work-return";
import { workReviewCommand } from "../../../src/lib/work_projection/work-review";

const migrationPorts = vi.hoisted(() => ({ bindings: vi.fn(), workspace: vi.fn(), grants: vi.fn() }));
vi.mock("@/server/modules/runtime/public/runtime", () => ({ activeHoldingBindings: migrationPorts.bindings }));
vi.mock("@/server/modules/identity_access/public/authorization", () => ({ firstAuthorizedWorkspace: migrationPorts.workspace }));
vi.mock("@/server/modules/commercial/public/package-grants", () => ({ companyPackageGrants: migrationPorts.grants }));

it("keeps existing workspace recovery scoped to current authority without reading or inventing product grants", async () => {
  vi.stubEnv("ERPX_PUBLIC_ORIGIN", "https://console-dev-erpx.jesadakorn.com");
  try {
    migrationPorts.bindings.mockResolvedValue([{ slug: "acme", displayName: "Acme" }, { slug: "denied", displayName: "Other company" }]);
    migrationPorts.workspace.mockImplementation(async (holding: string) => holding === "acme" ? { kind: "branch", holdingSlug: "acme", branchSlug: "main" } : null);
    const { existingWorkspaceLaunchChoices } = await import("@/server/modules/work_projection/public/product-launcher");
    expect(await existingWorkspaceLaunchChoices()).toEqual([{ holding: "Acme", href: "https://console-dev-erpx.jesadakorn.com/holdings/acme/branches/main/home" }]);
    expect(migrationPorts.grants).not.toHaveBeenCalled();
    migrationPorts.workspace.mockResolvedValue(null);
    expect(await existingWorkspaceLaunchChoices()).toEqual([]);
    vi.stubEnv("ERPX_PUBLIC_ORIGIN", "");
    expect(await existingWorkspaceLaunchChoices()).toEqual([]);
  } finally { vi.unstubAllEnvs(); }
});

it("gives every catalogue module an objective and discovery home without opening deferred features",()=>{
  expect(Object.keys(workplaceModuleDestinations).sort()).toEqual(moduleRegistry.map(module=>module.id).sort());
  const available=availableNavigationAppIds([{id:"organization",state:"Available"},{id:"planning",state:"Coming soon"},{id:"crm",state:"Pilot",permitted:false},{id:"finance",state:"Setup required"}]);
  const model=workplaceNavigation({workspaceBase:"/holdings/acme",role:"holding_owner",scopeKind:"holding",moduleIds:["organization","planning","crm","finance"],availableAppIds:available});
  expect(model.categories.some(category=>category.id==="production")).toBe(false);
  expect(model.search.some(item=>item.id==="crm-pipeline")).toBe(false);
  expect(model.categories.some(category=>category.id==="finance")).toBe(true);
  expect(model.categories.every(category=>category.apps.length>0)).toBe(true);
});

it("retains same-holding Front location and source queries while rejecting foreign return targets",()=>{
  const target="/front/acme/company/pms/property?record=reservation&guestAccount=guestAccount&queue=arrivals#work";
  expect(frontWorkReturn(target,"acme")).toBe(target);
  expect(frontWorkReturn(target,"acme","company")).toBe(target);
  expect(frontWorkReturn(target,"acme","other-company")).toBeNull();
  for(const value of ["https://evil.invalid/front/acme/company/pms/property","//evil.invalid/front/acme/company/pms/property","/front/other/company/pms/property","/holdings/acme/home","/front/acme/company/pos/%2fother","/front/acme/company/pms/%5cother","/front/acme/company/pms/%ZZ"])
    expect(frontWorkReturn(value,"acme")).toBeNull();
});

it("constructs only owner-supported decision commands and carries the read version",()=>{
  expect(workReviewCommand("leave","a/b",7,"reject","Evidence checked")).toEqual({path:"/leave-requests/a%2Fb/transition",body:{action:"reject",expectedVersion:7,note:"Evidence checked",override:false}});
  expect(workReviewCommand("staff_task","task",8,"approve","Checked").body).toMatchObject({expectedVersion:8,action:"approve"});
  expect(workReviewCommand("equipment_request","request",4,"return","Details missing").path).toBe("/requests/request/actions");
  expect(()=>workReviewCommand("expense","expense",2,"return","Fix this")).toThrow();
});
