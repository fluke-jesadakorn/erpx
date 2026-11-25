import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
const incomingHeaders = vi.hoisted(() => ({ value: new Headers({ host: "acme.jesadakorn.com" }) }));
vi.mock("next/headers", () => ({ headers: async () => incomingHeaders.value }));
import { createHash } from "node:crypto";
import {
  LANE8_RD_APPROVER_NAME,
  LANE8_RD_PAY_GROUPS,
  LANE8_RD_PEOPLE,
  LANE8_RD_ROSTER_SHIFT,
  lane8Configuration,
  nativeLane8Configured,
  readApproverName,
  readHireHandoffPayGroups,
  readHireHandoffPeople,
  readRosterShift,
  type Lane8Actor,
} from "../../../packages/server-people_payroll/src/adapters/native-lane-8/client";

const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE8_CONFIG";
const original = process.env[ENV];
/** Exact development configuration pin: three registered Holdings. */
const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:44994",
  timeoutMs: 1000,
  holdings: [
    { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme", companyId: "company-acme" },
    { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar", companyId: "company-northstar" },
    { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx", companyId: "company-erpx" },
  ],
} as const;
const config = (mutate: (value: Record<string, any>) => void = () => undefined) => {
  const value = JSON.parse(JSON.stringify(DEV_CONFIG)) as Record<string, any>;
  mutate(value);
  return JSON.stringify(value);
};
const db = {} as never;
const ok = (data: unknown) =>
  new Response(JSON.stringify({ data }), { status: 200, headers: { "content-type": "application/json" } });
const digest = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const actor = (holdingId: string, holdingSlug: string, companyId: string): Lane8Actor =>
  ({ holdingId, holdingSlug, companyId });
const urlOf = (call: number) => (vi.mocked(fetch).mock.calls[call] as unknown as [URL, RequestInit])[0];
const headersOf = (call: number) =>
  (vi.mocked(fetch).mock.calls[call] as unknown as [URL, RequestInit])[1].headers as Record<string, string>;

const groups = [
  { id: "g1", name: "Monthly", currency: "THB", frequency: "monthly" },
];
const people = [
  { id: "p1", legal_name: "Alice", email: null, version: 1 },
];
const approver = { legal_name: "Alice" };
const shift = {
  id: "s1",
  local_date: "2026-11-25",
  start_time: "09:00",
  end_time: "17:00",
  end_local: "2026-11-25T17:00",
  timezone: "Asia/Bangkok",
};

beforeEach(() => {
  process.env[ENV] = config();
  incomingHeaders.value = new Headers({ host: "acme.jesadakorn.com" });
});
afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
  vi.unstubAllGlobals();
});

