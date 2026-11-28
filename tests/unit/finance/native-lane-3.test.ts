import { createServer, type Server } from "node:http";
import { createHash } from "node:crypto";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));

import {
  lane3Config,
  nativeCurrentInvoiceLines,
  nativeLane3Configured,
} from "../../../packages/server-finance/src/adapters/native-lane-3/client";
import type { Lane3Actor } from "../../../packages/server-finance/src/adapters/native-lane-3/client";
import { nativeSetupCounts } from "../../../packages/server-finance/src/adapters/native-lane-3/setup-client";
import { nativeStarterProbes } from "../../../packages/server-finance/src/adapters/native-lane-3/starter-client";
import { readFile } from "node:fs/promises";

const KEY = "ERPX_NATIVE_LANE3_READ_CONFIG";
const DEV_CONFIG = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme" },
    { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar" },
    { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx" },
  ],
});
const company = "company-1";
const invoice = "inv-1";
const revision = 3;
const actor: Lane3Actor = { holdingId: "h001", holdingSlug: "acme", companyId: company };

/** Exact statement inherited from the retired TS reader (SQL oracle). */
const SQL =
  "SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal";
const SQL_HASH = "f49986445a3c579345afe3114abe22f4a5a06d715c043d5c94ead51a5ba00754";
const RESULT_HASH = "b4d46d527382ce7b6ce7cee1edaefd71d397aa755ca85d98e9820a070a14d675";

const line = (
  ordinal: number,
  description: string,
  subtotal_minor: number,
  tax_minor: number,
  total_minor: number,
) => ({
  id: `line-${ordinal}`,
  invoice_id: invoice,
  company_id: company,
  source_version: revision,
  ordinal,
  description,
  quantity_millis: 1000,
  unit_price_minor: 5000,
  tax_bps: 700,
  subtotal_minor,
  tax_minor,
  total_minor,
  account_id: "acc-1",
  tax_account_id: null,
  source_snapshot_json: "{}",
  currency: "THB",
  origin_kind: "native_line",
});
const lines = [line(1, "alpha", 5000, 350, 5350), line(2, "beta", 10000, 700, 10700)];

const canonical = (value: unknown): string =>
  JSON.stringify(value, (_key, current) =>
    current !== null && typeof current === "object" && !Array.isArray(current)
      ? Object.fromEntries(Object.entries(current).sort(([a], [b]) => a.localeCompare(b)))
      : current,
  );
const hash = (value: string): string => createHash("sha256").update(value, "utf8").digest("hex");

const holdings = (host: string, holdingSlug: string, holdingId: string) => ({
  origin: `https://${host}`,
  host,
  holdingId,
  holdingSlug,
});
const configure = (privateBaseUrl: string, list: unknown[] = [holdings("acme.example.com", "acme", "h001")]) => {
  process.env[KEY] = JSON.stringify({ privateBaseUrl, timeoutMs: 1000, holdings: list });
};

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env[KEY];
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string; method?: string; body: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    req.on("end", () => {
      const result = handler({ headers: req.headers, url: req.url, method: req.method, body: Buffer.concat(chunks).toString("utf8") });
      res.writeHead(result.status, { "content-type": "application/json" });
      res.end(JSON.stringify(result.value));
    });
  });
  await new Promise<void>((resolve) => server!.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("no listener");
  return `http://127.0.0.1:${address.port}`;
}

