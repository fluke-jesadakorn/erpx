import { describe, expect, it, vi } from "vitest";
import { createMemberDirectoryApplication, type MemberDirectoryRepository } from "../../../src/server/modules/organization/application/member-directory";
import { createIamThrottleApplication } from "../../../src/server/modules/identity_access/application/throttle";
import { createAccountInvitationListApplication } from "../../../src/server/modules/identity_access/application/account-invitation-list";
import { createMemberProfileViewApplication, type MemberProfileViewPorts } from "../../../src/server/modules/organization/application/member-profile-view";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";

describe("Organization directory privacy and owner query contract", () => {
  const actor = { userId: "self", companyId: "company-1", branchId: "branch-1", manager: false, beta: true };
  it("redacts colleague email while retaining own contact and verified Person links", async () => {
    const repository: MemberDirectoryRepository = {
      memberIds: vi.fn(async () => ["self", "colleague"]), accounts: vi.fn(async () => [{ id: "colleague", name: "Zulu", email: "private@example.test" }, { id: "self", name: "Alpha", email: "own@example.test" }]),
      personLinks: vi.fn(async () => [{ accountId: "colleague", personId: "person-1" }]),
    };
    const data = await createMemberDirectoryApplication(actor, repository, { now: () => 100 }).read();
    expect(data).toEqual([{ id: "self", name: "Alpha", email: "own@example.test", personId: null }, { id: "colleague", name: "Zulu", email: "", personId: "person-1" }]);
    expect(repository.memberIds).toHaveBeenCalledWith(actor, 100);
    expect(repository.personLinks).toHaveBeenCalledWith(actor.companyId, expect.any(Array), false);
  });
  it("enables the historical email fallback only for beta HR managers", async () => {
    const personLinks = vi.fn(async () => []);
    const repository = { memberIds: async () => ["colleague"], accounts: async () => [{ id: "colleague", name: "Member", email: "member@example.test" }], personLinks };
    const data = await createMemberDirectoryApplication({ ...actor, manager: true }, repository, { now: () => 100 }).read();
    expect(data[0].email).toBe("member@example.test"); expect(personLinks).toHaveBeenLastCalledWith(actor.companyId, expect.any(Array), true);
    await createMemberDirectoryApplication({ ...actor, manager: true, beta: false }, repository, { now: () => 100 }).read();
    expect(personLinks).toHaveBeenLastCalledWith(actor.companyId, expect.any(Array), false);
  });
  it("queries account batches at most 80 at a time and omits empty owner link reads", async () => {
    const accounts = vi.fn<(ids: string[]) => Promise<{ id: string; name: string; email: string }[]>>().mockResolvedValue([]), personLinks = vi.fn(async () => []);
    const repository = { memberIds: async () => Array.from({ length: 161 }, (_, index) => `member-${index}`), accounts, personLinks };
    expect(await createMemberDirectoryApplication(actor, repository, { now: () => 100 }).read()).toEqual([]);
    expect(accounts.mock.calls.map(([ids]) => ids.length)).toEqual([80, 80, 1]); expect(personLinks).not.toHaveBeenCalled();
  });
});

describe("Identity atomic throttle policy", () => {
  it("normalizes identity before an opaque key is minted and enforces the inclusive limit", async () => {
    const key = vi.fn(() => "opaque"), increment = vi.fn(async () => 8);
    await createIamThrottleApplication({ increment }, { now: () => 1000000 }, key).check("login", "Mixed@Example.Test");
    expect(key).toHaveBeenCalledWith("login", "mixed@example.test"); expect(increment).toHaveBeenCalledWith("opaque", 1000000, 700000);
    increment.mockResolvedValue(9);
    await expect(createIamThrottleApplication({ increment }, { now: () => 1000000 }, key).check("login", "Mixed@Example.Test")).rejects.toMatchObject({ status: 429, code: "IAM_RATE_LIMIT" });
  });
  it("fails closed if the counter is unavailable and honors custom windows", async () => {
    const increment = vi.fn(async () => null);
    await expect(createIamThrottleApplication({ increment }, { now: () => 100 }, () => "opaque").check("mfa", "member", 2, 50)).rejects.toMatchObject({ code: "IAM_RATE_LIMIT" });
    expect(increment).toHaveBeenCalledExactlyOnceWith("opaque", 100, 50);
  });
});

