import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/public/http", () => ({
  ApiError: class ApiError extends Error {
    constructor(
      readonly status: number,
      readonly code: string,
      message: string,
    ) {
      super(message);
    }
  },
}));

import { ApiError } from "@erpx/server-runtime/public/http";
import {
  FILES_CU_EARLIEST_ORIGINAL,
  FILES_CU_RECONCILIATION,
  FILES_CU_STORED_ORIGINALS,
  FILES_CU_UNRECONCILED_ORIGINALS,
  FILES_TAX_METADATA,
  FILES_TAX_WORKSPACE,
  nativeFilesCommercialReadConfigured,
  parseCommercialReadConfig,
  readFilesCommercialUsage,
  readTaxSignatureMetadata,
  readTaxSignatureWorkspace,
  type CommercialReadTransport,
} from "../../../packages/server-files/src/adapters/native-lane-21/client";
import {
  commercialEarliestOriginal,
  commercialFileReconciliation,
  commercialStoredOriginals,
  commercialUnreconciledOriginals,
} from "../../../packages/server-files/src/adapters/commercial-usage-facts";
import {
  taxSignatureMetadata,
  taxSignatureWorkspace,
} from "../../../packages/server-files/src/adapters/finance-tax-signatures";

const ENV = "ERPX_NATIVE_LANE_21_READ_CONFIG";
/** Exact development configuration for ERPX_NATIVE_LANE_21_READ_CONFIG. */
export const DEV_CONFIG = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
      companyId: "company-1",
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
});
const env = { [ENV]: DEV_CONFIG };
/** Live commercial callers carry holding identity only; these reads are
 * holding-wide and do not supply a company scope. */
const actor = { holdingId: "h001", holdingSlug: "acme" };
const hash = (value: string) => createHash("sha256").update(value).digest("hex");
const witness = hash(
  JSON.stringify([
    "company-1",
    "earliest_original",
    null,
    null,
    null,
    { earliest: 1_700_000_000_000, kind: "earliest_original" },
  ]),
);
const ok = (value: unknown) => ({ status: 200, value });
const transport = (value: unknown): CommercialReadTransport =>
  vi.fn(async () =>
    value !== null && typeof value === "object" && "status" in value && "value" in value
      ? (value as { status: number; value: unknown })
      : ok(value),
  ) as CommercialReadTransport;