describe("native Finance lane 3 multi-Holding invoice-line read", () => {
  it("pins the SQL and result oracles", () => {
    expect(hash(SQL)).toBe(SQL_HASH);
    expect(hash(canonical(lines))).toBe(RESULT_HASH);
  });

  it("validates 1..64 registered holdings and rejects a first-holding default", () => {
    const base = "http://127.0.0.1:3198";
    configure(base, [
      holdings("acme.example.com", "acme", "h001"),
      holdings("northstar.example.com", "northstar", "h002"),
      holdings("erpx-internal.jesadakorn.com", "erpx", "h003"),
    ]);
    expect(lane3Config().holdings.map((h) => h.holdingSlug)).toEqual(["acme", "northstar", "erpx"]);

    configure(base, [{ origin: "https://acme.example.com", holdingSlug: "acme" }]);
    expect(() => lane3Config()).toThrow();
  });

  it("routes to the exact configured Holding from the caller's WorkContext", async () => {
    const base = await listen((req) => {
      expect(req.method).toBe("GET");
      expect(req.url).toBe("/api/v1/holdings/northstar/finance/lane3/invoice-lines");
      expect(req.headers["x-erpx-native-host"]).toBe("northstar.example.com");
      expect(req.headers["x-erpx-context-holding"]).toBe("h002");
      expect(req.headers["x-erpx-context-company"]).toBe(company);
      expect(JSON.parse(req.body)).toEqual({ invoiceId: invoice, revision });
      return { status: 200, value: { data: { lines } } };
    });
    configure(base, [
      holdings("acme.example.com", "acme", "h001"),
      holdings("northstar.example.com", "northstar", "h002"),
      holdings("erpx-internal.jesadakorn.com", "erpx", "h003"),
    ]);
    const result = await nativeCurrentInvoiceLines(
      { holdingId: "h002", holdingSlug: "northstar", companyId: company },
      invoice,
      revision,
    );
    expect(result).toHaveLength(2);
    expect(result[1]!.total_minor).toBe(10700);
    expect(hash(canonical(result))).toBe(RESULT_HASH);
  });

  it("accepts the exact three-holding development configuration", () => {
    process.env[KEY] = DEV_CONFIG;
    const config = lane3Config();
    expect(config.base).toBe("http://127.0.0.1:3198");
    expect(config.timeoutMs).toBe(10000);
    expect(config.holdings.map((h) => [h.holdingId, h.holdingSlug, h.host])).toEqual([
      ["h001", "acme", "acme.jesadakorn.com"],
      ["h002", "northstar", "northstar.jesadakorn.com"],
      ["h003", "erpx", "erpx-internal.jesadakorn.com"],
    ]);
  });

  it("fails closed on an unregistered Holding context", async () => {
    const base = await listen(() => ({ status: 200, value: { data: { lines } } }));
    configure(base);
    await expect(
      nativeCurrentInvoiceLines({ holdingId: "h999", holdingSlug: "unknown", companyId: company }, invoice, revision),
    ).rejects.toMatchObject({ status: 503, code: "LANE3_INVOICE_LINES_TARGET_UNAVAILABLE" });
    await expect(
      nativeCurrentInvoiceLines({ holdingId: "h001", holdingSlug: "northstar", companyId: company }, invoice, revision),
    ).rejects.toMatchObject({ status: 503, code: "LANE3_INVOICE_LINES_TARGET_UNAVAILABLE" });
  });

  it("fails closed without native configuration", async () => {
    delete process.env[KEY];
    await expect(nativeCurrentInvoiceLines(actor, invoice, revision)).rejects.toMatchObject({
      status: 503,
      code: "LANE3_INVOICE_LINES_CONFIGURATION_REQUIRED",
    });
    expect(nativeLane3Configured()).toBe(false);
  });

  it("fails closed when the native route is unavailable", async () => {
    const base = await listen(() => ({ status: 503, value: { error: { code: "X" } } }));
    configure(base);
    await expect(nativeCurrentInvoiceLines(actor, invoice, revision)).rejects.toMatchObject({
      status: 503,
      code: "LANE3_INVOICE_LINES_UNAVAILABLE",
    });
  });

  it("rejects a malformed native row without a fallback", async () => {
    const base = await listen(() => ({
      status: 200,
      value: { data: { lines: [{ ...line(1, "alpha", 5000, 350, 5350), total_minor: 1 }] } },
    }));
    configure(base);
    await expect(nativeCurrentInvoiceLines(actor, invoice, revision)).rejects.toMatchObject({
      status: 502,
      code: "LANE3_INVOICE_LINES_RESPONSE_INVALID",
    });
  });

  it("rejects unsafe caller identities before any network call", async () => {
    const base = await listen(() => ({ status: 200, value: { data: { lines } } }));
    configure(base);
    await expect(nativeCurrentInvoiceLines(actor, "inv/../1", revision)).rejects.toMatchObject({
      status: 422,
      code: "LANE3_INVOICE_LINES_INPUT_INVALID",
    });
  });
});

