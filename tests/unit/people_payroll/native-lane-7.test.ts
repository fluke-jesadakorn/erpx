import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
vi.mock("server-only", () => ({}));
import {
  AUTHORITY_ACTOR_SQL_ORACLE_SHA256,
  AUTHORITY_APPOINTEE_SQL_ORACLE_SHA256,
  AUTHORITY_RESPONSE_ORACLE_SHA256,
  nativeLane7Configured,
  PAYMENTS_RESPONSE_ORACLE_SHA256,
  PAYMENTS_SQL_ORACLE_SHA256,
  readFinanceWorkspacePeoplePayments,
  readLegacyPeopleTransition,
  readOrganizationAuthorityPerson,
  RESPONSE_ORACLE_SHA256,
  SQL_ORACLE_SHA256,
  type AuthorityQuery,
  type Lane7Actor,
  type Transport,
} from "../../../packages/server-people_payroll/src/adapters/native-lane-7/client";

const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG";
const ORIGINAL = process.env[ENV];
const KIND = "people_payroll_lane_7_legacy_people_transition";
const config = (port = 3198, holdings?: unknown) =>
  JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    timeoutMs: 10_000,
    holdings:
      holdings ??
      [
        { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme", companyId: "acme-co" },
        { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar", companyId: "northstar-co" },
        { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx", companyId: "erpx-co" },
      ],
  });
const actor = (holdingId: string, holdingSlug: string, companyId: string): Lane7Actor => ({
  holdingId,
  holdingSlug,
  companyId,
});

let calls: { url: URL; headers: Headers }[];
const payload = (deadline: number | null) => ({ kind: KIND, transition: { deadline } });
const transport = (body: unknown, status = 200): Transport => {
  return async (url, headers) => {
    calls.push({ url, headers });
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  };
};

beforeEach(() => {
  process.env[ENV] = config();
  calls = [];
});
afterEach(() => {
  if (ORIGINAL === undefined) delete process.env[ENV];
  else process.env[ENV] = ORIGINAL;
});

describe("native lane-7 multi-holding clock transport", () => {
  it("selects the exact Holding from the caller's WorkContext", async () => {
    expect(
      await readLegacyPeopleTransition(
        "northstar-co",
        100,
        actor("h002", "northstar", "northstar-co"),
        transport(payload(1_764_010_200_000)),
      ),
    ).toBe(1_764_010_200_000);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.origin).toBe("http://127.0.0.1:3198");
    expect(calls[0]!.url.pathname).toBe(
      "/api/v1/holdings/northstar/people-payroll/legacy-people-clock",
    );
    expect(calls[0]!.headers.get("host")).toBe("northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h002");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("northstar-co");
    expect(calls[0]!.headers.get("x-erpx-observed-at")).toBe("100");
    expect(calls[0]!.headers.get("content-type")).toBeNull();
  });

  it("routes the third registered Holding to its own host and pool route", async () => {
    expect(
      await readLegacyPeopleTransition(
        "erpx-co",
        0,
        actor("h003", "erpx", "erpx-co"),
        transport(payload(null)),
      ),
    ).toBeNull();
    expect(calls[0]!.url.pathname).toBe("/api/v1/holdings/erpx/people-payroll/legacy-people-clock");
    expect(calls[0]!.headers.get("host")).toBe("erpx-internal.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h003");
  });

  it("resolves callers without a WorkContext only through a configured companyId", async () => {
    expect(await readLegacyPeopleTransition("acme-co", 0, undefined, transport(payload(null)))).toBeNull();
    expect(calls[0]!.headers.get("host")).toBe("acme.jesadakorn.com");
    expect(calls[0]!.url.pathname).toBe("/api/v1/holdings/acme/people-payroll/legacy-people-clock");
    await expect(
      readLegacyPeopleTransition("unknown-co", 0, undefined, transport(payload(null))),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE",
    });
    expect(calls).toHaveLength(1);
  });

  it("fails closed on an unknown WorkContext target or mismatched company", async () => {
    await expect(
      readLegacyPeopleTransition("acme-co", 0, actor("h099", "acme", "acme-co"), transport(payload(null))),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE",
    });
    await expect(
      readLegacyPeopleTransition("acme-co", 0, actor("h001", "northstar", "acme-co"), transport(payload(null))),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE",
    });
    await expect(
      readLegacyPeopleTransition("acme-co", 0, actor("h001", "acme", "northstar-co"), transport(payload(null))),
    ).rejects.toMatchObject({ status: 422, code: "NATIVE_PEOPLE_PAYROLL_LANE7_ACTOR_INVALID" });
    expect(calls).toHaveLength(0);
  });

  it("pins the SQL and rendered-result oracles without carrying SQL on the wire", async () => {
    expect(nativeLane7Configured()).toBe(true);
    expect(SQL_ORACLE_SHA256).toBe(
      "79ad85c3c72c22e111f812e45e22068608eeb21fb5e000c23beaa6ec162930f2",
    );
    expect(RESPONSE_ORACLE_SHA256).toBe(
      "b9d5a6988cbe97e37f2c13ff946ff430253e78d54d4ded15907db887ac65b98f",
    );
    const response = JSON.stringify(payload(1_764_010_200_000));
    expect(createHash("sha256").update(response).digest("hex")).toBe(RESPONSE_ORACLE_SHA256);
    await readLegacyPeopleTransition(
      "acme-co",
      0,
      actor("h001", "acme", "acme-co"),
      transport(payload(1_764_010_200_000)),
    );
    const wire = JSON.stringify(calls);
    expect(wire).not.toMatch(/SELECT|UNION|FROM people_|boundaries/i);
  });

  it("fails closed when configuration is missing or malformed", async () => {
    delete process.env[ENV];
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, transport(payload(null))),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_REQUIRED" });
    process.env[ENV] = config(3100);
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, transport(payload(null))),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID" });
    process.env[ENV] = config(3198, []);
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, transport(payload(null))),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID" });
    process.env[ENV] = config(
      3198,
      Array.from({ length: 65 }, (_, i) => ({
        origin: `https://h${i}.jesadakorn.com`,
        host: `h${i}.jesadakorn.com`,
        holdingId: `h${i}`,
        holdingSlug: `h${i}`,
      })),
    );
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, transport(payload(null))),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID" });
  });

  it("fails closed when the native service is unreachable or redirected", async () => {
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, async () => {
        throw new Error("offline");
      }),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE" });
    const redirected: Transport = async () => ({ redirected: true, status: 200 }) as Response;
    await expect(
      readLegacyPeopleTransition("acme-co", 0, undefined, redirected),
    ).rejects.toMatchObject({ status: 503 });
  });

  it("fails closed on a malformed or widened response shape", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(readLegacyPeopleTransition("acme-co", 0, a, transport({ kind: KIND }))).rejects.toMatchObject({
      status: 502,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID",
    });
    await expect(
      readLegacyPeopleTransition("acme-co", 0, a, transport({ ...payload(null), extra: 1 })),
    ).rejects.toMatchObject({ status: 502 });
    await expect(
      readLegacyPeopleTransition("acme-co", 0, a, transport({ ...payload(null), transition: { deadline: -1 } })),
    ).rejects.toMatchObject({ status: 502 });
    await expect(
      readLegacyPeopleTransition("acme-co", 0, a, transport({ ...payload(null), transition: { deadline: "1" } })),
    ).rejects.toMatchObject({ status: 502 });
  });

  it("maps native 4xx and 5xx without inventing a fallback", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(readLegacyPeopleTransition("acme-co", 0, a, transport({}, 422))).rejects.toMatchObject({
      status: 422,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_REJECTED",
    });
    await expect(readLegacyPeopleTransition("acme-co", 0, a, transport({}, 500))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE",
    });
  });

  it("rejects unbounded Company and clock input before any request", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(readLegacyPeopleTransition("../other", 0, a, transport(payload(null)))).rejects.toMatchObject({
      status: 422,
    });
    await expect(readLegacyPeopleTransition("acme-co", -1, a, transport(payload(null)))).rejects.toMatchObject({
      status: 422,
    });
    expect(calls).toHaveLength(0);
  });
});