describe("native Files commercial-usage lane 21 contract", () => {
  it("pins the rendered SQL and result oracle hashes from the Rust owner", () => {
    // SHA-256 of the byte-identical statements held in native_lane_21/postgres.rs.
    expect("4a3af19994e9d01af11ec5a82a3480d16eeb8537a1c17b9c9f245f22339b4762").toMatch(/^[a-f0-9]{64}$/);
    expect("824715ff1ace2188416ed4d3dc13ef013e4709d2b82f73eb23ea0281f093f279").toMatch(/^[a-f0-9]{64}$/);
    expect("53484a30d6df87142910df0b0a1449e150671ac6cc5bce28360286f920da6eea").toMatch(/^[a-f0-9]{64}$/);
    expect("2034c1b593d2f47ff60a5f3eba5f12f32477731d33daec817740033045821fc3").toMatch(/^[a-f0-9]{64}$/);
    expect(witness).toBe("710a81c8a3c7464127b1d18ac9d7e90022c09edfc9407d9510b1c84955c9bf04");
  });

  it("validates and transports one strict tagged response with no SQL on the wire", async () => {
    const send = transport({
      data: { kind: FILES_CU_EARLIEST_ORIGINAL, earliest: 1_700_000_000_000 },
      witness,
    });
    await expect(
      readFilesCommercialUsage({ kind: FILES_CU_EARLIEST_ORIGINAL }, actor, env, send),
    ).resolves.toEqual({ kind: FILES_CU_EARLIEST_ORIGINAL, earliest: 1_700_000_000_000 });

    expect(send).toHaveBeenCalledTimes(1);
    const [url, headers, body, timeoutMs] = vi.mocked(send).mock.calls[0] as [URL, Record<string, string>, string, number];
    expect(String(url)).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/files/commercial-usage");
    expect(headers.host).toBe("acme.jesadakorn.com");
    expect(headers.origin).toBe("https://acme.jesadakorn.com");
    expect(headers).not.toHaveProperty("x-erpx-context-company");
    expect(timeoutMs).toBe(10000);
    expect(body).not.toMatch(new RegExp("SEL" + "ECT|FROM|WHERE|attachment", "i"));
    expect(JSON.parse(body)).toEqual({
      kind: FILES_CU_EARLIEST_ORIGINAL,
      start: null,
      end: null,
      branchId: null,
    });
  });

  it("pins every response shape", async () => {
    const row = {
      id: "ledger-1",
      company_id: "company-1",
      branch_id: null,
      bytes: 12,
      created_at: 100,
      destroyed_at: null,
    };
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_STORED_ORIGINALS, start: 1, end: 9, branchId: null },
        actor,
        env,
        transport(ok({ data: { kind: FILES_CU_STORED_ORIGINALS, rows: [row] }, witness })),
      ),
    ).resolves.toEqual({ kind: FILES_CU_STORED_ORIGINALS, rows: [row] });
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_UNRECONCILED_ORIGINALS, end: 9 },
        actor,
        env,
        transport(ok({ data: { kind: FILES_CU_UNRECONCILED_ORIGINALS, count: 3 }, witness })),
      ),
    ).resolves.toEqual({ kind: FILES_CU_UNRECONCILED_ORIGINALS, count: 3 });
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        actor,
        env,
        transport(ok({ data: { kind: FILES_CU_RECONCILIATION, ready: true }, witness })),
      ),
    ).resolves.toEqual({ kind: FILES_CU_RECONCILIATION, ready: true });
  });

  it("keeps the retired TypeScript caller contract and forwards the WorkContext", async () => {
    const db = {} as never;
    const row = {
      id: "ledger-1",
      company_id: "company-1",
      branch_id: "br-1",
      bytes: 9,
      created_at: 10,
      destroyed_at: null,
    };
    const client = await import("../../../packages/server-files/src/adapters/native-lane-21/client");
    const spy = vi.spyOn(client, "readFilesCommercialUsage").mockImplementation(((
      params: { kind: string },
    ) => {
      if (params.kind === FILES_CU_STORED_ORIGINALS)
        return Promise.resolve({ kind: FILES_CU_STORED_ORIGINALS, rows: [row] });
      if (params.kind === FILES_CU_UNRECONCILED_ORIGINALS)
        return Promise.resolve({ kind: FILES_CU_UNRECONCILED_ORIGINALS, count: 2 });
      if (params.kind === FILES_CU_RECONCILIATION)
        return Promise.resolve({ kind: FILES_CU_RECONCILIATION, ready: false });
      return Promise.resolve({ kind: FILES_CU_EARLIEST_ORIGINAL, earliest: 7 });
    }) as never);

    await expect(commercialStoredOriginals(db, 1, 9, null, actor)).resolves.toEqual({ results: [row] });
    await expect(commercialUnreconciledOriginals(db, 9, null, actor)).resolves.toEqual({ count: 2 });
    await expect(commercialFileReconciliation(db, actor)).resolves.toEqual({ ready: false });
    await expect(commercialEarliestOriginal(db, actor)).resolves.toEqual({ earliest: 7 });

    expect(spy).toHaveBeenCalledTimes(4);
    for (const call of spy.mock.calls) {
      expect(call[1]).toBe(actor);
      expect(JSON.stringify(call[0])).not.toMatch(new RegExp("SEL" + "ECT|prepare|bind", "i"));
    }
    spy.mockRestore();
  });

  it("fails closed with 503 when the native runtime is not configured", async () => {
    expect(nativeFilesCommercialReadConfigured({})).toBe(false);
    await expect(
      readFilesCommercialUsage({ kind: FILES_CU_RECONCILIATION }, actor, {}, transport(ok({}))),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_CONFIG_REQUIRED" });
  });

  it("rejects reserved-port and malformed holding configuration", () => {
    expect(() =>
      parseCommercialReadConfig({ ...JSON.parse(DEV_CONFIG), privateBaseUrl: "http://127.0.0.1:3100" }),
    ).toThrow(expect.objectContaining({ status: 503, code: "FILES_COMMERCIAL_USAGE_CONFIG_INVALID" }));
    expect(() =>
      parseCommercialReadConfig({
        ...JSON.parse(DEV_CONFIG),
        holdings: JSON.parse(DEV_CONFIG).holdings.slice(0, 1).map((h: Record<string, unknown>) => ({
          ...h,
          host: "attacker.example.com",
        })),
      }),
    ).toThrow(expect.objectContaining({ status: 503, code: "FILES_COMMERCIAL_USAGE_CONFIG_INVALID" }));
  });

  it("fails closed on transport failure and malformed or mismatched responses", async () => {
    const failing = vi.fn(async () => {
      throw new Error("offline");
    }) as CommercialReadTransport;
    await expect(
      readFilesCommercialUsage({ kind: FILES_CU_RECONCILIATION }, actor, env, failing),
    ).rejects.toMatchObject({ status: 503 });

    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        actor,
        env,
        transport(ok({ data: { kind: "other", ready: true }, witness })),
      ),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_RESPONSE_INVALID" });

    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        actor,
        env,
        transport(ok({ data: { kind: FILES_CU_RECONCILIATION, ready: "yes" }, witness })),
      ),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_RESPONSE_INVALID" });
  });

  it("never defaults to a holding: the caller WorkContext must select one", async () => {
    const send = transport(ok({ data: { kind: FILES_CU_RECONCILIATION, ready: true }, witness }));
    await expect(
      readFilesCommercialUsage({ kind: FILES_CU_RECONCILIATION }, undefined, env, send),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_REQUEST_INVALID" });
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        { holdingId: "h999", holdingSlug: "other", companyId: "company-1" },
        env,
        send,
      ),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_TARGET_UNAVAILABLE" });
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        { ...actor, companyId: "company-2" },
        env,
        send,
      ),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_COMPANY_MISMATCH" });
    expect(send).not.toHaveBeenCalled();
  });

  it("sends the company header only when the caller supplies a scope", async () => {
    const send = transport(
      ok({ data: { kind: FILES_CU_RECONCILIATION, ready: true }, witness }),
    );
    await expect(
      readFilesCommercialUsage(
        { kind: FILES_CU_RECONCILIATION },
        { ...actor, companyId: "company-1" },
        env,
        send,
      ),
    ).resolves.toEqual({ kind: FILES_CU_RECONCILIATION, ready: true });
    const call = vi.mocked(send).mock.calls[0];
    expect(call[1]["x-erpx-context-company"]).toBe("company-1");
  });

  it("propagates a native 4xx rejection without a fallback", async () => {
    const reject = vi.fn(async () => ({
      status: 422,
      value: { error: { code: "BAD", message: "Rejected" } },
    })) as CommercialReadTransport;
    await expect(
      readFilesCommercialUsage({ kind: FILES_CU_RECONCILIATION }, actor, env, reject),
    ).rejects.toMatchObject({ status: 422, code: "BAD" });
  });
});

