import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("server-only", () => ({}));
import {
  nativeLane9Configured,
  personImage,
  RESPONSE_ORACLE_SHA256,
  SQL_ORACLE_SHA256,
  type Lane9Actor,
  type Transport,
} from "../../../packages/server-people_payroll/src/adapters/native-lane-9/client";

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
        { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx", companyId: "erpx-co" },
      ],
  });
const actor = (holdingId: string, holdingSlug: string, companyId: string): Lane9Actor => ({
  holdingId,
  holdingSlug,
  companyId,
});
const IMAGE = JSON.stringify({
  person: { id: "person-1" },
  engagements: [],
  employees: [],
  cases: [],
  tasks: [],
  compensation: [],
  leave: [],
});
const payload = (image: string | null = IMAGE) => ({ personImage: image });

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
const db = {} as Parameters<typeof personImage>[0];

beforeEach(() => {
  process.env[ENV] = config();
  calls = [];
});
afterEach(() => {
  if (ORIGINAL === undefined) delete process.env[ENV];
  else process.env[ENV] = ORIGINAL;
});

describe("native lane-9 multi-holding person-image transport", () => {
  it("selects the exact Holding from the caller's WorkContext", async () => {
    expect(
      await personImage(db, "northstar-co", "person-1", actor("h002", "northstar", "northstar-co"), transport(payload())),
    ).toBe(IMAGE);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.origin).toBe("http://127.0.0.1:3198");
    expect(calls[0]!.url.pathname).toBe("/api/v1/holdings/northstar/people-payroll/person-image");
    expect(calls[0]!.headers.get("host")).toBe("northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("origin")).toBe("https://northstar.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h002");
    expect(calls[0]!.headers.get("x-erpx-context-company")).toBe("northstar-co");
    expect(calls[0]!.headers.get("x-erpx-context-person")).toBe("person-1");
    expect(calls[0]!.headers.get("content-type")).toBeNull();
  });

  it("routes the third registered Holding to its own host and route", async () => {
    expect(
      await personImage(db, "erpx-co", "person-1", actor("h003", "erpx", "erpx-co"), transport(payload())),
    ).toBe(IMAGE);
    expect(calls[0]!.url.pathname).toBe("/api/v1/holdings/erpx/people-payroll/person-image");
    expect(calls[0]!.headers.get("host")).toBe("erpx-internal.jesadakorn.com");
    expect(calls[0]!.headers.get("x-erpx-context-holding")).toBe("h003");
  });

  it("resolves a caller without a WorkContext only through a configured companyId", async () => {
    expect(await personImage(db, "acme-co", "person-1", undefined, transport(payload()))).toBe(IMAGE);
    expect(calls[0]!.headers.get("host")).toBe("acme.jesadakorn.com");
    await expect(
      personImage(db, "unknown-co", "person-1", undefined, transport(payload())),
    ).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE",
    });
    expect(calls).toHaveLength(1);
  });

  it("fails closed on an unknown WorkContext target or mismatched company", async () => {
    await expect(
      personImage(db, "acme-co", "person-1", actor("h099", "acme", "acme-co"), transport(payload())),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE" });
    await expect(
      personImage(db, "acme-co", "person-1", actor("h001", "northstar", "acme-co"), transport(payload())),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE" });
    await expect(
      personImage(db, "acme-co", "person-1", actor("h001", "acme", "northstar-co"), transport(payload())),
    ).rejects.toMatchObject({ status: 422, code: "NATIVE_PEOPLE_PAYROLL_LANE9_ACTOR_INVALID" });
    expect(calls).toHaveLength(0);
  });

  it("pins SQL and rendered-result oracles without carrying SQL on the wire", () => {
    expect(nativeLane9Configured()).toBe(true);
    expect(SQL_ORACLE_SHA256).toBe(
      "944d4be0677435e95c0f3fe456038c9f217c5ec2c4990673318d7bc9901b64e0",
    );
    expect(RESPONSE_ORACLE_SHA256).toBe(
      "5ef5ac117da385b5f86d249033449b7fff1483f9317ef0378804a993b595b81c",
    );
    const response = JSON.stringify(payload());
    expect(createHash("sha256").update(response).digest("hex")).toBe(RESPONSE_ORACLE_SHA256);
  });

  it("never emits SQL or SQL fragments over the wire", async () => {
    await personImage(db, "acme-co", "person-1", actor("h001", "acme", "acme-co"), transport(payload()));
    const wire = JSON.stringify(calls);
    expect(wire).not.toMatch(/SELECT|UNION|FROM |jsonb_build_object|hr_people/i);
  });

  it("replaced the TypeScript SQL literal completely", () => {
    const source = readFileSync(
      resolve(__dirname, "../../../packages/server-people_payroll/src/adapters/project-worker-startup-state.ts"),
      "utf8",
    );
    expect(source).not.toMatch(/jsonb_build_object/);
    expect(source).not.toMatch(/leave_policy_assignments/);
    expect(source).toContain("native-lane-9/client");
  });

  it("fails closed when configuration is missing or malformed", async () => {
    const a = actor("h001", "acme", "acme-co");
    delete process.env[ENV];
    await expect(personImage(db, "acme-co", "person-1", a, transport(payload()))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_REQUIRED",
    });
    process.env[ENV] = config(3100);
    await expect(personImage(db, "acme-co", "person-1", a, transport(payload()))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID",
    });
    process.env[ENV] = config(3198, []);
    await expect(personImage(db, "acme-co", "person-1", a, transport(payload()))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID",
    });
    process.env[ENV] = config(
      3198,
      Array.from({ length: 65 }, (_, i) => ({
        origin: `https://h${i}.jesadakorn.com`,
        host: `h${i}.jesadakorn.com`,
        holdingId: `h${i}`,
        holdingSlug: `h${i}`,
      })),
    );
    await expect(personImage(db, "acme-co", "person-1", a, transport(payload()))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID",
    });
  });

  it("fails closed when the native service is unreachable or redirected", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(
      personImage(db, "acme-co", "person-1", a, async () => {
        throw new Error("offline");
      }),
    ).rejects.toMatchObject({ status: 503, code: "NATIVE_PEOPLE_PAYROLL_LANE9_UNAVAILABLE" });
    const redirected: Transport = async () => ({ redirected: true, status: 200 }) as Response;
    await expect(personImage(db, "acme-co", "person-1", a, redirected)).rejects.toMatchObject({ status: 503 });
  });

  it("fails closed on a malformed or widened response shape", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(personImage(db, "acme-co", "person-1", a, transport({}))).rejects.toMatchObject({
      status: 502,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_RESPONSE_INVALID",
    });
    await expect(personImage(db, "acme-co", "person-1", a, transport({ ...payload(), extra: 1 }))).rejects.toMatchObject({
      status: 502,
    });
    await expect(personImage(db, "acme-co", "person-1", a, transport(payload(null)))).rejects.toMatchObject({
      status: 502,
    });
    const widened = JSON.parse(IMAGE);
    widened.extra = "not-owner";
    await expect(
      personImage(db, "acme-co", "person-1", a, transport(payload(JSON.stringify(widened)))),
    ).rejects.toMatchObject({ status: 502 });
    await expect(
      personImage(db, "acme-co", "person-1", a, transport(payload("{not-json"))),
    ).rejects.toMatchObject({ status: 502 });
  });

  it("maps native 4xx and 5xx without inventing a fallback", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(personImage(db, "acme-co", "person-1", a, transport({}, 422))).rejects.toMatchObject({
      status: 422,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_REJECTED",
    });
    await expect(personImage(db, "acme-co", "person-1", a, transport({}, 500))).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_PEOPLE_PAYROLL_LANE9_UNAVAILABLE",
    });
  });

  it("rejects unbounded Company and person input before any request", async () => {
    const a = actor("h001", "acme", "acme-co");
    await expect(personImage(db, "../other", "person-1", a, transport(payload()))).rejects.toMatchObject({
      status: 422,
    });
    await expect(personImage(db, "acme-co", "bad id", a, transport(payload()))).rejects.toMatchObject({
      status: 422,
    });
    expect(calls).toHaveLength(0);
  });
});
