import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("server-only", () => ({}));
import {
  createNativeBrandManagementClient,
  nativeBrandManagementBranch,
  nativeBrandManagementCompanyExists,
  type NativeActiveBranch,
} from "../../../modules/brand/server/src/adapters/native-brand-management";
import { LANE39_CONFIG_ENV } from "../../../modules/brand/server/src/adapters/native-lane-39/client";
import type { NativeLane39Transport } from "../../../modules/brand/server/src/adapters/native-lane-39/client";

/** Same registered private config as lane 39; lane 40 reuses the Holding set. */
const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
      companyId: "acme-co",
    },
  ],
} as const;

const raw = JSON.stringify(DEV_CONFIG);
const actor = {
  holdingId: "h001",
  holdingSlug: "acme",
  companyId: "acme-co",
  assignmentId: "a-1",
  scope: { kind: "company" },
} as never;

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });

const adapter = resolve(
  process.cwd(),
  "modules/brand/server/src/adapters/native-brand-management.ts",
);
const source = readFileSync(adapter, "utf8");

describe("lane 40 Brand native management guard transport", () => {
  it("emits the exact branch target, bind-free body and Company header", async () => {
    const transport: NativeLane39Transport = vi.fn(async (url, init) => {
      expect(url.toString()).toBe(
        "http://127.0.0.1:3198/api/v1/holdings/acme/branding/lane40-management-branch",
      );
      expect(init.method).toBe("POST");
      expect(String(init.body)).toBe(JSON.stringify({ id: "br-1" }));
      expect(String(init.body)).not.toContain("SELECT");
      const headers = init.headers as Headers;
      expect(headers.get("host")).toBe("acme.jesadakorn.com");
      expect(headers.get("x-erpx-context-company")).toBe("acme-co");
      return json({ branch: { id: "br-1", display_name: "North Branch" } });
    });
    const client = createNativeBrandManagementClient(raw, transport);
    const branch: NativeActiveBranch | null = await client.branch(actor, "br-1");
    expect(branch).toEqual({ id: "br-1", display_name: "North Branch" });
  });

  it("keeps the retired null and boolean payloads", async () => {
    const transport: NativeLane39Transport = vi.fn(async (url) =>
      url.pathname.endsWith("company")
        ? json({ exists: true })
        : json({ branch: null }),
    );
    const client = createNativeBrandManagementClient(raw, transport);
    expect(await client.branch(actor, "br-9")).toBeNull();
    expect(await client.companyExists(actor, "acme-co")).toBe(true);
  });

  it("fails closed on a drifted guard reply", async () => {
    const transport: NativeLane39Transport = vi.fn(async () =>
      json({ branch: { id: "br-1", display_name: "", extra: 1 } }),
    );
    const client = createNativeBrandManagementClient(raw, transport);
    await expect(client.branch(actor, "br-1")).rejects.toMatchObject({
      code: "BRAND_MANAGEMENT_RESPONSE_INVALID",
    });
  });

  it("rejects a malformed target before any transport call", async () => {
    const transport = vi.fn(async () => json({ branch: null }));
    const client = createNativeBrandManagementClient(raw, transport);
    await expect(client.branch(actor, "bad/id")).rejects.toMatchObject({
      code: "BRAND_MANAGEMENT_REQUEST_INVALID",
    });
    expect(transport).not.toHaveBeenCalled();
  });

  it("no TypeScript authority, SQL plan or bind array remains in the adapter", () => {
    expect(source).not.toContain("SELECT ");
    expect(source).not.toContain("prepare(");
    expect(source).not.toContain("planBrandRead");
    expect(source).not.toContain("BRAND_RD_MANAGEMENT");
    expect(LANE39_CONFIG_ENV).toBe("ERPX_NATIVE_BRAND_LANE39_CONFIG");
  });

  it("default exports keep the branding.ts call signatures", async () => {
    const previous = process.env[LANE39_CONFIG_ENV];
    process.env[LANE39_CONFIG_ENV] = raw;
    try {
      const transport = vi.fn(async (url: URL | RequestInfo) =>
        json(
          String(url).endsWith("company")
            ? { exists: false }
            : { branch: null },
        ),
      );
      const spy = vi.spyOn(globalThis, "fetch").mockImplementation(transport as never);
      expect(await nativeBrandManagementBranch({} as never, actor, "br-1")).toBe(null);
      expect(await nativeBrandManagementCompanyExists({} as never, actor, "acme-co")).toBe(false);
      expect(spy).toHaveBeenCalled();
      spy.mockRestore();
    } finally {
      if (previous === undefined) delete process.env[LANE39_CONFIG_ENV];
      else process.env[LANE39_CONFIG_ENV] = previous;
    }
  });
});