describe("native People Payroll lane 8 app-entrypoint reads", () => {
  it("resolves the exact registered Host for legacy fact callers and pins the result oracle", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(groups)));
    const result = await readHireHandoffPayGroups(db, "company-acme");
    expect(result.results).toEqual(groups);
    expect(result.success).toBe(true);
    expect(digest(result.results)).toBe("38773d85c8928b525431ae7c77e65dda093fca91da66ebe5ed57a21dc75fded0");
    expect(urlOf(0).toString()).toBe("http://127.0.0.1:44994/api/v1/holdings/acme/people-payroll/app-entrypoint-read");
    expect(vi.mocked(fetch).mock.calls[0][1]?.method).toBe("POST");
    expect(headersOf(0).host).toBe("acme.jesadakorn.com");
    expect(headersOf(0).origin).toBe("https://acme.jesadakorn.com");
    expect(headersOf(0)["content-type"]).toBe("application/json");
    expect(headersOf(0)["x-erpx-context-company"]).toBe("company-acme");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_PAY_GROUPS,
      values: [],
    });
  });

  it("selects a non-first Holding from the WorkContext, not the first configured target", async () => {
    incomingHeaders.value = new Headers({ host: "northstar.jesadakorn.com" });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(groups)));
    const result = await readHireHandoffPayGroups(db, "company-northstar", actor("h002", "northstar", "company-northstar"));
    expect(result.results).toEqual(groups);
    expect(urlOf(0).toString()).toBe("http://127.0.0.1:44994/api/v1/holdings/northstar/people-payroll/app-entrypoint-read");
    expect(headersOf(0).host).toBe("northstar.jesadakorn.com");
  });

  it("fails closed on unknown actor targets, unknown Host and actor/Host disagreement", async () => {
    vi.stubGlobal("fetch", vi.fn());
    for (const a of [
      actor("h999", "acme", "company-acme"),
      actor("h001", "other", "company-acme"),
      actor("h001", "acme", "company-northstar"),
    ]) {
      await expect(readHireHandoffPeople(db, "company-acme", a)).rejects.toMatchObject({
        code: "WORK_CONTEXT_CHANGED",
        status: 409,
      });
    }
    incomingHeaders.value = new Headers({ host: "evil.example.com" });
    await expect(readHireHandoffPeople(db, "company-acme")).rejects.toMatchObject({
      code: "WORK_CONTEXT_CHANGED",
      status: 409,
    });
    incomingHeaders.value = new Headers({ host: "acme.jesadakorn.com" });
    await expect(readHireHandoffPeople(db, "company-acme", actor("h002", "northstar", "company-northstar")))
      .rejects.toMatchObject({ code: "WORK_CONTEXT_CHANGED", status: 409 });
    expect(fetch).not.toHaveBeenCalled();
  });

  it("reads owner people options, approver names and roster shifts with pinned result oracles", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(people)));
    expect((await readHireHandoffPeople(db, "company-acme")).results).toEqual(people);
    expect(digest(people)).toBe("2678db1a00e220011dc1779030f6bb286740ab6c389f8be83b78d7e43fb01fae");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({ kind: LANE8_RD_PEOPLE, values: [] });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(approver)));
    expect(await readApproverName(db, "company-acme", "user-1")).toEqual(approver);
    expect(digest(approver)).toBe("a5d848e359e76f7e6045ad87db0197cffa5a17bc157a664a4eec3899bb124f3b");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_APPROVER_NAME, values: ["user-1"],
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(null)));
    expect(await readApproverName(db, "company-acme", "user-2")).toBeNull();

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(shift)));
    expect(await readRosterShift(db, "shift-1", "company-acme", "eng-1")).toEqual(shift);
    expect(digest(shift)).toBe("d8dc80742c80e477b90329c6cfe9b783db3f377e50dc0a2865dcf5b8f16f0ee7");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_ROSTER_SHIFT, values: ["shift-1", "eng-1"],
    });
  });

  it("rejects malformed rows, extra envelope keys and unbounded collections", async () => {
    const cases: unknown[] = [
      [{ ...groups[0], extra: "field" }],
      [{ id: "g1", name: "Monthly", currency: "THB" }],
      [{ id: "../g1", name: "Monthly", currency: "THB", frequency: "monthly" }],
      Array.from({ length: 5001 }, () => groups[0]),
    ];
    for (const data of cases) {
      vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(data)));
      await expect(readHireHandoffPayGroups(db, "company-acme")).rejects.toMatchObject({
        code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE",
        status: 503,
      });
    }
    const malformed = new Response(JSON.stringify({ data: groups, oracle: "x" }), {
      status: 200, headers: { "content-type": "application/json" },
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(malformed));
    await expect(readHireHandoffPayGroups(db, "company-acme")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE",
    });
  });

  it("fails closed for missing configuration, transport failure and 5xx", async () => {
    delete process.env[ENV];
    await expect(readHireHandoffPeople(db, "company-acme")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    process.env[ENV] = config();
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));
    await expect(readHireHandoffPeople(db, "company-acme")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE",
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", {
      status: 503, headers: { "content-type": "application/json" },
    })));
    await expect(readHireHandoffPeople(db, "company-acme")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE",
    });
    expect(nativeLane8Configured()).toBe(true);
  });

  it("surfaces owner 4xx and rejects a Company outside the resolved Holding", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID", message: "Rejected." } }),
      { status: 422, headers: { "content-type": "application/json" } },
    )));
    await expect(readRosterShift(db, "../shift", "company-acme", "eng-1")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID", status: 422, message: "Rejected.",
    });
    await expect(readRosterShift(db, "shift-1", "other-company", "eng-1")).rejects.toMatchObject({
      code: "WORK_CONTEXT_CHANGED", status: 409,
    });
  });

  it("rejects insecure, duplicated or malformed native configuration", () => {
    expect(lane8Configuration(config()).holdings).toHaveLength(3);
    for (const value of [
      config(v => { v.privateBaseUrl = "http://127.0.0.1:3100"; }),
      config(v => { v.privateBaseUrl = "https://127.0.0.1:44994"; }),
      config(v => { v.holdings = []; }),
      config(v => { v.holdings[0].origin = "http://acme.jesadakorn.com"; }),
      config(v => { v.holdings[1].host = v.holdings[0].host; }),
      config(v => { v.holdings[1].holdingId = v.holdings[0].holdingId; v.holdings[1].holdingSlug = v.holdings[0].holdingSlug; }),
      "{not-json",
    ]) {
      expect(() => lane8Configuration(value)).toThrow();
    }
  });
});
