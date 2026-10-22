import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

const state = vi.hoisted(() => ({
  db: { identity: "provided Holding database" }, read: { identity: "private read replica" }, files: { identity: "private file store" },
  actor: { userId: "user", holdingId: "holding", companyId: "company", branchId: null },
  holding: vi.fn(), env: vi.fn(), iam: vi.fn(), staff: vi.fn(), portal: vi.fn(), booking: vi.fn(),
  settings: vi.fn(), organization: vi.fn(), lifecycle: vi.fn(), management: vi.fn(), project: vi.fn(),
  portalList: vi.fn(), catalog: vi.fn(), provisioning: vi.fn(), bindings: vi.fn(), operations: vi.fn(),
  workspace: vi.fn(), slot: vi.fn(),
}));
vi.mock("@erpx/server-runtime/public/runtime", () => ({ holdingResources: state.holding, runtimeEnv: state.env, activeHoldingBindings: state.bindings }));
vi.mock("@erpx/server-operations/public/staff-settings", () => ({ staffSettings: state.settings }));
vi.mock("@erpx/server-organization/public/organization", () => ({ organizationOverview: state.organization }));
vi.mock("@erpx/server-people_payroll/public/people-lifecycle", () => ({ peopleLifecycleData: state.lifecycle }));
vi.mock("@erpx/server-identity_access/public/iam-context", () => ({ iamContext: state.iam }));
vi.mock("@erpx/server-identity_access/public/user-management", () => ({ readManagement: state.management }));
vi.mock("@erpx/server-work_projection/public/staff-page", () => ({ staffPage: state.staff }));
vi.mock("@erpx/server-operations/public/staff-tasks", () => ({ getProject: state.project, listTasks: vi.fn(), taskDetail: vi.fn() }));
vi.mock("@erpx/server-legal/public/legal/portal", () => ({ portalContext: state.portal, portalList: state.portalList }));
vi.mock("@erpx/server-pms/public/pms-booking", () => ({ bookingDatabase: state.booking, bookingCatalog: state.catalog }));
vi.mock("@erpx/server-platform_administration/public/platform-operations", () => ({ platformProvisioningSnapshot: state.provisioning, summarizePlatformOperations: state.operations }));
vi.mock("@erpx/server-work_projection/public/workspace-preferences", () => ({ readWorkspacePreference: state.workspace }));
vi.mock("@erpx/server-manage_erpx/public/people-app-entrypoint-facts", () => ({ ownerOnboardingSlot: state.slot }));
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";
import { staffPresentationQueries } from "@erpx/server-work_projection/public/presentation-context-queries";
import { legalPortalPresentationQueries } from "@erpx/server-legal/public/presentation-context-queries";
import { bookingPresentationQueries } from "@erpx/server-pms/public/presentation-context-queries";
import { platformOperationsForPresentation } from "@erpx/server-platform_administration/public/presentation-queries";
import { publicBetaEnabled } from "@erpx/server-runtime/public/presentation-configuration";
import { savedWorkspaceForPresentation } from "@erpx/server-work_projection/public/workspace-entry-presentation-query";
import { onboardingPresentationQueries } from "@erpx/server-manage_erpx/public/onboarding-presentation-query";
const actor = state.actor as unknown as WorkContext;
beforeEach(() => {
  vi.clearAllMocks();
  state.holding.mockResolvedValue({ write: state.db, read: state.read, files: state.files });
  state.env.mockResolvedValue({ CONTROL_DB: state.db, CONTROL_READ_DB: state.read, ERPX_PUBLIC_BETA: "true" });
  state.iam.mockResolvedValue({ db: state.db, actor });
  state.staff.mockResolvedValue({ write: state.db, actor, base: "/holdings/acme", options: { levels: [] }, projects: [], staff: null });
  state.portal.mockResolvedValue({ write: state.db, read: state.read, files: state.files, actor });
  state.booking.mockResolvedValue(state.db);
  for (const mock of [state.settings, state.organization, state.lifecycle, state.management, state.project, state.portalList, state.catalog, state.workspace, state.slot]) mock.mockResolvedValue([]);
});

