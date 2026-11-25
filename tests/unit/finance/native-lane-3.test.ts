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
  handler: (req: { headers: Record<string, unknown>; url?: string; body: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
    req.on("end", () => {
      const result = handler({ headers: req.headers, url: req.url, body: Buffer.concat(chunks).toString("utf8") });
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
