import { expect, it, vi } from "vitest";
import { navigationSnapshot, workspaceNavigation, restrictedWorkspaceNavigation } from "@erpx/server-work_projection/public/navigation-snapshot";
import { businessRoles } from "@erpx/feature-identity_access/lib/business-roles";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { WorkSnapshot } from "@erpx/server-work_projection/public/work-items";

const reads = vi.hoisted(() => ({ work: vi.fn(), unread: vi.fn(), capabilities: vi.fn(), authorize: vi.fn() }));
vi.mock("@erpx/server-work_projection/public/work-items", () => ({ workSnapshot: reads.work }));
vi.mock("@erpx/server-work_projection/public/navigation-counts", () => ({ unreadHomeCount: reads.unread }));
vi.mock("@erpx/server-work_projection/public/work-capabilities", () => ({ getWorkCapabilities: reads.capabilities }));
vi.mock("@erpx/server-runtime/public/runtime", () => ({ holdingResources: async () => ({ write: "database" }) }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requireWorkContext: reads.authorize }));

const actor = { holdingId: "h001", role: "employee", scope: { kind: "holding" } } as WorkContext;
const complete: WorkSnapshot = { items: [
  { id: "later", type: "staff_task", title: "Later", detail: "Review", href: "/tasks/later", status: "pending", updatedAt: 1, queue: "todo" },
  { id: "first", type: "staff_task", title: "First", detail: "Review", href: "/tasks/first", status: "pending", updatedAt: 2, queue: "todo" },
  { id: "team", type: "staff_task", title: "Team", detail: "Wait", href: "/tasks/team", status: "pending", updatedAt: 3, queue: "waiting", relationships: ["team"] },
], unavailableSources: [] };
function fixture(snapshot = complete, unavailable: string[] = []) {
  reads.work.mockReset().mockResolvedValue(snapshot);
  reads.unread.mockReset().mockResolvedValue(4);
  reads.capabilities.mockReset().mockResolvedValue({ teamQueue: false, unavailable });
  reads.authorize.mockReset().mockResolvedValue(actor);
}
it("preserves both API formats, exact personal counts, priority order and team availability", async () => {
  fixture();
  const compact = await navigationSnapshot(actor, false);
  expect(compact).toEqual({ home: 4, work: 2, workViewAvailability: { team: true, routing: false } });
  const full = await navigationSnapshot(actor);
  expect(full.attention?.items.map(item => item.id)).toEqual(["first", "later"]);
  expect(full.attention?.total).toBe(full.work);
  expect(full.snapshotAt).toBeGreaterThan(0);
});
it("retains unknown counts for missing sources and the compact capability contract", async () => {
  fixture(complete, ["capability unavailable"]);
  expect((await navigationSnapshot(actor, false)).work).toBe(2);
  expect(await navigationSnapshot(actor)).toMatchObject({ work: null, attention: { incomplete: true } });
  fixture({ ...complete, unavailableSources: ["tasks"] });
  expect((await navigationSnapshot(actor, false)).work).toBeNull();
});
it("passes every role and scope's authorized actor unchanged to all readers", async () => {
  for (const role of ["holding_owner", "branch_manager", "department_head", ...Object.keys(businessRoles)]) {
    for (const kind of ["holding", "branch", "department"] as const) {
      fixture();
      const scoped = { ...actor, role, scope: { ...actor.scope, kind } } as WorkContext;
      await navigationSnapshot(scoped);
      expect(reads.work).toHaveBeenCalledExactlyOnceWith(scoped);
      expect(reads.unread).toHaveBeenCalledExactlyOnceWith("database", scoped);
      expect(reads.capabilities).toHaveBeenCalledExactlyOnceWith("database", scoped);
    }
  }
});
it("starts one Work read and exposes that same result for My day", async () => {
  fixture();
  const read = await workspaceNavigation("acme");
  expect(reads.authorize).toHaveBeenCalledExactlyOnceWith("acme");
  expect(await read.initialWork).toBe(complete);
  expect(await read.initialNavigation).toMatchObject({ work: 2 });
  expect(reads.work).toHaveBeenCalledTimes(1);
});
it("rejects denied contexts before starting a read and permits seed failure to retry", async () => {
  fixture(); reads.authorize.mockRejectedValueOnce(Error("Forbidden"));
  await expect(workspaceNavigation("denied")).rejects.toThrow("Forbidden");
  expect(reads.work).not.toHaveBeenCalled();
  reads.work.mockRejectedValueOnce(Error("Offline"));
  const read = await workspaceNavigation("acme");
  expect(await read.initialNavigation).toBeNull();
});


it("keeps an included-source shell on the current context without loading or serializing private Work snapshots",async()=>{
 fixture();const page=await restrictedWorkspaceNavigation('acme');expect(page.context).toBe(actor);expect(reads.authorize).toHaveBeenCalledExactlyOnceWith('acme');expect(page.initialNavigation).toBeUndefined();expect(Object.keys(page)).not.toContain('initialWork');expect(reads.work).not.toHaveBeenCalled();expect(reads.unread).not.toHaveBeenCalled();expect(reads.capabilities).not.toHaveBeenCalled();expect(JSON.stringify(page)).not.toContain('Later');
 fixture();const full=await workspaceNavigation('acme');expect(await full.initialNavigation).toMatchObject({work:2});expect(reads.work).toHaveBeenCalledOnce();
});