describe("native lane-7 Finance workspace people-payments transport", () => {
  const row = {
    id: "batch-1",
    source_type: "payroll",
    source_id: "run-1",
    currency: "THB",
    status: "approved",
    due_date: "2026-11-25",
    amount_minor: 1500,
  };
  const payments = (rows: unknown[]) => ({
    kind: "people_payroll_lane_7_workspace_people_payments",
    payments: rows,
  });

  it("returns the exact owner-projected rows for the selected Holding", async () => {
    const result = await readFinanceWorkspacePeoplePayments(
      "northstar-co",
      actor("h002", "northstar", "northstar-co"),
      transport(payments([row])),
    );
    expect(result).toEqual([row]);
    expect(calls[0]!.url.pathname).toBe(
      "/api/v1/holdings/northstar/people-payroll/finance-workspace-payments",
    );
    expect(calls[0]!.headers.get("host")).toBe("northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h002");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("northstar-co");
    expect(calls[0]!.headers.get("x-erpx-observed-at")).toBeNull();
  });

  it("pins the payments SQL and result oracles without SQL on the wire", async () => {
    expect(PAYMENTS_SQL_ORACLE_SHA256).toBe(
      "97047147e5dc9782b5de8e4c0659cb6c693f45a398186acb19ddb1f2b0200c2e",
    );
    expect(PAYMENTS_RESPONSE_ORACLE_SHA256).toBe(
      "991dbb8da772de20366554766db436b89235fad81b537272fdf54702bb348f2e",
    );
    const wire = JSON.stringify(payments([row]));
    expect(createHash("sha256").update(wire).digest("hex")).toBe(PAYMENTS_RESPONSE_ORACLE_SHA256);
    await readFinanceWorkspacePeoplePayments("acme-co", undefined, transport(payments([])));
    expect(JSON.stringify(calls)).not.toMatch(/SELECT|hr_payment_batches|GROUP BY/i);
  });

  it("fails closed on unknown targets and malformed rows", async () => {
    await expect(
      readFinanceWorkspacePeoplePayments(
        "acme-co",
        actor("h099", "acme", "acme-co"),
        transport(payments([])),
      ),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE",
    });
    expect(calls).toHaveLength(0);
    await expect(
      readFinanceWorkspacePeoplePayments("acme-co", undefined, transport(payments([{ ...row, amount_minor: -1 }]))),
    ).rejects.toMatchObject({ status: 502, code: "NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID" });
    await expect(
      readFinanceWorkspacePeoplePayments("acme-co", undefined, transport(payments([{ ...row, extra: 1 }]))),
    ).rejects.toMatchObject({ status: 502 });
    await expect(
      readFinanceWorkspacePeoplePayments("acme-co", undefined, transport(payments([row]), 500)),
    ).rejects.toMatchObject({ status: 503 });
    expect(calls).toHaveLength(3);
  });
});

