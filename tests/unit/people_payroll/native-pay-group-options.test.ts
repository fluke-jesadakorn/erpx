import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("server-only", () => ({}));
import {
  nativePayGroupOptions,
  RESPONSE_ORACLE_SHA256,
  SQL_ORACLE_SHA256,
  type Lane9Actor,
  type Transport,
} from "../../../packages/server-people_payroll/src/adapters/native-pay-group-options";

const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG";
const ORIGINAL = process.env[ENV];
const config = (port = 3198, holdings?: unknown) =>
  JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    timeoutMs: 10_000,
    holdings:
      holdings ??
      [
        { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme", companyId: "acme-co" },
        { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar", companyId: "northstar-co" },
      ],
  });
const actor = (holdingId: string, holdingSlug: string, companyId: string): Lane9Actor => ({
  holdingId,
  holdingSlug,
  companyId,
});
const RESPONSE = { groups: [{ id: "pg-1", name: "Monthly", currency: "THB", frequency: "monthly" }] };

let calls: { url: URL; headers: Headers }[];
const transport = (body: unknown, status = 200): Transport => {
  return async (url, headers) => {
    calls.push({ url, headers });
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  };
};
const db = {} as Parameters<typeof nativePayGroupOptions>[0];

beforeEach(() => {
  process.env[ENV] = config();
  calls = [];
});
afterEach(() => {
  if (ORIGINAL === undefined) delete process.env[ENV];
  else process.env[ENV] = ORIGINAL;
});

describe("native active pay-group options transport", () => {
  it("selects the exact Holding from the caller's WorkContext", async () => {
    expect(
      await nativePayGroupOptions(db, "northstar-co", actor("h002", "northstar", "northstar-co"), transport(RESPONSE)),
    ).toEqual(RESPONSE.groups);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.origin).toBe("http://127.0.0.1:3198");
    expect(calls[0]!.url.pathname).toBe("/api/v1/holdings/northstar/people-payroll/pay-group-options");
    expect(calls[0]!.headers.get("host")).toBe("northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("origin")).toBe("https://northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("northstar-co");
  });

  it("resolves a caller without a WorkContext only through a configured companyId", async () => {
    expect(await nativePayGroupOptions(db, "acme-co", undefined, transport(RESPONSE))).toEqual(
      RESPONSE.groups,
    );
    expect(calls[0]!.headers.get("host")).toBe("acme.jesadakorn.com");
    await expect(
      nativePayGroupOptions(db, "unknown-co", undefined, transport(RESPONSE)),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE",
    });
    expect(calls).toHaveLength(1);
  });

  it("fails closed on an unknown WorkContext target or mismatched company", async () => {
    await expect(
      nativePayGroupOptions(db, "acme-co", actor("h099", "acme", "acme-co"), transport(RESPONSE)),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE" });
    await expect(
      nativePayGroupOptions(db, "acme-co", actor("h001", "acme", "northstar-co"), transport(RESPONSE)),
    ).rejects.toMatchObject({ status: 422, code: "NATIVE_PEOPLE_PAYROLL_LANE9_ACTOR_INVALID" });
    expect(calls).toHaveLength(0);
  });

  it("pins SQL and rendered-result oracles without carrying SQL on the wire", () => {
    expect(SQL_ORACLE_SHA256).toBe(
      "d53d068a7d9d7a7a5f1ccaf34026302bf253cfb15128adc8c7cd6d9aa5aeda43",
    );
    expect(RESPONSE_ORACLE_SHA256).toBe(
      "dce62e6f1ebac21b311cd31edd02c69b8e6f2c8e62d0d7104718a378e692a29d",
    );
    const response = JSON.stringify(RESPONSE);
    expect(createHash("sha256").update(response).digest("hex")).toBe(RESPONSE_ORACLE_SHA256);
    const sql = "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND status='active' ORDER BY name";
    expect(createHash("sha256").update(sql).digest("hex")).toBe(SQL_ORACLE_SHA256);
  });

  it("never emits SQL or SQL fragments over the wire", async () => {
    await nativePayGroupOptions(db, "acme-co", actor("h001", "acme", "acme-co"), transport(RESPONSE));
    expect(JSON.stringify(calls)).not.toMatch(/SELECT|UNION|FROM |hr_pay_groups/i);
  });

  it("rejects unexpected payload shapes and maps upstream failures closed", async () => {
    await expect(
      nativePayGroupOptions(db, "acme-co", undefined, transport({ groups: [{ id: "pg-1" }] })),
    ).rejects.toMatchObject({ status: 502, code: "NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID" });
    await expect(
      nativePayGroupOptions(db, "acme-co", undefined, transport({ rows: [] })),
    ).rejects.toMatchObject({ status: 502, code: "NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID" });
    await expect(
      nativePayGroupOptions(db, "acme-co", undefined, transport({}, 503)),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_PGO_UNAVAILABLE" });
    await expect(
      nativePayGroupOptions(db, "acme-co", undefined, transport({}, 422)),
    ).rejects.toMatchObject({ status: 422, code: "NATIVE_PEOPLE_PAYROLL_PGO_REJECTED" });
  });

  it("the transport source carries no SQL literal", () => {
    const source = readFileSync(
      resolve(__dirname, "../../../packages/server-people_payroll/src/adapters/native-pay-group-options.ts"),
      "utf8",
    );
    expect(source).not.toMatch(/\bSELECT\b|hr_pay_groups|status='active'/i);
    expect(source).toContain("native-lane-9/client");
  });
});