describe("Owner queries for invitation and member panels", () => {
  const actor: WorkContext = {
    holdingId: "h001", companyId: "company-1", branchId: "branch-1", departmentId: null,
    assignmentId: "assignment-1", userId: "self", role: "employee", name: "Self", email: "self@example.test",
    holdingSlug: "acme", holdingName: "Acme", inherited: false, modules: [], assignments: [], workspaces: [],
    scope: { kind: "branch", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme", companyId: "company-1", branchId: "branch-1", branchSlug: "branch", branchName: "Branch" }, assignmentScope: "holding",
  };
  const member = { id: "colleague", name: "Colleague", email: "", personId: "person-1" };
  function profilePorts(): MemberProfileViewPorts {
    return {
      profile: vi.fn(async () => ({ id: "person-1", engagement_id: "engagement-1" })),
      settings: vi.fn(async () => ({ levels: [], people: [], departments: [], canManage: false })),
      canManagePeople: vi.fn(() => false), access: vi.fn(async () => ({ account: null, assignments: [] })),
      levelHistory: vi.fn(async () => []), nodes: vi.fn(async () => []),
      organization: vi.fn<MemberProfileViewPorts["organization"]>(), hasPermission: vi.fn(() => false),
    };
  }
  it("does not read invitations or runtime prerequisites for a non-owner", async () => {
    const list = vi.fn(async () => []);
    expect(await createAccountInvitationListApplication({ list }).read(actor)).toBeNull();
    expect(list).not.toHaveBeenCalled();
  });
  it("passes the exact Holding, Company and optional Branch invitation scope to its owner repository", async () => {
    const list = vi.fn(async () => []);
    const app = createAccountInvitationListApplication({ list });
    expect(await app.read({ ...actor, role: "holding_owner" })).toEqual([]);
    expect(list).toHaveBeenLastCalledWith({ holdingId: actor.holdingId, companyId: actor.companyId, branchId: actor.branchId });
    await app.read({ ...actor, role: "holding_owner", branchId: null });
    expect(list).toHaveBeenLastCalledWith({ holdingId: actor.holdingId, companyId: actor.companyId, branchId: null });
  });
  it("propagates invitation availability failures instead of showing an empty success", async () => {
    const failure = new Error("Owner repository unavailable");
    await expect(createAccountInvitationListApplication({ list: async () => { throw failure; } }).read({ ...actor, role: "holding_owner" })).rejects.toBe(failure);
  });
  it("preserves member owner read ordering and omits manager-only editor data for staff", async () => {
    const ports = profilePorts(), calls: string[] = [];
    ports.profile = async () => { calls.push("person"); return { id: "person-1", engagement_id: "engagement-1" }; };
    ports.settings = async () => { calls.push("settings"); return { levels: [], people: [], departments: [], canManage: false }; };
    ports.access = async () => { calls.push("access"); return { account: null, assignments: [] }; };
    ports.levelHistory = async id => { calls.push(`history:${id}`); return []; };
    const result = await createMemberProfileViewApplication(ports).read(actor, member);
    expect(calls).toEqual(["person", "settings", "access", "history:engagement-1"]);
    expect(result).toMatchObject({ manage: false, nodes: [], organization: undefined, canManageAccess: false, canRevokeCompany: false });
    expect(ports.nodes).not.toHaveBeenCalled(); expect(ports.organization).not.toHaveBeenCalled();
  });
  it("does not read access or history when the authorized member has no Person link", async () => {
    const ports = profilePorts(); ports.profile = vi.fn(async () => null);
    const result = await createMemberProfileViewApplication(ports).read(actor, { ...member, personId: null });
    expect(result).toMatchObject({ person: null, access: null, history: [] });
    expect(ports.access).not.toHaveBeenCalled(); expect(ports.levelHistory).not.toHaveBeenCalled(); expect(ports.hasPermission).not.toHaveBeenCalled();
  });
  it("retains independent access controls even when the caller can manage People", async () => {
    const ports = profilePorts(); ports.canManagePeople = () => true;
    ports.hasPermission = (_actor, permission) => permission === "iam.roles.manage";
    const colleague = await createMemberProfileViewApplication(ports).read(actor, member);
    expect(colleague).toMatchObject({ manage: true, canManageAccess: false, canRevokeCompany: true });
    expect(ports.nodes).toHaveBeenCalledExactlyOnceWith(actor); expect(ports.organization).toHaveBeenCalledExactlyOnceWith(actor);
    const own = await createMemberProfileViewApplication(ports).read(actor, { ...member, id: actor.userId });
    expect(own).toMatchObject({ canManageAccess: false, canRevokeCompany: false });
  });
  it("does not expose history or settings after the Person owner rejects the scope", async () => {
    const ports = profilePorts(), failure = new Error("Person not found in this workspace");
    ports.profile = vi.fn(async () => { throw failure; });
    await expect(createMemberProfileViewApplication(ports).read(actor, member)).rejects.toBe(failure);
    expect(ports.settings).not.toHaveBeenCalled(); expect(ports.access).not.toHaveBeenCalled(); expect(ports.levelHistory).not.toHaveBeenCalled();
  });
});