describe("Owner presentation query composition", () => {
  it("keeps Holding acquisition lazy and hides database and file capabilities", async () => {
    const query = await peoplePayrollPresentationQueries("holding");
    expect(state.holding).toHaveBeenCalledExactlyOnceWith("holding");
    expect(state.settings).not.toHaveBeenCalled();
    expect(Object.values(query).every(value => typeof value === "function")).toBe(true);
    expect(query).not.toHaveProperty("write"); expect(query).not.toHaveProperty("read"); expect(query).not.toHaveProperty("files");
    await query.staffSettings(actor);
    expect(state.settings).toHaveBeenCalledExactlyOnceWith(state.db, actor);
  });
  it("passes the same prepared connection across People Organization and Operations reads", async () => {
    const query = await peoplePayrollPresentationQueries("holding");
    state.holding.mockResolvedValue({ write: { identity: "later scope" } });
    await Promise.all([query.staffSettings(actor), query.organizationOverview(actor), query.peopleLifecycleData(actor)]);
    for (const mock of [state.settings, state.organization, state.lifecycle]) expect(mock).toHaveBeenCalledExactlyOnceWith(state.db, actor);
    expect(state.holding).toHaveBeenCalledTimes(1);
  });
  it("propagates resource and reader failures without converting visibility errors", async () => {
    const failure = Error("Holding unavailable"); state.holding.mockRejectedValueOnce(failure);
    await expect(peoplePayrollPresentationQueries("holding")).rejects.toBe(failure);
    const query = await peoplePayrollPresentationQueries("holding"), denied = { status: 403, code: "PERMISSION_REQUIRED" };
    state.organization.mockRejectedValueOnce(denied);
    await expect(query.organizationOverview(actor)).rejects.toBe(denied);
  });
  it("retains the IAM actor and provided native connection while hiding its database", async () => {
    const query = await iamPresentationQueries("acme");
    expect(query.actor).toBe(actor); expect(query).not.toHaveProperty("db");
    await query.queries.readManagement(query.actor);
    expect(state.management).toHaveBeenCalledExactlyOnceWith(state.db, actor);
    expect(state.iam).toHaveBeenCalledExactlyOnceWith("acme");
  });
  it("retains staff page data and lazy cross-owner readers without returning its write handle", async () => {
    const query = await staffPresentationQueries("acme");
    expect(query).toMatchObject({ actor, base: "/holdings/acme", options: { levels: [] }, projects: [], staff: null });
    expect(query).not.toHaveProperty("write"); expect(state.project).not.toHaveBeenCalled();
    await query.queries.getProject(actor, "project");
    expect(state.project).toHaveBeenCalledExactlyOnceWith(state.db, actor, "project");
  });
  it('requests optional Project choices only for the explicit Routine presentation while preserving the default required context call',async()=>{
    await staffPresentationQueries('acme',{routineProjects:true});expect(state.staff).toHaveBeenLastCalledWith('acme',{routineProjects:true});await staffPresentationQueries('acme');expect(state.staff).toHaveBeenLastCalledWith('acme');
  });
  it('requests optional Project choices explicitly for standalone Tasks without exposing the server-only picker mode',async()=>{
    state.staff.mockResolvedValueOnce({write:state.db,actor,base:'/holdings/acme',options:{levels:[]},projects:[],staff:null,projectPickerStrict:true});
    const query=await staffPresentationQueries('acme',{projectOptions:'optional'});expect(state.staff).toHaveBeenCalledExactlyOnceWith('acme',{projectOptions:'optional'});expect(query).not.toHaveProperty('projectPickerStrict');expect(query).not.toHaveProperty('write');
  });
  it("removes every portal resource while retaining its actor and same-connection document read", async () => {
    const query = await legalPortalPresentationQueries("acme");
    expect(Object.keys(query).sort()).toEqual(["actor", "queries"]);
    await query.queries.portalList(query.actor);
    expect(state.portalList).toHaveBeenCalledExactlyOnceWith(state.db, actor);
  });
  it("separates booking resource resolution from catalogue reading and preserves each error", async () => {
    const missing = Error("HOLDING_NOT_FOUND"); state.booking.mockRejectedValueOnce(missing);
    await expect(bookingPresentationQueries("absent")).rejects.toBe(missing); expect(state.catalog).not.toHaveBeenCalled();
    const query = await bookingPresentationQueries("acme");
    expect(Object.keys(query)).toEqual(["bookingCatalog"]);
    const failure = Error("catalogue unavailable"); state.catalog.mockRejectedValueOnce(failure);
    await expect(query.bookingCatalog()).rejects.toBe(failure);
    expect(state.catalog).toHaveBeenCalledExactlyOnceWith(state.db);
  });
  it("preserves platform provisioning then Holding preparation then summary order without exposing resources", async () => {
    const events: string[] = [];
    state.provisioning.mockImplementation(async () => { events.push("provisioning"); return { version: 2 }; });
    state.bindings.mockImplementation(async () => { events.push("bindings"); return [{ id: "h003" }, { id: "h001", slug: "acme", displayName: "Acme" }]; });
    state.holding.mockImplementation(async () => { events.push("holding"); return { write: state.db }; });
    state.operations.mockImplementation(async () => { events.push("summary"); return { generatedAt: 100 }; });
    const platformActor = { userId: "admin", role: "platform_admin" } as Parameters<typeof platformOperationsForPresentation>[0];
    expect(await platformOperationsForPresentation(platformActor)).toEqual({ provisioning: { version: 2 }, operations: { generatedAt: 100 } });
    expect(events).toEqual(["provisioning", "bindings", "holding", "summary"]);
    expect(state.holding).toHaveBeenCalledExactlyOnceWith("h001");
    expect(state.operations).toHaveBeenCalledExactlyOnceWith(platformActor, state.db, [{ id: "h001", slug: "acme", displayName: "Acme", db: state.db }]);
  });
  it("projects public registration and onboarding scalars without exposing the runtime environment", async () => {
    expect(await publicBetaEnabled()).toBe(true);
    state.env.mockResolvedValueOnce({ CONTROL_DB: state.db, ERPX_PUBLIC_BETA: "false" });
    const query = await onboardingPresentationQueries();
    expect(Object.keys(query).sort()).toEqual(["publicBetaEnabled", "slot"]); expect(query.publicBetaEnabled).toBe(false);
    await query.slot("user"); expect(state.slot).toHaveBeenCalledExactlyOnceWith(state.db, "user");
  });
  it("reads the saved workspace through the private Control connection with the original account identity", async () => {
    state.workspace.mockResolvedValueOnce({ workspace_path: "/holdings/acme/home" });
    expect(await savedWorkspaceForPresentation("user")).toEqual({ workspace_path: "/holdings/acme/home" });
    expect(state.workspace).toHaveBeenCalledExactlyOnceWith(state.db, "user");
  });
});
