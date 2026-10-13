import { describe, expect, it, vi } from "vitest";
import { createInvitationDeliveryApplication, type InvitationDeliveryPorts } from "../../../src/server/modules/identity_access/application/invitation-delivery";
import { createOrganizationAccessApplication } from "../../../src/server/modules/organization/application/access";
import type { OrganizationAccessActor, OrganizationAccessSnapshot } from "../../../src/server/modules/organization/domain/access";
import { createResourceCleanupApplication } from "../../../src/server/modules/manage_erpx/application/resource-cleanup";
import { createPlatformOperationsApplication } from "../../../src/server/modules/platform_administration/application/operations-summary";
import type { HoldingJobStatistics } from "../../../src/server/modules/platform_administration/domain/operations-summary";

function invitationFixture() {
  const job = { id: "job-1", invitation_id: "invite-1", attempts: 1, lease_token: "lease-1" };
  const ports: InvitationDeliveryPorts = {
    clock: { now: () => 100000 }, identifiers: { next: () => "lease-1" }, activationLink: () => "https://erpx.example.test/activate?token=one",
    send: vi.fn(async () => "provider-1"),
    repository: { waiting: vi.fn(async () => [{ id: "awaiting-1", invitation_id: "invite-1" }]), review: vi.fn(async () => "pending"), promote: vi.fn(async () => {}), reject: vi.fn(async () => {}), recoverInterrupted: vi.fn(async () => {}), claim: vi.fn(async () => job), invitation: vi.fn(async () => ({ email: "member@example.test", name: "Member", status: "pending", expires_at: 100001 })), cancel: vi.fn(async () => {}), sent: vi.fn(async () => {}), failed: vi.fn(async () => {}) },
  };
  return { job, ports, application: createInvitationDeliveryApplication(ports) };
}
describe("Identity invitation delivery owns its state decisions", () => {
  it("requires an approved review and cancels a rejected invitation without sending", async () => {
    const f = invitationFixture(); await f.application.promote("holding-1");
    expect(f.ports.repository.promote).not.toHaveBeenCalled();
    f.ports.repository.review = vi.fn(async () => "approved"); await f.application.promote("holding-1");
    expect(f.ports.repository.promote).toHaveBeenCalledWith("awaiting-1", 100000);
    f.ports.repository.review = vi.fn(async () => "rejected"); await f.application.promote("holding-1");
    expect(f.ports.repository.reject).toHaveBeenCalledWith("awaiting-1", 100000);
    expect(f.ports.send).not.toHaveBeenCalled();
  });
  it("sends through the claimed lease and reuses the durable job key for provider idempotency", async () => {
    const f = invitationFixture(); expect(await f.application.deliver()).toBe(true);
    expect(f.ports.repository.claim).toHaveBeenCalledWith("lease-1", 100000);
    expect(f.ports.send).toHaveBeenCalledWith("member@example.test", "Activate your ERPX account", "https://erpx.example.test/activate?token=one", "job-1");
    expect(f.ports.repository.sent).toHaveBeenCalledWith("job-1", "lease-1", "provider-1", 100000);
  });
  it("rejects an invitation at its exact expiry without constructing a token or sending", async () => {
    const f = invitationFixture(); f.ports.repository.invitation = vi.fn(async () => ({ email: "member@example.test", name: "Member", status: "pending", expires_at: 100000 }));
    f.ports.activationLink = vi.fn(() => { throw new Error("must not mint an expired token"); });
    expect(await f.application.deliver()).toBe(true);
    expect(f.ports.repository.cancel).toHaveBeenCalledWith("job-1", "lease-1", 100000);
    expect(f.ports.send).not.toHaveBeenCalled(); expect(f.ports.activationLink).not.toHaveBeenCalled();
  });
  it("bounds retry delay and retains actionable configuration errors", async () => {
    const f = invitationFixture(); f.job.attempts = 6;
    f.ports.send = async () => { throw new Error("EMAIL_NOT_CONFIGURED"); };
    await f.application.deliver();
    expect(f.ports.repository.failed).toHaveBeenCalledWith("job-1", "lease-1", { state: "failed", code: "EMAIL_NOT_CONFIGURED", nextAttemptAt: 2020000 }, 100000);
    f.job.attempts = 9; f.ports.send = async () => { throw new Error("provider internal secret"); };
    await f.application.deliver();
    expect(f.ports.repository.failed).toHaveBeenLastCalledWith("job-1", "lease-1", { state: "failed", code: "EMAIL_DELIVERY_FAILED", nextAttemptAt: 3700000 }, 100000);
  });
});