describe("native Finance lane 3 setup reference counts", () => {
  const SETUP_SQL = [
    "SELECT count(*) count FROM finance_accounts WHERE company_id=? AND status='active'",
    "SELECT count(*) count FROM finance_periods WHERE company_id=?",
    "SELECT count(*) count FROM finance_tax_codes WHERE company_id=? AND status='active'",
    "SELECT count(DISTINCT bank_account) count FROM finance_bank_statements WHERE company_id=?",
    "SELECT base_currency FROM finance_charts WHERE company_id=? ORDER BY created_at LIMIT 1",
  ];
  const SETUP_SQL_HASHES = [
    "93c03579e1470379214f35ceda1a5e6f78449e63f175ff82cf183d3c04782b99",
    "fe6365c1e885065e2400fe8336d6656f62d659bc03873f43602e0d46e02b6a06",
    "73cb81137301f8825fd022dbfe6adea056f9bb319d01d6ae69806d888b06959e",
    "da7ae70b369dcb2c8fad82362585bebf596b7b462f7ea0111e80227be8fce607",
    "9e36403926f0c29834a89050131429d33f7f5c17fda843c8e73622f52d248970",
  ];
  const SETUP_RESULT = {
    accountCount: 12,
    periodCount: 3,
    taxCount: 4,
    bankCount: 2,
    firstCurrency: "THB",
  };
  const SETUP_RESULT_HASH = "9db081ab14783f3cb24e0fdf1a92478bf6f6a05f2b83fa7413c0693dc1bd7ebc";

  it("pins the five SQL bytes and the result projection", () => {
    expect(SETUP_SQL.map(hash)).toEqual(SETUP_SQL_HASHES);
    expect(hash(canonical(SETUP_RESULT))).toBe(SETUP_RESULT_HASH);
  });

  it("reads setup counts from the exact WorkContext Holding", async () => {
    const base = await listen((req) => {
      expect(req.url).toBe("/api/v1/holdings/northstar/finance/lane3/setup-counts");
      expect(req.headers["x-erpx-native-host"]).toBe("northstar.example.com");
      expect(req.headers["x-erpx-context-holding"]).toBe("h002");
      expect(req.headers["x-erpx-context-company"]).toBe(company);
      return { status: 200, value: { data: { setup: SETUP_RESULT } } };
    });
    configure(base, [
      holdings("acme.example.com", "acme", "h001"),
      holdings("northstar.example.com", "northstar", "h002"),
      holdings("erpx-internal.jesadakorn.com", "erpx", "h003"),
    ]);
    const result = await nativeSetupCounts({
      holdingId: "h002",
      holdingSlug: "northstar",
      companyId: company,
    });
    expect(result).toEqual(SETUP_RESULT);
    expect(hash(canonical(result))).toBe(SETUP_RESULT_HASH);
  });

  it("fails closed on an unregistered setup target and a malformed result", async () => {
    const base = await listen(() => ({ status: 200, value: { data: { setup: SETUP_RESULT } } }));
    configure(base);
    await expect(
      nativeSetupCounts({ holdingId: "h999", holdingSlug: "acme", companyId: company }),
    ).rejects.toMatchObject({ status: 503, code: "LANE3_SETUP_COUNTS_TARGET_UNAVAILABLE" });
  });

  it("rejects a malformed setup result without a fallback", async () => {
    const base = await listen(() => ({
      status: 200,
      value: { data: { setup: { ...SETUP_RESULT, bankCount: -1 } } },
    }));
    configure(base);
    await expect(nativeSetupCounts(actor)).rejects.toMatchObject({
      status: 502,
      code: "LANE3_SETUP_COUNTS_RESPONSE_INVALID",
    });
  });

  it("deletes the migrated SQL from the finance workspace adapter", async () => {
    const source = await readFile(
      new URL("../../../packages/server-finance/src/adapters/finance-workspace.ts", import.meta.url),
      "utf8",
    );
    for (const sql of SETUP_SQL) expect(source).not.toContain(sql);
    expect(source).toContain("nativeSetupCounts");
    expect(source).toContain("setupFacts");
  });
});

