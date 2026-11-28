import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {
  createNativeLane19Client,
  nativeLane19Configured,
  nativeLane19GuestAccountEvidence,
  parseNativeLane19Config,
  type NativeLane19Actor,
} from "../../../modules/pms/server/src/adapters/native-lane-19/client";

const ENV = "ERPX_NATIVE_LANE_19_CONFIG";

/** Exact development value of `ERPX_NATIVE_LANE_19_CONFIG`. */
const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
    },
    {
      origin: "https://northstar.jesadakorn.com",
      host: "northstar.jesadakorn.com",
      holdingId: "h002",
      holdingSlug: "northstar",
    },
    {
      origin: "https://erpx-internal.jesadakorn.com",
      host: "erpx-internal.jesadakorn.com",
      holdingId: "h003",
      holdingSlug: "erpx",
    },
  ],
};

/** Byte-identical to the Rust `result_oracle_is_byte_exact` fixture. The
 * multi-Holding cutover changes routing only, never the response shape. */
const PINNED =
  '{"id":"acct-1","propertyId":"prop-1","hotel":"Riverside","businessDate":"2026-11-25","currency":"THB","account":{"guestAccountId":"old-1","id":"acct-1","kind":"guest_account","state":"open"},"entries":[{"id":"entry-1","journalId":"journal-1","data":{"id":"entry-1","kind":"charge","toGuestAccountId":"old-2","total":117}}]}';
const PINNED_HASH = "9f5e00d2e082305bbc3c8ec14a5ddee3affcdc7078a06ab5cdfe5910b4690fec";

const config = (overrides: Record<string, unknown> = {}) => structuredClone({ ...DEV_CONFIG, ...overrides });
const actor = {
  holdingId: "h001",
  holdingSlug: "acme",
  companyId: "company-1",
  branchId: "branch-1",
  departmentId: null,
};
const evidenceActor = (overrides: Record<string, unknown> = {}) =>
  ({ ...actor, ...overrides }) as NativeLane19Actor;
const responseBody = { evidence: JSON.parse(PINNED) };
const ok = (body: unknown = responseBody) =>
  new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
const failed = (status: number, code: string) =>
  new Response(JSON.stringify({ error: { code, message: "Synthetic failure" } }), {
    status,
    headers: { "content-type": "application/json" },
  });