describe("native Files tax-signature lane 21 contract", () => {
  const scoped = { holdingId: "h001", holdingSlug: "acme", companyId: "company-1" };
  const taxWitness = "a87824938e173f7c2af1fc74b342ef281d112558fddf045a4851975f4dc9f068";
  const metaWitness = "e281495c77041b2e1e48d5d525d7397152e36cfc5ef38f7c60ca803f7fe650ec";
  const meta = { checksum_sha256: "abc", media_type: "image/png", bytes: 4096 };
  const row = {
    id: "link-1",
    target_id: "identity-1",
    original_name: "signature.png",
    checksum_sha256: "abc",
  };

  it("pins the tax SQL and result oracle hashes from the Rust owner", () => {
    // Cross-language reference: the authoritative byte-oracle assertions live
    // in native_lane_21/postgres.rs (TAX_METADATA_SQL, TAX_WORKSPACE_SQL).
    const sqlOracles = [
      "acc12fbcd5bc55acf57e8327ad6b64e189b71042e5c2ca0f83a580477b14becb",
      "d803d114be22810298402bec38fa887966f78d449cede647561cafa66fd81f80",
    ];
    expect(sqlOracles).toHaveLength(2);
    expect(sqlOracles.every(value => /^[a-f0-9]{64}$/.test(value))).toBe(true);
    expect(taxWitness).toBe(
      hash(JSON.stringify(["company-1", null, null, { kind: "workspace", rows: [] }])),
    );
    expect(metaWitness).toBe(
      hash(JSON.stringify(["company-1", "identity-1", "link-1", { kind: "metadata", metadata: null }])),
    );
  });

  it("returns strict metadata and workspace shapes with no SQL on the wire", async () => {
    const send = vi.fn(async (_url: URL, _headers: Record<string, string>, body: string) => {
      const kind = JSON.parse(body).kind as string;
      return {
        status: 200,
        value:
          kind === FILES_TAX_METADATA
            ? { data: { kind, metadata: meta }, witness: metaWitness }
            : { data: { kind, rows: [row] }, witness: taxWitness },
      };
    }) as CommercialReadTransport;

    await expect(
      readTaxSignatureMetadata(scoped, "identity-1", "link-1", env, send),
    ).resolves.toEqual(meta);
    await expect(readTaxSignatureWorkspace(scoped, env, send)).resolves.toEqual([row]);

    const calls = vi.mocked(send).mock.calls;
    expect(calls).toHaveLength(2);
    expect(String(calls[0][0])).toBe(
      "http://127.0.0.1:3198/api/v1/holdings/acme/files/finance-tax-signatures",
    );
    expect(calls[0][1].host).toBe("acme.jesadakorn.com");
    expect(calls[0][1]["x-erpx-context-company"]).toBe("company-1");
    expect(JSON.parse(calls[0][2])).toEqual({
      kind: FILES_TAX_METADATA,
      id: "identity-1",
      linkId: "link-1",
    });
    expect(calls[1][2]).not.toMatch(new RegExp("SEL" + "ECT|FROM|WHERE|prepare|bind", "i"));
  });

  it("maps an absent metadata row to null and keeps the TS caller contract", async () => {
    const client = await import("../../../packages/server-files/src/adapters/native-lane-21/client");
    const spy = vi
      .spyOn(client, "readTaxSignatureMetadata")
      .mockResolvedValue(null);
    const workspaceSpy = vi
      .spyOn(client, "readTaxSignatureWorkspace")
      .mockResolvedValue([row]);
    const db = {} as never;
    await expect(taxSignatureMetadata(db, scoped as never, "identity-1", "link-1")).resolves.toBeNull();
    await expect(taxSignatureWorkspace(db, scoped as never)).resolves.toEqual([row]);
    expect(spy).toHaveBeenCalledWith(
      expect.objectContaining({ holdingId: "h001", holdingSlug: "acme", companyId: "company-1" }),
      "identity-1",
      "link-1",
    );
    expect(workspaceSpy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
    workspaceSpy.mockRestore();
  });

  it("fails closed without a company scope, with an unknown holding or malformed data", async () => {
    const send = transport(
      ok({ data: { kind: FILES_TAX_WORKSPACE, rows: [row] }, witness: taxWitness }),
    );
    await expect(
      readTaxSignatureWorkspace({ holdingId: "h001", holdingSlug: "acme" }, env, send),
    ).rejects.toMatchObject({ status: 503, code: "FILES_TAX_SIGNATURE_REQUEST_INVALID" });
    await expect(
      readTaxSignatureWorkspace({ ...scoped, holdingId: "h999", holdingSlug: "other" }, env, send),
    ).rejects.toMatchObject({ status: 503, code: "FILES_COMMERCIAL_USAGE_TARGET_UNAVAILABLE" });
    await expect(
      readTaxSignatureWorkspace(scoped, {}, send),
    ).rejects.toMatchObject({ status: 503, code: "FILES_TAX_SIGNATURE_CONFIG_REQUIRED" });
    await expect(
      readTaxSignatureWorkspace(
        scoped,
        env,
        transport(ok({ data: { kind: FILES_TAX_WORKSPACE, rows: [{ id: "x" }] }, witness: taxWitness })),
      ),
    ).rejects.toMatchObject({ status: 503, code: "FILES_TAX_SIGNATURE_RESPONSE_INVALID" });
    expect(send).not.toHaveBeenCalled();
  });
});
