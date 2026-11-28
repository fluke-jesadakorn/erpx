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
  LANE8_RD_ISSUABLE_RUN,
  LANE8_RD_ISSUABLE_ITEMS,
  LANE8_RD_OWN_SLIPS,
  LANE8_RD_SLIP,
  LANE8_RD_PAYMENT,
  LANE8_RD_EXPORT_BYTES,
  LANE8_RD_COUNTRY_PACK,
  LANE8_RD_PAY_GROUP_FACTS,
  LANE8_RD_STATUTORY_RUN,
  LANE8_RD_STATUTORY_ITEMS,
  LANE8_RD_RETAINED_PACK,
  LANE8_RD_HR_ENGAGEMENT,
  LANE8_RD_HR_ELIGIBILITY,
  LANE8_RD_HR_AFFECTED_PEOPLE,
  LANE8_RD_HR_FUTURE_SHIFTS,
  LANE8_RD_HR_CURRENT_POLICIES,
  LANE8_RD_HR_PAY_GROUPS,
  lane8Configuration,
  nativeLane8Configured,
  readApproverName,
  readExportBytes,
  readHireHandoffPayGroups,
  readHireHandoffPeople,
  readHrAffectedPeople,
  readHrCurrentPolicies,
  readHrEligibility,
  readHrEngagement,
  readHrFutureShifts,
  readHrPayGroups,
  readIssuableItems,
  readIssuableRun,
  readOwnSlips,
  readPayment,
  readRetainedPack,
  readRosterShift,
  readSlip,
  readStatutoryCountryPack,
  readStatutoryItems,
  readStatutoryPayGroup,
  readStatutoryRun,
  type Lane8Actor,
} from "../../../modules/people-payroll/server/src/adapters/native-lane-8/client";

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
const issuableRun = {
  id: "run", input_digest: "d", revision: 1, version: 2, mode: "production",
  period_start: "2026-11-01", period_end: "2026-11-30", pay_date: "2026-11-30", currency: "THB",
};
const issuableItems = [{
  id: "item", engagement_id: "eng", person_id: "person", recipient_user_id: null,
  input_snapshot_json: "{}", result_json: "{}", gross_minor: 1000, deductions_minor: 10,
  employer_cost_minor: 5, net_minor: 990, currency: "THB",
}];
const ownSlips = [{
  id: "slip", run_id: "run", run_revision: 1, pay_date: "2026-11-30", currency: "THB",
  gross_minor: 1000, deductions_minor: 10, net_minor: 990, source_digest: "s",
  export_sha256: "e", issued_at: 100, snapshot_json: "{}",
}];
const slip = {
  id: "slip", run_id: "run", person_id: "person", recipient_user_id: null,
  snapshot_json: "{}", export_html: "<p>x</p>", export_sha256: "e", issued_at: 100,
};
const payment = { status: "settled", settled_at: null, batch_status: "settled" };
const exportBytes = { export_html: "<p>x</p>", export_sha256: "e" };
const countryPack = {
  id: "pack", country_code: "TH", version: 1, status: "active", rules_json: "{}",
  adapter_id: "thai", source_digest: "s", record_version: 1,
};
const statutoryGroup = { country_code: "TH", currency: "THB", frequency: "monthly" };
const statutoryRun = { mode: "production", country_pack_id: null, currency: "THB" };
const statutoryItems = [{ gross_minor: 1000, deductions_minor: 10, net_minor: 990, result_json: "{}" }];
const retainedPack = { ...countryPack, status: "superseded", record_version: 2 };

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

  it("reads payslip source facts with pinned result oracles", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(issuableRun)));
    expect(await readIssuableRun(db, "run", "company-acme")).toEqual(issuableRun);
    expect(digest(issuableRun)).toBe("c740a1993671aa8a82c6e3e67cc9de49d907012e00878d141bcb7effe36606c9");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_ISSUABLE_RUN, values: ["run"],
    });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(issuableItems)));
    expect((await readIssuableItems(db, "company-acme", "run")).results).toEqual(issuableItems);
    expect(digest(issuableItems)).toBe("e429224f435614c083e792bb65d850ac7cc3b495f4dc43424debd45e9671f585");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_ISSUABLE_ITEMS, values: ["run"],
    });
  });

  it("preserves broad and user-scoped payslip filter arity and bind order", async () => {
    vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(ok(ownSlips))));
    expect((await readOwnSlips(db, "company-acme", undefined, undefined)).results).toEqual(ownSlips);
    expect(digest(ownSlips)).toBe("010f9a605bd304837de5533ed221bc2ec8a482baf01a43fe4b9b4c5fac543927");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_OWN_SLIPS, values: ["", "", ""],
    });
    await readOwnSlips(db, "company-acme", "run", "user");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[1][1]?.body))).toEqual({
      kind: LANE8_RD_OWN_SLIPS, values: ["run", "user", "user"],
    });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(slip)));
    expect(await readSlip(db, "slip", "company-acme", undefined)).toEqual(slip);
    expect(digest(slip)).toBe("1f280cbc458f237097109645d5e7758c35e05e02bd95566d172990dffaa368fd");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_SLIP, values: ["slip", "", ""],
    });
  });

  it("reads payment and export bytes with pinned result oracles", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(payment)));
    expect(await readPayment(db, "company-acme", "run", "person")).toEqual(payment);
    expect(digest(payment)).toBe("6c04de5a525f186a4e17afb724a82f9a8e93237698b34f2ae4a36f8cb052754b");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_PAYMENT, values: ["run", "person"],
    });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(exportBytes)));
    expect(await readExportBytes(db, "slip", "company-acme")).toEqual(exportBytes);
    expect(digest(exportBytes)).toBe("cbc0927106c063723f699388bb76e08db7ce5e7cfe84b493efcd457f50d3fd24");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_EXPORT_BYTES, values: ["slip"],
    });
  });

  it("fails closed on malformed payslip rows and null single-row reads", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok([{ ...ownSlips[0], extra: "field" }])));
    await expect(readOwnSlips(db, "company-acme", undefined, undefined)).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok({ ...issuableRun, revision: "1" })));
    await expect(readIssuableRun(db, "run", "company-acme")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE",
    });
    vi.stubGlobal("fetch", vi.fn().mockImplementation(() => Promise.resolve(ok(null))));
    expect(await readIssuableRun(db, "run", "company-acme")).toBeNull();
    expect(await readSlip(db, "slip", "company-acme", "user")).toBeNull();
    expect(await readPayment(db, "company-acme", "run", "person")).toBeNull();
    expect(await readExportBytes(db, "slip", "company-acme")).toBeNull();
  });

  it("reads Thai statutory country pack and active pay group with pinned oracles", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(countryPack)));
    expect(await readStatutoryCountryPack(db, "pack")).toEqual(countryPack);
    expect(digest(countryPack)).toBe("13bf5d02520371cf1f0673103675b0cde636797ad33e6919bf2ebc92ee4578fc");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_COUNTRY_PACK, values: ["pack"],
    });
    expect(headersOf(0)["x-erpx-context-company"]).toBe("company-acme");

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(statutoryGroup)));
    expect(await readStatutoryPayGroup(db, "company-acme", "group")).toEqual(statutoryGroup);
    expect(digest(statutoryGroup)).toBe("8ff624e68f15d4ee1659cd6173b0d02d6fb20d42651a9c2fccd8d391fc37c8e6");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_PAY_GROUP_FACTS, values: ["group"],
    });
  });

  it("reads statutory run, item totals and retained pack with pinned oracles", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(statutoryRun)));
    expect(await readStatutoryRun(db, "run")).toEqual(statutoryRun);
    expect(digest(statutoryRun)).toBe("ccd4213201881bd66e00e2f464076e19cf86afc731f73efa6f7c386288709b40");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_STATUTORY_RUN, values: ["run"],
    });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(statutoryItems)));
    expect((await readStatutoryItems(db, "run")).results).toEqual(statutoryItems);
    expect(digest(statutoryItems)).toBe("450526b45b05d32bec54f5b3319355fe18ae1339e1b6ecfdfe109922957a18d2");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_STATUTORY_ITEMS, values: ["run"],
    });

    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(retainedPack)));
    expect(await readRetainedPack(db, "pack")).toEqual(retainedPack);
    expect(digest(retainedPack)).toBe("a45986148d4c72195ada0153c180d5f0c06bc5ab4f7de5be7720e7cada91df27");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_RETAINED_PACK, values: ["pack"],
    });
  });

  it("fails closed on malformed statutory rows and delegates target selection", async () => {
    incomingHeaders.value = new Headers({ host: "erpx-internal.jesadakorn.com" });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok({ ...countryPack, version: "1" })));
    await expect(readStatutoryCountryPack(db, "pack")).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(countryPack)));
    await readStatutoryCountryPack(db, "pack", actor("h003", "erpx", "company-erpx"));
    expect(urlOf(0).toString()).toBe("http://127.0.0.1:44994/api/v1/holdings/erpx/people-payroll/app-entrypoint-read");
    expect(headersOf(0).host).toBe("erpx-internal.jesadakorn.com");
    await expect(readStatutoryCountryPack(db, "pack", actor("h999", "erpx", "company-erpx")))
      .rejects.toMatchObject({ code: "WORK_CONTEXT_CHANGED", status: 409 });
  });

  it("reads HR settings engagement and overtime eligibility facts with pinned oracles", async () => {
    const hrEngagement = { id: "eng", person_id: "person", branch_id: "branch", department_id: null };
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(hrEngagement)));
    expect(await readHrEngagement(db, "company-acme", "eng", "branch", null)).toEqual(hrEngagement);
    expect(digest(hrEngagement)).toBe("9bf0416be778b11ccb7e3e9c5d062d66e83818660f9f899b8056b68bb4e8f6d2");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_ENGAGEMENT, values: ["eng", "branch", "branch", "", ""],
    });
    expect(headersOf(0)["x-erpx-context-company"]).toBe("company-acme");

    const eligibility = { eligible: true, version: 2, top_level: true };
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(eligibility)));
    expect(await readHrEligibility(db, "company-acme", "eng")).toEqual(eligibility);
    expect(digest(eligibility)).toBe("41d4b7ac53465eb272dd1faf886b9d796d03ce9a6223b6d2ef1d18435e6231de");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_ELIGIBILITY, values: ["eng"],
    });
  });

  it("reads affected people, shifts, policies and pay groups with pinned oracles", async () => {
    const affected = [{ engagement_id: "eng", person_id: "person", name: "Alice" }];
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(affected)));
    expect((await readHrAffectedPeople(db, "company-acme", ["eng"], "branch", null)).results).toEqual(affected);
    expect(digest(affected)).toBe("1ed13dbf367c71ac1b467d1a6308fdc5837d2e2851c9192d04e4ad17c3e772ee");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_AFFECTED_PEOPLE, values: ['["eng"]', "branch", "branch", "", ""],
    });

    const shifts = [{ engagement_id: "eng", total: 3 }];
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(shifts)));
    expect((await readHrFutureShifts(db, "company-acme", ["eng"], 1774483200000)).results).toEqual(shifts);
    expect(digest(shifts)).toBe("2d7b04dabb5c245c3d264af9ba8879c9de6b9d5de47d9b7bd23766af4d27d656");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_FUTURE_SHIFTS, values: ['["eng"]', "1774483200000"],
    });

    const policies = [{ engagement_id: "eng", policy: "Annual", calendar: null }];
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(policies)));
    expect((await readHrCurrentPolicies(db, "company-acme", ["eng"], "2026-11-25")).results).toEqual(policies);
    expect(digest(policies)).toBe("affb3859275ec02af057bf965e907829ac54018d6850e61b8032e96fab06288a");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_CURRENT_POLICIES, values: ['["eng"]', "2026-11-25", "2026-11-25"],
    });

    const groups = [{ engagement_id: "eng", name: "Monthly" }];
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok(groups)));
    expect((await readHrPayGroups(db, "company-acme", ["eng"])).results).toEqual(groups);
    expect(digest(groups)).toBe("3bbb0ab1b52e6a219ffce5d661e665b3eb294df6ffa6a7fd0f0762b63210c6b7");
    expect(JSON.parse(String(vi.mocked(fetch).mock.calls[0][1]?.body))).toEqual({
      kind: LANE8_RD_HR_PAY_GROUPS, values: ['["eng"]'],
    });
  });

  it("fails closed on malformed HR rows and bounded filters", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok({ id: "eng", person_id: "person", branch_id: 1, department_id: null })));
    await expect(readHrEngagement(db, "company-acme", "eng", null, null)).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(ok({ id: "eng", person_id: "person", branch_id: null, department_id: null })));
    await expect(readHrEngagement(db, "company-acme", "eng", "../other", null)).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    await expect(readHrPayGroups(db, "company-acme", [])).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
    await expect(readHrFutureShifts(db, "company-acme", ["eng"], 1.5)).rejects.toMatchObject({
      code: "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", status: 503,
    });
  });
});