const expectUnavailable = async (promise: Promise<unknown>, code: string) => {
  await expect(promise).rejects.toMatchObject({ status: 503, code });
};
const headersOf = (init: RequestInit) => new Headers(init.headers as HeadersInit);

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("native lane 19 guest-account evidence transport", () => {
  it("pins the dev config and routes the caller's registered Holding", async () => {
    const transport = vi.fn().mockResolvedValue(ok());
    const client = createNativeLane19Client(config(), transport);
    const evidence = await client.guestAccountFinanceEvidence(evidenceActor(), "acct-1");

    expect(JSON.stringify(evidence)).toBe(PINNED);
    expect(createHash("sha256").update(PINNED).digest("hex")).toBe(PINNED_HASH);

    const [url, init] = transport.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      "http://127.0.0.1:3198/api/v1/holdings/acme/pms/guest-account-finance-evidence",
    );
    const headers = headersOf(init);
    expect(headers.get("host")).toBe("acme.jesadakorn.com");
    expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
    expect(headers.get("x-erpx-context-company")).toBe("company-1");
    expect(headers.get("x-erpx-context-branch")).toBe("branch-1");
    expect(headers.get("x-erpx-context-record")).toBe("acct-1");
    expect(JSON.stringify(init)).not.toMatch(/SELECT|prepare/i);
  });

  it("selects every registered Holding by holdingId + holdingSlug", async () => {
    for (const [holdingId, holdingSlug, host, path] of [
      ["h001", "acme", "acme.jesadakorn.com", "acme"],
      ["h002", "northstar", "northstar.jesadakorn.com", "northstar"],
      ["h003", "erpx", "erpx-internal.jesadakorn.com", "erpx"],
    ]) {
      const transport = vi.fn().mockResolvedValue(ok());
      const evidence = await createNativeLane19Client(config(), transport).guestAccountFinanceEvidence(
        evidenceActor({ holdingId, holdingSlug }),
        "acct-1",
      );
      expect(evidence.id).toBe("acct-1");
      const [url, init] = transport.mock.calls[0] as [URL, RequestInit];
      expect(url.toString()).toBe(
        `http://127.0.0.1:3198/api/v1/holdings/${path}/pms/guest-account-finance-evidence`,
      );
      expect(headersOf(init).get("host")).toBe(host);
    }
  });

  it("selects by origin and rejects conflicting selectors", async () => {
    const transport = vi.fn().mockResolvedValue(ok());
    const client = createNativeLane19Client(config(), transport);
    await client.guestAccountFinanceEvidence(
      evidenceActor({ holdingId: undefined, holdingSlug: undefined, origin: "https://erpx-internal.jesadakorn.com" }),
      "acct-1",
    );
    const [url, init] = transport.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      "http://127.0.0.1:3198/api/v1/holdings/erpx/pms/guest-account-finance-evidence",
    );
    expect(headersOf(init).get("host")).toBe("erpx-internal.jesadakorn.com");

    await expectUnavailable(
      client.guestAccountFinanceEvidence(
        evidenceActor({ origin: "https://northstar.jesadakorn.com" }),
        "acct-1",
      ),
      "PMS_NATIVE_LANE_19_TARGET_UNAVAILABLE",
    );
  });

  it("never defaults to the first Holding and fails closed on unknown targets", async () => {
    const transport = vi.fn();
    const client = createNativeLane19Client(config(), transport);
    for (const unknown of [
      evidenceActor({ holdingId: undefined, holdingSlug: undefined }),
      evidenceActor({ holdingId: "h999", holdingSlug: "unknown" }),
      evidenceActor({ holdingId: "h001", holdingSlug: "northstar" }),
      evidenceActor({ holdingId: undefined, holdingSlug: undefined, origin: "https://unknown.example.com" }),
    ]) {
      await expectUnavailable(client.guestAccountFinanceEvidence(unknown, "acct-1"), "PMS_NATIVE_LANE_19_TARGET_UNAVAILABLE");
    }
    await expectUnavailable(
      client.guestAccountFinanceEvidence(evidenceActor({ origin: "http://acme.jesadakorn.com" }), "acct-1"),
      "PMS_EVIDENCE_REQUEST_INVALID",
    );
    expect(transport).not.toHaveBeenCalled();
  });

  it("honours an optional pinned companyId and rejects a mismatched company", async () => {
    const pinned = config({
      holdings: [
        {
          ...DEV_CONFIG.holdings[0],
          companyId: "company-1",
        },
      ],
    });
    const transport = vi.fn().mockResolvedValue(ok());
    await expect(
      createNativeLane19Client(pinned, transport).guestAccountFinanceEvidence(evidenceActor(), "acct-1"),
    ).resolves.toMatchObject({ id: "acct-1" });
    await expectUnavailable(
      createNativeLane19Client(pinned, transport).guestAccountFinanceEvidence(
        evidenceActor({ companyId: "company-2" }),
        "acct-1",
      ),
      "PMS_NATIVE_LANE_19_COMPANY_MISMATCH",
    );
  });

  it("fails closed when the owner is unreachable, returns 5xx, or is not configured", async () => {
    await expectUnavailable(
      createNativeLane19Client(config(), () => {
        throw new Error("offline");
      }).guestAccountFinanceEvidence(evidenceActor(), "acct-1"),
      "PMS_NATIVE_LANE_19_UNAVAILABLE",
    );
    await expectUnavailable(
      createNativeLane19Client(config(), async () => failed(500, "INTERNAL")).guestAccountFinanceEvidence(
        evidenceActor(),
        "acct-1",
      ),
      "PMS_NATIVE_LANE_19_UNAVAILABLE",
    );
    vi.stubEnv(ENV, undefined);
    expect(nativeLane19Configured()).toBe(false);
    await expectUnavailable(
      nativeLane19GuestAccountEvidence(evidenceActor(), "acct-1"),
      "PMS_NATIVE_LANE_19_CONFIGURATION_REQUIRED",
    );
    vi.stubEnv(ENV, "{not json");
    await expectUnavailable(
      nativeLane19GuestAccountEvidence(evidenceActor(), "acct-1"),
      "PMS_NATIVE_LANE_19_CONFIGURATION_INVALID",
    );
  });

  it("accepts 1..64 holdings and rejects unsafe or ambiguous configuration", async () => {
    expect(parseNativeLane19Config(config()).holdings).toHaveLength(3);
    expect(
      parseNativeLane19Config(config({ holdings: [DEV_CONFIG.holdings[0]] })).holdings,
    ).toHaveLength(1);
    expect(
      parseNativeLane19Config(
        config({
        holdings: Array.from({ length: 64 }, (_, i) => ({
          origin: `https://h${i}.example.com`,
          host: `h${i}.example.com`,
          holdingId: `h${i}`,
          holdingSlug: `holding-${i}`,
        })),
      }),
      ).holdings,
    ).toHaveLength(64);
    for (const invalid of [
      config({ holdings: [] }),
      config({ holdings: Array.from({ length: 65 }, (_, i) => ({ ...DEV_CONFIG.holdings[0], holdingId: `h${i}` })) }),
      config({ holdings: [{ ...DEV_CONFIG.holdings[0], host: "evil.example.com" }] }),
      config({ holdings: [DEV_CONFIG.holdings[0], DEV_CONFIG.holdings[0]] }),
    ]) {
      let error: unknown;
      try {
        parseNativeLane19Config(invalid);
      } catch (caught) {
        error = caught;
      }
      expect(error).toMatchObject({ code: "PMS_NATIVE_LANE_19_CONFIGURATION_INVALID", status: 503 });
    }
  });

  it("rejects malformed responses without a fallback", async () => {
    const malformed = [
      { unexpected: true },
      { evidence: { ...responseBody.evidence, account: [] } },
      { evidence: { ...responseBody.evidence, entries: [{ id: "x", journalId: 7, data: {} }] } },
      { evidence: { ...responseBody.evidence, entries: "not an array" } },
    ];
    for (const body of malformed) {
      await expectUnavailable(
        createNativeLane19Client(config(), async () => ok(body)).guestAccountFinanceEvidence(
          evidenceActor(),
          "acct-1",
        ),
        "PMS_NATIVE_LANE_19_RESPONSE_INVALID",
      );
    }
  });

  it("maps bounded owner failures and rejects unsafe identities", async () => {
    const transport = vi.fn().mockResolvedValue(failed(404, "PMS_NOT_FOUND"));
    await expect(
      createNativeLane19Client(config(), transport).guestAccountFinanceEvidence(evidenceActor(), "acct-1"),
    ).rejects.toMatchObject({ status: 404, code: "PMS_NOT_FOUND", message: "Synthetic failure" });

    const client = createNativeLane19Client(config(), vi.fn());
    await expectUnavailable(client.guestAccountFinanceEvidence(evidenceActor(), "acct/1"), "PMS_EVIDENCE_REQUEST_INVALID");
  });

  it("leaves no SQL in the replaced TypeScript adapter", () => {
    const source = readFileSync(
      new URL("../../../modules/pms/server/src/adapters/guest-account-finance-evidence.ts", import.meta.url),
      "utf8",
    );
    expect(source).not.toMatch(/\bSELECT\b/i);
    expect(source).not.toMatch(/\bprepare\s*\(/);
    expect(source).toContain("nativeLane19GuestAccountEvidence");
  });

  it("exposes an ApiError-compatible failure type", () => {
    expect(new ApiError(503, "CODE", "message")).toBeInstanceOf(Error);
  });
});