describe("native Finance lane 3 starter setup probes", () => {
  const PROBE_SQL = [
    "SELECT 1 FROM finance_charts WHERE company_id=?",
    "SELECT 1 FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=?",
  ];
  const PROBE_SQL_HASHES = [
    "c9913a28d54089cb16bb341daeffc59f40214a884cbfc3725a329072a3dd89f8",
    "a9614765c92d2a128b7a82fe8e9dea4941bc5d928f5bca00ddce9a2f120cc530",
  ];
  const PROBES = { existingChart: true, overlappingPeriod: false };
  const PROBES_HASH = "3479f16cfe2b1389cb0bc790bc8e3ef7d9068459fb420a7d2d934d2c159e4d67";
  const start = "2026-01-01";
  const end = "2026-12-31";

  it("pins the two SQL bytes and the boolean projection", () => {
    expect(PROBE_SQL.map(hash)).toEqual(PROBE_SQL_HASHES);
    expect(hash(canonical(PROBES))).toBe(PROBES_HASH);
  });

  it("posts the exact bounded range to the exact Holding target", async () => {
    const base = await listen((req) => {
      expect(req.url).toBe("/api/v1/holdings/northstar/finance/lane3/starter-probes");
      expect(req.headers["x-erpx-native-host"]).toBe("northstar.example.com");
      expect(req.headers["x-erpx-context-holding"]).toBe("h002");
      expect(req.headers["x-erpx-context-company"]).toBe(company);
      expect(JSON.parse(req.body)).toEqual({ start, end });
      return { status: 200, value: { data: { probes: PROBES } } };
    });
    configure(base, [
      holdings("acme.example.com", "acme", "h001"),
      holdings("northstar.example.com", "northstar", "h002"),
      holdings("erpx-internal.jesadakorn.com", "erpx", "h003"),
    ]);
    const result = await nativeStarterProbes(
      { holdingId: "h002", holdingSlug: "northstar", companyId: company },
      start,
      end,
    );
    expect(result).toEqual(PROBES);
    expect(hash(canonical(result))).toBe(PROBES_HASH);
  });

  it("fails closed on an unregistered target and rejects malformed dates", async () => {
    const base = await listen(() => ({ status: 200, value: { data: { probes: PROBES } } }));
    configure(base);
    await expect(
      nativeStarterProbes({ holdingId: "h999", holdingSlug: "acme", companyId: company }, start, end),
    ).rejects.toMatchObject({ status: 503, code: "LANE3_STARTER_PROBES_TARGET_UNAVAILABLE" });
    await expect(nativeStarterProbes(actor, "2026/01/01", end)).rejects.toMatchObject({
      status: 422,
      code: "LANE3_STARTER_PROBES_INPUT_INVALID",
    });
  });

  it("rejects a malformed native projection without a fallback", async () => {
    const base = await listen(() => ({
      status: 200,
      value: { data: { probes: { ...PROBES, overlappingPeriod: 1 } } },
    }));
    configure(base);
    await expect(nativeStarterProbes(actor, start, end)).rejects.toMatchObject({
      status: 502,
      code: "LANE3_STARTER_PROBES_RESPONSE_INVALID",
    });
  });
});
