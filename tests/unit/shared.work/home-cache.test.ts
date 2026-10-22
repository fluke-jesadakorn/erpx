import { describe, expect, it, vi } from "vitest";
import { createHomeCache, type HomeCachePorts, type HomeFill } from "@erpx/server-work_projection/application/home-cache";
import type { CachedResult } from "@erpx/server-work_projection/domain/work-cache";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
const actor = { holdingId: "h001", companyId: "company", userId: "reader", assignmentId: "grant", role: "employee",
  scope: { kind: "holding" }, branchId: null, departmentId: null, modules: ["collaboration"],
  assignments: [{ id: "grant", version: 1, validUntil: 90_000 }],
  iam: { roleRevision: 1, identityVersion: 1, permissions: ["work.read"], freshUntil: 90_000 },
} as WorkContext;
function fixture() {
  let now = 1000, revision = "1", covered = true;
  const results = new Map<string, CachedResult>(), pending = new Map<string, HomeFill>(), redis = new Map<string, string>();
  const ports: HomeCachePorts = { results, pending, clock: { now: () => now }, hash: value => value, bytes: value => value.length,
    configuration: () => ({ environment: "test", databaseManifest: "manifest", deploymentId: "build" }),
    fence: vi.fn(async () => covered ? { revision, database: "db", validUntil: now + 30_000 } : null),
    redisEnabled: () => true, get: vi.fn(async key => redis.get(key) ?? null),
    set: vi.fn(async (key, value) => { redis.set(key, value); }), report: vi.fn(),
  };
  const service = createHomeCache(ports);
  const load = vi.fn(async () => ({ rows: [revision], unavailableSources: [] as string[], deadline: Infinity }));
  const policy = { complete: (value: Awaited<ReturnType<typeof load>>) => value.unavailableSources.length === 0,
    deadline: (value: Awaited<ReturnType<typeof load>>) => value.deadline ?? Infinity };
  const read = (context = actor, query: unknown = null) => service.read(context, { readFence: async () => null }, "feed", query, load, policy);
  return { service, read, load, policy, ports, results, pending, redis,
    tick: (time: number) => { now = time; }, commit: () => { revision = String(Number(revision) + 1); },
    uncover: () => { covered = false; } };
}
describe("Home revision cache", () => {
  it("coalesces fills, preserves ordering and counts, and isolates each returned object", async () => {
    const f = fixture();
    const values = await Promise.all(Array.from({ length: 5 }, () => f.read()));
    expect(f.load).toHaveBeenCalledTimes(1);
    values[0].rows.push("mutated");
    expect(values[1].rows).toEqual(["1"]);
    expect((await f.read()).rows).toEqual(["1"]);
    expect(f.load).toHaveBeenCalledTimes(1);
    expect(f.pending.size).toBe(0);
  });
  it("partitions users even with the same role, and every authority/scope boundary", async () => {
    const f = fixture(), baseline = f.service.contextKey(actor);
    const changes = [{ userId: "other" }, { holdingId: "h002" }, { companyId: "other" }, { branchId: "other" },
      { departmentId: "other" }, { assignmentId: "other" }, { role: "other" }, { scope: { kind: "branch" } },
      { modules: ["other"] }, { iam: { ...actor.iam, permissions: [] } }, { iam: { ...actor.iam, roleRevision: 2 } },
      { iam: { ...actor.iam, identityVersion: 2 } }, { assignments: [{ ...actor.assignments[0], version: 2 }] },
      { authority: { revision: 2 } }];
    for (const change of changes) {
      const context = { ...actor, ...change } as WorkContext;
      expect(f.service.contextKey(context)).not.toBe(baseline);
      await f.read(context);
    }
    await f.read();
    expect(f.load).toHaveBeenCalledTimes(changes.length + 1);
    const fence = { revision: "1", database: "db", validUntil: 31_000 };
    const key = f.service.key(actor, fence, "feed", null);
    expect(f.service.key(actor, { ...fence, database: "other" }, "feed", null)).not.toBe(key);
    expect(f.service.key(actor, fence, "audience", null)).not.toBe(key);
    expect(f.service.key(actor, fence, "feed", { beforeId: "next" })).not.toBe(key);
    for (const change of [{ environment: "production" }, { databaseManifest: "other" }, { deploymentId: "other" }]) {
      f.ports.configuration = () => ({ environment: "test", databaseManifest: "manifest", deploymentId: "build", ...change });
      expect(f.service.key(actor, fence, "feed", null)).not.toBe(key);
    }
  });
  it("rejects a fill crossing commit and never joins the old revision after commit", async () => {
    const f = fixture(); let release!: () => void, entered!: () => void;
    const started = new Promise<void>(resolve => { entered = resolve; });
    const gate = new Promise<void>(resolve => { release = resolve; });
    f.load.mockImplementationOnce(async () => { entered(); await gate; return { rows: ["old"], unavailableSources: [], deadline: Infinity }; });
    const first = f.read(); await started; f.commit();
    expect((await f.read()).rows).toEqual(["2"]);
    release(); expect((await first).rows).toEqual(["2"]);
    expect(f.load).toHaveBeenCalledTimes(2);
  });
  it("rechecks revision on hits and retries a changing hit", async () => {
    const f = fixture(); await f.read();
    const original = f.ports.fence;
    let calls = 0;
    f.ports.fence = async unit => { if (++calls === 2) f.commit(); return original(unit); };
    expect((await f.read()).rows).toEqual(["2"]);
    expect(f.load).toHaveBeenCalledTimes(2);
  });
  it("caps age at 30 seconds, authority and assignment expiry, and shift end", async () => {
    for (const boundary of ["age", "authority", "assignment", "shift"]) {
      const f = fixture();
      const context = boundary === "authority" ? { ...actor, iam: { ...actor.iam!, freshUntil: 2000 } } :
        boundary === "assignment" ? { ...actor, assignments: [{ ...actor.assignments[0], validUntil: 2000 }] } : actor;
      if (boundary === "shift") f.load.mockResolvedValue({ rows: ["1"], unavailableSources: [], deadline: 2000 });
      await f.read(context);
      f.tick(boundary === "age" ? 31_001 : 2001);
      await f.read(context);
      expect(f.load.mock.calls.length).toBeGreaterThan(1);
    }
  });
  it("bypasses incomplete or failed revision coverage, and does not keep failures/unavailable sources", async () => {
    const f = fixture(); f.uncover(); await f.read(); await f.read();
    expect(f.load).toHaveBeenCalledTimes(2); expect(f.results.size).toBe(0);
    const failed = fixture(); failed.ports.fence = async () => { throw Error("db"); };
    await failed.read(); expect(failed.results.size).toBe(0);
    const incomplete = fixture(); incomplete.load.mockResolvedValue({ rows: [], unavailableSources: ["Work"], deadline: Infinity });
    await incomplete.read(); await incomplete.read(); expect(incomplete.load).toHaveBeenCalledTimes(2);
    expect(incomplete.results.size).toBe(0); expect(incomplete.redis.size).toBe(0);
    const rejected = fixture(); rejected.load.mockRejectedValueOnce(Error("failed"));
    await expect(rejected.read()).rejects.toThrow("failed");
    expect(rejected.pending.size).toBe(0); await rejected.read(); expect(rejected.load).toHaveBeenCalledTimes(2);
  });
  it("reads PostgreSQL on Redis failure, recovers on retry, and shares the memory bound across namespaces", async () => {
    const f = fixture(); f.ports.get = async () => { throw Error("redis down"); }; f.ports.set = async () => { throw Error("redis down"); };
    expect((await f.read()).rows).toEqual(["1"]); await f.read(); expect(f.load).toHaveBeenCalledTimes(1);
    for (let index = 0; index < 140; index++) await f.read(actor, index);
    expect(f.results.size).toBe(128);
    f.load.mockResolvedValue({ rows: ["x".repeat(8 * 1024 * 1024)], unavailableSources: [], deadline: Infinity });
    await f.read(actor, "large"); expect([...f.results.values()].reduce((sum, entry) => sum + entry.bytes, 0)).toBeLessThanOrEqual(8 * 1024 * 1024);
  });
});