describe("Organization scoped privacy projection", () => {
  const actor: OrganizationAccessActor = { actorId: "reader", branchId: null, departmentId: null, scopeKind: "holding", assignmentScope: "holding", mayReadIam: true, mayManagePeople: false, mayManageOrganization: false };
  const person: OrganizationAccessSnapshot["people"][number] = { person_id: "person-1", user_id: "user-1", legal_name: "Member", email: "private@example.test", engagement_id: "eng-1", engagement_version: 1, person_version: 1, company_id: "company-1", branch_id: "branch-1", department_id: "department-1", position_id: "position-1", manager_person_id: null, level_id: "level-1", level_number: 1, level_name: "CEO", status: "active", person_status: "active", start_date: "2026-01-01", end_date: null, job_title: "CEO", issues: [] };
  const position: OrganizationAccessSnapshot["positions"][number] = { id: "position-1", company_id: "company-1", branch_id: "branch-1", department_id: "department-1", title: "CEO", level_id: "level-1", level_number: 1, reports_to_position_id: null, is_ceo: 1, roster_required: 0, active: 1, version: 1 };
  const repository = () => ({ roster: vi.fn(async () => ({ people: [person], positions: [position] })), nodes: vi.fn(async () => []), levels: vi.fn(async () => []) });
  const clock = { now: () => Date.parse("2026-10-05T00:00:00Z") };
  it("rejects unauthorized readers before any Company information is loaded", async () => {
    const ports = repository();
    await expect(createOrganizationAccessApplication({ ...actor, mayReadIam: false }, ports, clock).read()).rejects.toMatchObject({ code: "PERMISSION_REQUIRED", status: 403 });
    expect(ports.roster).not.toHaveBeenCalled(); expect(ports.nodes).not.toHaveBeenCalled();
  });
  it("redacts IT contact/employment dates and returns HR details only with HR authority", async () => {
    const itData = await createOrganizationAccessApplication(actor, repository(), clock).read();
    expect(itData.people[0]).toMatchObject({ email: null, start_date: "", end_date: null });
    expect(itData.canMovePeople).toBe(false);
    const hr = await createOrganizationAccessApplication({ ...actor, mayManagePeople: true, mayManageOrganization: true }, repository(), clock).read();
    expect(hr.people[0].email).toBe("private@example.test"); expect(hr.canManage).toBe(true);
    expect(person.email).toBe("private@example.test");
  });
  it("filters Branch and Department placement while withholding Company commands", async () => {
    const scoped = { ...actor, scopeKind: "department", assignmentScope: "department", branchId: "branch-1", departmentId: "other-department", mayManagePeople: true, mayManageOrganization: true };
    const data = await createOrganizationAccessApplication(scoped, repository(), clock).read();
    expect(data.people).toEqual([]); expect(data.positions).toEqual([]); expect(data.canManage).toBe(false); expect(data.canMovePeople).toBe(false);
  });
});

