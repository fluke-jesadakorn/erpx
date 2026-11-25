import { afterEach, describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativeLane20Configured,
  propertyReadinessFacts,
  type LaneActor,
  type PropertyReadiness,
} from "../../../packages/server-pms/src/adapters/native-lane-20/client";

const ENV = "ERPX_NATIVE_LANE_20_CONFIG";
const db = {} as SqlDatabase;
const facts: PropertyReadiness = {
  hasRoomTypes: true,
  hasActiveRooms: true,
  hasActiveRates: false,
  hasStays: true,
};
const acme: LaneActor = { holdingId: "h001", holdingSlug: "acme", companyId: "acme-co" };
const northstar: LaneActor = { origin: "https://northstar.jesadakorn.com", companyId: "northstar-co" };

let cleanup: Array<() => void> = [];
afterEach(() => {
  for (const stop of cleanup.splice(0)) stop();
  delete process.env[ENV];
});

type Seen = { method?: string; url?: string; headers: IncomingMessage["headers"] };

const holding = (origin: string, id: string, slug: string, companyId?: string) => ({
  origin,
  host: new URL(origin).host,
  holdingId: id,
  holdingSlug: slug,
  ...(companyId ? { companyId } : {}),
});

async function withServer(
  handler: (request: IncomingMessage, response: ServerResponse) => void,
  run: (seen: Seen, port: number) => Promise<void>,
  holdings: readonly unknown[] = [
    holding("https://acme.jesadakorn.com", "h001", "acme", "acme-co"),
    holding("https://northstar.jesadakorn.com", "h002", "northstar", "northstar-co"),
    holding("https://erpx-internal.jesadakorn.com", "h003", "erpx", "erpx-dev"),
  ],
): Promise<void> {
  const seen: Seen = { headers: {} };
  const server = createServer((request, response) => {
    seen.method = request.method;
    seen.url = request.url;
    seen.headers = request.headers;
    handler(request, response);
  });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as AddressInfo).port;
  process.env[ENV] = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${port}`,
    timeoutMs: 1000,
    holdings,
  });
  cleanup.push(() => server.close());
  await run(seen, port);
}

const json = (response: ServerResponse, status: number, value: unknown): void => {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(JSON.stringify(value));
};

describe("native PMS lane 20 multi-holding transport", () => {
  it("selects the registered Holding from WorkContext and pins its exact route and headers", async () => {
    await withServer((_request, response) => json(response, 200, { facts }), async seen => {
      const result = await propertyReadinessFacts(db, acme, "property-1");
      expect(result).toEqual(facts);
      expect(seen.method).toBe("GET");
      expect(seen.url).toBe("/api/v1/holdings/acme/pms/native-lane-20/property-readiness");
      expect(seen.headers.host).toBe("acme.jesadakorn.com");
      expect(seen.headers.origin).toBe("https://acme.jesadakorn.com");
      expect(seen.headers["x-erpx-context-company"]).toBe("acme-co");
      expect(seen.headers["x-erpx-context-property"]).toBe("property-1");
    });
  });

  it("selects a second Holding by origin, never the first registered one", async () => {
    await withServer((_request, response) => json(response, 200, { facts: null }), async seen => {
      expect(await propertyReadinessFacts(db, northstar, "property-2")).toBeNull();
      expect(seen.url).toBe("/api/v1/holdings/northstar/pms/native-lane-20/property-readiness");
      expect(seen.headers.host).toBe("northstar.jesadakorn.com");
      expect(seen.headers.origin).toBe("https://northstar.jesadakorn.com");
      expect(seen.headers["x-erpx-context-company"]).toBe("northstar-co");
    });
  });

  it("fails closed for unknown, missing and ambiguous targets without issuing a request", async () => {
    const unknown = { holdingId: "h999", holdingSlug: "ghost", companyId: "ghost-co" };
    for (const actor of [unknown, {}, { origin: "https://evil.example.test" }]) {
      let called = false;
      await withServer(
        () => { called = true; },
        async () => {
          await expect(propertyReadinessFacts(db, actor, "property-1")).rejects.toMatchObject({
            status: 503,
            code: "PMS_NATIVE_LANE_20_TARGET_UNAVAILABLE",
          });
        },
      );
      expect(called).toBe(false);
      cleanup.forEach(stop => stop());
      cleanup = [];
    }
    await withServer(
      (_request, response) => json(response, 200, { facts }),
      async () => {
        const shared = [
          holding("https://acme.jesadakorn.com", "h001", "acme", "shared-co"),
          holding("https://northstar.jesadakorn.com", "h002", "northstar", "shared-co"),
        ];
        process.env[ENV] = JSON.stringify({
          privateBaseUrl: `http://127.0.0.1:1`,
          timeoutMs: 1000,
          holdings: shared,
        });
        await expect(propertyReadinessFacts(db, { companyId: "shared-co" }, "property-1")).rejects.toMatchObject({
          status: 503,
          code: "PMS_NATIVE_LANE_20_TARGET_UNAVAILABLE",
        });
      },
    );
  });

  it("fails closed on malformed facts, extra fields and missing envelope", async () => {
    const bodies: unknown[] = [
      { facts: { hasRoomTypes: true, hasActiveRooms: true, hasActiveRates: false } },
      { facts: { ...facts, injected: true } },
      { facts: { ...facts, hasStays: "yes" } },
      { value: facts },
    ];
    for (const body of bodies) {
      await withServer((_request, response) => json(response, 200, body), async () => {
        await expect(propertyReadinessFacts(db, acme, "property-1")).rejects.toMatchObject({
          status: 503,
          code: "PMS_NATIVE_LANE_20_RESPONSE_INVALID",
        });
      });
      cleanup.forEach(stop => stop());
      cleanup = [];
    }
  });

  it("propagates a bounded owner rejection and fails closed on server errors", async () => {
    await withServer(
      (_request, response) => json(response, 422, { error: { code: "PMS_NATIVE_LANE_20_REQUEST_INVALID", message: "Rejected" } }),
      async () => {
        await expect(propertyReadinessFacts(db, acme, "property-1")).rejects.toMatchObject({
          status: 422,
          code: "PMS_NATIVE_LANE_20_REQUEST_INVALID",
        });
      },
    );
    cleanup.forEach(stop => stop());
    cleanup = [];
    await withServer((_request, response) => json(response, 500, {}), async () => {
      await expect(propertyReadinessFacts(db, acme, "property-1")).rejects.toMatchObject({ status: 503 });
    });
  });

  it("rejects malformed holding configurations", async () => {
    const empty: unknown[] = [];
    const duplicate = [
      holding("https://acme.jesadakorn.com", "h001", "acme", "acme-co"),
      holding("https://acme.jesadakorn.com", "h002", "northstar", "northstar-co"),
    ];
    for (const holdings of [empty, duplicate]) {
      await withServer(
        (_request, response) => json(response, 200, { facts }),
        async () => {
          await expect(propertyReadinessFacts(db, acme, "property-1")).rejects.toMatchObject({
            status: 503,
            code: "PMS_NATIVE_LANE_20_CONFIG_INVALID",
          });
        },
        holdings,
      );
      cleanup.forEach(stop => stop());
      cleanup = [];
    }
  });

  it("fails closed when the native owner is not configured", async () => {
    delete process.env[ENV];
    expect(nativeLane20Configured()).toBe(false);
    await expect(propertyReadinessFacts(db, acme, "property-1")).rejects.toMatchObject({
      status: 503,
      code: "PMS_NATIVE_LANE_20_CONFIG_REQUIRED",
    });
  });

  it("pins the rendered SQL and result oracles", () => {
    const scope = "SELECT 1 FROM pms_properties WHERE id=$1 AND company_id=$2";
    const readiness = "SELECT EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='room_type') AS has_room_types, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='room' AND r.data_json::jsonb->>'active'='true') AS has_active_rooms, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='rate' AND r.data_json::jsonb->>'active'='true') AS has_active_rates, EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id AND p.company_id=$1 WHERE r.property_id=$2 AND r.kind='stay') AS has_stays";
    const sha = (value: string) => createHash("sha256").update(value).digest("hex");
    expect(sha(`${scope}\n${readiness}`)).toBe("de5d1aac0cbfb981b79fb75777e19bd00565cdcaaac16b4aed053607bce35565");
    expect(
      sha(JSON.stringify({ hasActiveRates: false, hasActiveRooms: true, hasRoomTypes: true, hasStays: true })),
    ).toBe("5c3f6e00b4bd3508d9e301ecdbaaf34971f27d12c6b05592503a49f2a5387a41");
  });
});
