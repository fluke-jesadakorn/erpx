import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));

import {
  createProjectActualsReader,
  type ActualsTransport,
  type ProjectActualsActor,
} from "../../../packages/server-projects/src/adapters/native-lane-27/client";

const ENV = "ERPX_NATIVE_PROJECTS_CONFIG";

/** Exact development value for ERPX_NATIVE_PROJECTS_CONFIG. */
const configValue = {
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme" },
    { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar" },
    { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx" },
  ],
};
const env = { [ENV]: JSON.stringify(configValue) };
const makeEnv = (value: unknown): Record<string, string | undefined> => ({ [ENV]: JSON.stringify(value) });

const actor: ProjectActualsActor = {
  companyId: "company-1",
  userId: "user-1",
  holdingId: "h001",
  holdingSlug: "acme",
};
const incomingFor = (host: string) => ({
  get: (name: string) =>
    name === "host" ? host : name === "origin" ? `https://${host}` : null,
});
const incoming = incomingFor("acme.jesadakorn.com");

const workLog = {
  id: "log-1",
  project_id: "project-1",
  user_id: "user-1",
  work_date: "2026-11-25",
  minutes: 60,
  state: "submitted",
  version: 1,
  reviewed_by: null,
  rejection_reason: "",
  created_at: 1,
  updated_at: 2,
  is_actor: true,
  can_review: false,
};
const cost = {
  id: "cost-1",
  project_id: "project-1",
  journal_line_id: "line-1",
  amount_minor: "1250",
  amount: "12.50",
  state: "draft",
  version: 1,
  created_by: "user-1",
  approved_by: null,
  rejection_reason: "",
  created_at: 1,
  updated_at: 2,
  journal_id: "journal-1",
  journal_number: "JV-1",
  entry_date: "2026-11-25",
  currency: "THB",
  journal_status: "posted",
  account_code: "5000",
  account_name: "Expense",
  can_review: true,
  is_creator: true,
};

const handoff = {
  id: "handoff-1",
  company_id: "company-1",
  project_id: "project-1",
  source_project_version: 4,
  recipient_id: "user-2",
  status: "pending",
  version: 1,
  snapshot_json: '{"packages":[],"tasks":[],"actuals":[],"edges":[],"unassignedTasks":[]}',
  snapshot_sha256: "a".repeat(64),
  evidence: "handoff evidence",
  prepared_by: "user-1",
  prepared_at: 100,
  decided_by: null,
  decided_at: null,
  decision_reason: null,
};
const handoffWork = {
  id: "handoff-1",
  version: 1,
  project_id: "project-1",
  recipient_id: "user-2",
  prepared_by: "user-1",
  status: "pending",
  prepared_at: 100,
  decided_at: null,
  title: "Project 1",
  lifecycle_status: "handoff_pending",
};

const projectRow = {
  id: "project-1",
  company_id: "company-1",
  branch_id: null,
  department_id: null,
  title: "Project 1",
  description: "Owner-local visible row",
  owner_user_id: "user-1",
  lifecycle_status: "active",
  version: 4,
  updated_at: 100,
};

const reply = (body: unknown, status = 200): ActualsTransport => async () => ({
  status,
  contentType: "application/json",
  text: JSON.stringify(body),
});

const reader = (
  transport: ActualsTransport,
  environment: Readonly<Record<string, string | undefined>> = env,
  who: ProjectActualsActor = actor,
  current = incoming,
) => createProjectActualsReader(who, current, environment, transport);

const failure = (run: () => unknown) => {
  try {
    run();
  } catch (error) {
    return error as { status: number; code: string };
  }
  throw new Error("expected the native reader to fail closed");
};

describe("native lane 27 projects actuals transport", () => {
  it("pins the owner KPI after the cutover", () => {
    const root = resolve(__dirname, "../../../packages/server-projects/src");
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts"))
          total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    // Before lane 27: 71. Actuals (2), handoff list/work (2) and the r30
    // visible-row head (1) are native; other owners keep the shared heads (66).
    expect(total).toBe(66);
  });

  it("pins the native SQL and result oracle", () => {
    const source = readFileSync(
      new URL(
        "../../../packages/server-projects/src/adapters/native_lane_27/postgres.rs",
        import.meta.url,
      ),
      "utf8",
    );
    const hashes = ["WORK_LOGS", "COST_ALLOCATIONS", "HANDOFFS", "HANDOFF_PROJECT", "HANDOFF_WORK", "VISIBLE_PROJECT", "VISIBLE_PROJECTS"].map((name) => {
      const literal = source.match(new RegExp(`const ${name}: &str = "([\\s\\S]*?)";`));
      expect(literal).not.toBeNull();
      const rendered = literal![1].replace(/\\\n\s*/g, "");
      return createHash("sha256").update(rendered).digest("hex");
    });
    expect(hashes).toEqual([
      "ed88d0261c964c7eec0fa2b94776b3ec91c34ef6c1cab5218e0c525e4fc581d3",
      "6240d5a2abc3be737145a146c2879a69091b5a0ba6e50b163ab61d23b65ba021",
      "2d0e7f0c956f8885ac03cf63817f88ad2d6c5ef065fcbdb5b87e1530f4f292ee",
      "4c0832698a78ad2a8c2140281e40a14f90fcfebe5ddffe5b0ea60a47f4f14088",
      "e22d5ff91897cea8271de4816a66db3730b2c840a10322e37f7997a710805280",
      "d177aa96424d2e995b3393adcce9ae26db436764742bd11336d494d11311928c",
      "30b6f9068cdb06f8462e9ba4f315d254df02dc1d6668ff22f43cf105e79e90ec",
    ]);
    expect(
      createHash("sha256")
        .update(JSON.stringify({ workLogs: [workLog], costAllocations: [cost] }))
        .digest("hex"),
    ).toBe("c6333aab03720cd3a36feb9bc6a1e4f99fb94372a5c168a5d8e06eec2de76cb1");
  });

  it("carries bounded identity headers and pins response shape", async () => {
    const seen: Array<{ url: string; headers: Record<string, string> }> = [];
    const transport: ActualsTransport = async (url, headers) => {
      seen.push({ url: url.toString(), headers: { ...headers } });
      return { status: 200, contentType: "application/json", text: JSON.stringify({ workLogs: [workLog] }) };
    };
    const rows = await reader(transport).workLogs("project-1");
    expect(rows).toEqual([workLog]);
    expect(seen[0]!.url).toBe(
      "http://127.0.0.1:3198/api/v1/holdings/acme/projects/actuals/work-logs",
    );
    expect(seen[0]!.headers).toMatchObject({
      host: "acme.jesadakorn.com",
      origin: "https://acme.jesadakorn.com",
      accept: "application/json",
      "x-erpx-context-company": "company-1",
      "x-erpx-context-user": "user-1",
      "x-erpx-context-project": "project-1",
    });
  });

  it("selects the caller's Holding and never defaults to the first", async () => {
    const cases = [
      { who: { ...actor, holdingId: "h002", holdingSlug: "northstar" }, host: "northstar.jesadakorn.com", slug: "northstar" },
      { who: { ...actor, holdingId: "h003", holdingSlug: "erpx" }, host: "erpx-internal.jesadakorn.com", slug: "erpx" },
    ] as const;
    for (const item of cases) {
      const seen: string[] = [];
      const transport: ActualsTransport = async (url, headers) => {
        seen.push(url.toString());
        expect(headers.host).toBe(item.host);
        expect(headers.origin).toBe(`https://${item.host}`);
        return { status: 200, contentType: "application/json", text: JSON.stringify({ workLogs: [] }) };
      };
      await reader(transport, env, item.who, incomingFor(item.host)).workLogs("project-1");
      expect(seen).toEqual([
        `http://127.0.0.1:3198/api/v1/holdings/${item.slug}/projects/actuals/work-logs`,
      ]);
    }
  });

  it("refuses unknown targets, wrong origins and company binding changes", async () => {
    expect(failure(() => reader(reply({ workLogs: [] }), env, { ...actor, holdingId: "h999" })))
      .toMatchObject({ status: 503, code: "WORK_CONTEXT_CHANGED" });
    expect(failure(() => reader(reply({ workLogs: [] }), env, { ...actor, holdingSlug: "erpx" })))
      .toMatchObject({ status: 503, code: "WORK_CONTEXT_CHANGED" });
    expect(failure(() => reader(reply({ workLogs: [] }), env, actor, incomingFor("northstar.jesadakorn.com"))))
      .toMatchObject({ status: 503, code: "WORK_CONTEXT_CHANGED" });
    const bound = {
      ...configValue,
      holdings: configValue.holdings.map((holding, index) =>
        index === 0 ? { ...holding, companyId: "bound-company" } : holding,
      ),
    };
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv(bound))))
      .toMatchObject({ status: 503, code: "WORK_CONTEXT_CHANGED" });
    await expect(
      createProjectActualsReader(
        { ...actor, companyId: "bound-company" },
        incoming,
        makeEnv(bound),
        reply({ workLogs: [] }),
      ).workLogs("project-1"),
    ).resolves.toEqual([]);
  });

  it("accepts 1..64 registered holdings and rejects invalid registries", async () => {
    const one = { ...configValue, holdings: [configValue.holdings[0]!] };
    await expect(reader(reply({ workLogs: [] }), makeEnv(one)).workLogs("project-1")).resolves.toEqual([]);
    const many = {
      ...configValue,
      holdings: Array.from({ length: 64 }, (_, index) => ({
        origin: `https://h${index}.jesadakorn.com`,
        host: `h${index}.jesadakorn.com`,
        holdingId: `id-${index}`,
        holdingSlug: `slug-${index}`,
      })),
    };
    // 64 registrations are accepted; the caller must still match one exactly.
    await expect(
      createProjectActualsReader(
        { ...actor, holdingId: "id-63", holdingSlug: "slug-63" },
        incomingFor("h63.jesadakorn.com"),
        makeEnv(many),
        reply({ workLogs: [] }),
      ).workLogs("project-1"),
    ).resolves.toEqual([]);
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv(many))))
      .toMatchObject({ status: 503, code: "WORK_CONTEXT_CHANGED" });
    const tooMany = { ...many, holdings: [...many.holdings, { ...many.holdings[0]!, holdingId: "id-64", holdingSlug: "slug-64", origin: "https://h64.jesadakorn.com", host: "h64.jesadakorn.com" }] };
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv(tooMany))))
      .toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_INVALID" });
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv({ ...configValue, holdings: [] }))))
      .toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_INVALID" });
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv({ ...configValue, holdings: [
      configValue.holdings[0]!, { ...configValue.holdings[1]!, holdingId: "h001", holdingSlug: "acme", origin: "https://other.jesadakorn.com", host: "other.jesadakorn.com" },
    ] })))).toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_INVALID" });
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv({ ...configValue, holdings: [
      { ...configValue.holdings[0]!, host: "wrong.jesadakorn.com" },
    ] })))).toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_INVALID" });
  });

  it("accepts the Rust-owned amount projection", async () => {
    const rows = await reader(reply({ costAllocations: [cost] })).costAllocations("project-1");
    expect(rows[0]!.amount).toBe("12.50");
    expect(rows[0]!.amount_minor).toBe("1250");
  });

  it("fails closed on an unavailable native route", async () => {
    await expect(reader(reply({ error: { code: "X", message: "down" } }, 503)).workLogs("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_UNAVAILABLE" });
    const failing: ActualsTransport = async () => { throw new Error("socket"); };
    await expect(reader(failing).costAllocations("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_UNAVAILABLE" });
  });

  it("fails closed on an invalid or off-shape reply", async () => {
    await expect(reader(reply({ workLogs: [{ ...workLog, minutes: 0 }] })).workLogs("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(reader(reply({ workLogs: [workLog], extra: 1 })).workLogs("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(reader(reply({ costAllocations: [{ ...cost, amount: "12.5" }] })).costAllocations("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
  });

  it("fails closed without valid configuration", () => {
    expect(failure(() => reader(reply({ workLogs: [] }), {} as Record<string, string | undefined>)))
      .toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_REQUIRED" });
    expect(failure(() => reader(reply({ workLogs: [] }), makeEnv({ ...configValue, timeoutMs: 100 }))))
      .toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_CONFIG_INVALID" });
  });

  it("routes handoff reads with bounded identity headers", async () => {
    const seen: Array<{ url: string; headers: Record<string, string> }> = [];
    const transport: ActualsTransport = async (url, headers) => {
      seen.push({ url: url.toString(), headers: { ...headers } });
      const body = url.pathname.endsWith("handoff-project")
        ? { handoff: { project_id: "project-1" } }
        : { handoffs: url.pathname.endsWith("handoff-work") ? [handoffWork] : [handoff] };
      return { status: 200, contentType: "application/json", text: JSON.stringify(body) };
    };
    const native = reader(transport);
    expect(await native.handoffs("project-1")).toEqual([handoff]);
    expect(seen[0]!.url).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/projects/handoffs");
    expect(seen[0]!.headers["x-erpx-context-project"]).toBe("project-1");

    expect(await native.handoffWork()).toEqual([handoffWork]);
    expect(seen[1]!.url).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/projects/handoff-work");
    expect(seen[1]!.headers["x-erpx-context-project"]).toBeUndefined();
    expect(seen[1]!.headers["x-erpx-context-user"]).toBe("user-1");

    expect(await native.handoffProject("handoff-1")).toEqual({ project_id: "project-1" });
    expect(seen[2]!.url).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/projects/handoff-project");
    expect(seen[2]!.headers["x-erpx-context-handoff"]).toBe("handoff-1");
  });

  it("selects the caller's Holding for handoff reads too", async () => {
    const seen: string[] = [];
    const transport: ActualsTransport = async (url, headers) => {
      seen.push(url.toString());
      expect(headers.host).toBe("northstar.jesadakorn.com");
      return { status: 200, contentType: "application/json", text: JSON.stringify({ handoffs: [] }) };
    };
    await reader(
      transport,
      env,
      { ...actor, holdingId: "h002", holdingSlug: "northstar" },
      incomingFor("northstar.jesadakorn.com"),
    ).handoffs("project-1");
    expect(seen).toEqual(["http://127.0.0.1:3198/api/v1/holdings/northstar/projects/handoffs"]);
  });

  it("fails closed on off-shape handoff replies and missing handoff identities", async () => {
    await expect(
      reader(reply({ handoffs: [{ ...handoff, snapshot_sha256: "A".repeat(64) }] })).handoffs("project-1"),
    ).rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(
      reader(reply({ handoffs: [{ ...handoffWork, status: "archived" }] })).handoffWork(),
    ).rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(
      reader(reply({ handoff: { project_id: "bad/id" } })).handoffProject("handoff-1"),
    ).rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(reader(reply({ handoff: null })).handoffProject("")).rejects.toMatchObject({
      status: 503,
      code: "PROJECT_ACTUALS_LANE27_REQUEST_INVALID",
    });
    await expect(reader(reply({ handoffs: [] })).handoffs("")).rejects.toMatchObject({
      status: 503,
      code: "PROJECT_ACTUALS_LANE27_REQUEST_INVALID",
    });
  });

  it("preserves native 4xx semantics", async () => {
    await expect(
      reader(reply({ error: { code: "PROJECT_ACTUALS_LANE27_REQUEST_INVALID", message: "bad" } }, 422)).workLogs("project-1"),
    ).rejects.toMatchObject({ status: 422, code: "PROJECT_ACTUALS_LANE27_REQUEST_INVALID" });
  });

  it("carries bounded identity for visible project reads", async () => {
    const seen: Array<{ url: string; headers: Record<string, string> }> = [];
    const transport: ActualsTransport = async (url, headers) => {
      seen.push({ url: url.toString(), headers: { ...headers } });
      return {
        status: 200,
        contentType: "application/json",
        text: JSON.stringify(url.pathname.endsWith("visible-projects") ? { projects: [projectRow] } : { project: projectRow }),
      };
    };
    const native = reader(transport);
    expect(await native.visibleProject("project-1")).toEqual(projectRow);
    expect(seen[0]!.url).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/projects/visible-project");
    expect(seen[0]!.headers["x-erpx-context-project"]).toBe("project-1");
    expect(await native.visibleProjects()).toEqual([projectRow]);
    expect(seen[1]!.url).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/projects/visible-projects");
    expect(seen[1]!.headers["x-erpx-context-project"]).toBeUndefined();
    expect(seen[1]!.headers["x-erpx-context-company"]).toBe("company-1");
    expect(seen[1]!.headers["x-erpx-context-user"]).toBe("user-1");
  });

  it("fails closed on off-shape visible project replies", async () => {
    await expect(reader(reply({ project: { ...projectRow, lifecycle_status: "unknown" } })).visibleProject("project-1"))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(reader(reply({ projects: [{ ...projectRow, extra: true }] })).visibleProjects())
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_RESPONSE_INVALID" });
    await expect(reader(reply({ project: null })).visibleProject(""))
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_REQUEST_INVALID" });
    await expect(reader(reply({ projects: [projectRow] }, 503)).visibleProjects())
      .rejects.toMatchObject({ status: 503, code: "PROJECT_ACTUALS_LANE27_UNAVAILABLE" });
  });

  it("selects the caller's Holding for visible project reads", async () => {
    const seen: string[] = [];
    const transport: ActualsTransport = async (url) => {
      seen.push(url.toString());
      return { status: 200, contentType: "application/json", text: JSON.stringify({ projects: [] }) };
    };
    await reader(
      transport,
      env,
      { ...actor, holdingId: "h002", holdingSlug: "northstar" },
      incomingFor("northstar.jesadakorn.com"),
    ).visibleProjects();
    expect(seen).toEqual(["http://127.0.0.1:3198/api/v1/holdings/northstar/projects/visible-projects"]);
  });
});