describe("Manage ERPX bounded resource cleanup", () => {
  it("preserves attached uploads, removes only expired unbound objects and uses one clock", async () => {
    const removed: string[][] = [], expired: string[] = [], calls: number[] = [];
    const application = createResourceCleanupApplication({ clock: { now: () => 1000000 }, objects: { delete: async keys => { removed.push(keys); } }, repository: {
      async expiredUploads(before, limit) { calls.push(before, limit); return [{ id: "linked", revision: 1 }, { id: "unbound", revision: 2 }]; },
      async uploadHasAttachment(id) { return id === "linked"; }, async expireUpload(id, before, now) { expired.push(id); calls.push(before, now); },
      async expireAiReservations(pendingBefore, now) { calls.push(pendingBefore, now); },
    } });
    await application.cleanup("holding-1");
    expect(removed).toEqual([["holding-1/objects/unbound/r2", "holding-1/staging/unbound"]]); expect(expired).toEqual(["unbound"]);
    expect(calls).toEqual([940000, 20, 940000, 1000000, 880000, 1000000]);
  });
  it("does not falsely mark an upload expired when its file deletion fails", async () => {
    const expireUpload = vi.fn(async () => {}), expireAiReservations = vi.fn(async () => {});
    const application = createResourceCleanupApplication({ clock: { now: () => 1000000 }, objects: { delete: async () => { throw new Error("store unavailable"); } }, repository: { expiredUploads: async () => [{ id: "unbound", revision: 1 }], uploadHasAttachment: async () => false, expireUpload, expireAiReservations } });
    await expect(application.cleanup("holding-1")).rejects.toThrow("store unavailable");
    expect(expireUpload).not.toHaveBeenCalled(); expect(expireAiReservations).not.toHaveBeenCalled();
  });
});

describe("Platform independently observable Holding queues", () => {
  const identity = (id: string) => ({ id, slug: id, displayName: id });
  const statistics = (pendingEvents: number, deadLetterEvents = 0): HoldingJobStatistics => ({ pendingEvents, deadLetterEvents, claimedEvents: 0, oldestPendingAt: null, deadLetters: [] });
  it("retains healthy Holdings and totals when another binding fails", async () => {
    const application = createPlatformOperationsApplication({ controlHealth: async () => [{ service: "jobs", heartbeatAt: 99, details: "{}" }], jobs: async id => { if (id === "unavailable") throw new Error("private connection error"); return statistics(2, 1); }, migrations: async () => ({ track: "holding-write", applied: 4, pending: [], latestApplied: "0004", error: null }) }, { now: () => 100 });
    const result = await application.summarize("platform_admin", [identity("healthy"), identity("unavailable")]);
    expect(result.holdings.map(row => row.status)).toEqual(["attention", "unavailable"]);
    expect(result.totals).toEqual({ holdings: 2, pendingEvents: 2, deadLetterEvents: 1, claimedEvents: 0 });
    expect(result.holdings.every(row => row.jobHeartbeatAt === 99)).toBe(true);
    expect(JSON.stringify(result)).not.toContain("private connection error");
  });
  it("keeps job health visible if only migration receipts are unavailable", async () => {
    const application = createPlatformOperationsApplication({ controlHealth: async () => [], jobs: async () => statistics(1), migrations: async () => { throw new Error("missing migration table"); } }, { now: () => 100 });
    const result = await application.summarize("platform_admin", [identity("holding-1")]);
    expect(result.holdings[0]).toMatchObject({ status: "pending", error: null, migrations: { error: "Migration receipts are unavailable for this Holding." } });
  });
  it("keeps distinct input bindings observable even when their Holding identifiers coincide", async () => {
    const application = createPlatformOperationsApplication({ controlHealth: async () => [], jobs: async (_id, _now, index) => statistics(index + 1), migrations: async () => ({ track: "holding-write", applied: 1, pending: [], latestApplied: "0001", error: null }) }, { now: () => 100 });
    const result = await application.summarize("platform_admin", [identity("same-id"), identity("same-id")]);
    expect(result.holdings.map(row => row.pendingEvents)).toEqual([1, 2]);
    expect(result.totals.pendingEvents).toBe(3);
  });
});