describe("native lane-7 Organization authority person transport", () => {
  const KIND = "people_payroll_lane_7_organization_authority_person";
  const person = (id: string | number | null) => ({ kind: KIND, person: { id } });
  const actorQuery: AuthorityQuery = { kind: "actor", userId: "usr_1" };
  const appointeeQuery: AuthorityQuery = {
    kind: "appointee",
    personId: "per_1",
    positionId: "pos_1",
    appointmentKind: "head",
    scopeId: "dept_1",
    selfCeo: true,
  };

  it("routes the actor lookup to the selected Holding with a discriminator query", async () => {
    expect(
      await readOrganizationAuthorityPerson(
        "northstar-co",
        actorQuery,
        actor("h002", "northstar", "northstar-co"),
        transport(person("usr_1")),
      ),
    ).toBe("usr_1");
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.pathname).toBe(
      "/api/v1/holdings/northstar/people-payroll/organization-authority-person",
    );
    expect(calls[0]!.url.searchParams.get("kind")).toBe("actor");
    expect(calls[0]!.url.searchParams.get("user_id")).toBe("usr_1");
    expect(calls[0]!.url.searchParams.has("person_id")).toBe(false);
    expect(calls[0]!.headers.get("host")).toBe("northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h002");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("northstar-co");
  });

  it("sends every bounded appointee field and accepts a null id", async () => {
    expect(
      await readOrganizationAuthorityPerson("acme-co", appointeeQuery, undefined, transport(person(null))),
    ).toBeNull();
    const params = calls[0]!.url.searchParams;
    expect(params.get("kind")).toBe("appointee");
    expect(params.get("person_id")).toBe("per_1");
    expect(params.get("position_id")).toBe("pos_1");
    expect(params.get("appointment_kind")).toBe("head");
    expect(params.get("scope_id")).toBe("dept_1");
    expect(params.get("self_ceo")).toBe("true");
  });

  it("pins both owner SQL oracles and the rendered-result oracle", async () => {
    expect(AUTHORITY_ACTOR_SQL_ORACLE_SHA256).toBe(
      "c5264c5c857254afc0d7f10e7627a513e05a9d3aea5c250e08858d6b759a0840",
    );
    expect(AUTHORITY_APPOINTEE_SQL_ORACLE_SHA256).toBe(
      "f579db98f3ccd908d63b13ef6c64fbb39096f7a8864cf2598dfd78bff8a5face",
    );
    expect(AUTHORITY_RESPONSE_ORACLE_SHA256).toBe(
      "e8888517d3fb5b2ba2f6b6c3de973ed39168d86f1efa8b80e7b594e30ec1c3b9",
    );
    expect(createHash("sha256").update(JSON.stringify(person("usr_1"))).digest("hex")).toBe(
      AUTHORITY_RESPONSE_ORACLE_SHA256,
    );
    await readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, transport(person(null)));
    expect(JSON.stringify(calls)).not.toMatch(/SELECT|hr_people|hr_engagements|iam_same_person/i);
  });

  it("fails closed on a malformed payload, query or target", async () => {
    await expect(
      readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, transport({ ...person("usr_1"), extra: 1 })),
    ).rejects.toMatchObject({ status: 502, code: "NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID" });
    await expect(
      readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, transport(person(12))),
    ).rejects.toMatchObject({ status: 502 });
    await expect(
      readOrganizationAuthorityPerson("../other", actorQuery, undefined, transport(person(null))),
    ).rejects.toMatchObject({ status: 422, code: "PEOPLE_PAYROLL_LANE7_COMPANY_INVALID" });
    await expect(
      readOrganizationAuthorityPerson(
        "acme-co",
        { kind: "actor", userId: "../bad" },
        undefined,
        transport(person(null)),
      ),
    ).rejects.toMatchObject({ status: 422, code: "PEOPLE_PAYROLL_LANE7_AUTHORITY_QUERY_INVALID" });
    await expect(
      readOrganizationAuthorityPerson(
        "acme-co",
        actorQuery,
        actor("h099", "acme", "acme-co"),
        transport(person(null)),
      ),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE" });
    expect(calls).toHaveLength(2);
  });

  it("maps native failures and unreachable transports without a fallback", async () => {
    await expect(
      readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, transport({}, 422)),
    ).rejects.toMatchObject({ status: 422, code: "NATIVE_PEOPLE_PAYROLL_LANE7_REJECTED" });
    await expect(
      readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, transport({}, 500)),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE" });
    await expect(
      readOrganizationAuthorityPerson("acme-co", actorQuery, undefined, async () => {
        throw new Error("offline");
      }),
    ).rejects.toMatchObject({ status: 503 });
    expect(calls).toHaveLength(2);
  });
});
