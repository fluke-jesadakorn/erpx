import { afterEach, expect, it, vi } from "vitest";
import { env } from "../../support/runtime-env";
import { measureReadRequest } from "@erpx/server-runtime/adapters/runtime/read-request";
import { measureReadPhase } from "@erpx/server-runtime/adapters/runtime/read-metrics";
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllEnvs(); });

it("correlates nested driver reads without logging the response or SQL values", async () => {
  vi.stubEnv("NODE_ENV", "development"); vi.stubEnv("ERPX_PERF_LOG", "1");
  const log = vi.spyOn(console, "info").mockImplementation(() => {});
  const response = await measureReadRequest("test", async () => {
    await measureReadPhase("nested", () => env.CONTROL_DB.prepare("SELECT ?::text value").bind("private-value").first());
    return Response.json({ secret: "private-value" }, { headers: { "Cache-Control": "private, no-store" } });
  });
  const events = log.mock.calls.map(call => JSON.parse(String(call[0])));
  const root = events.find(event => event.event === "read_request");
  expect(root).toMatchObject({ queries: 1, status: 200, failed: false });
  expect(events[0].requestId).toBe(root.requestId);
  expect(response.headers.get("x-erpx-read-id")).toBe(root.requestId);
  expect(response.headers.get("server-timing")).toContain('queries;desc="1"');
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(await response.json()).toEqual({ secret: "private-value" });
  expect(JSON.stringify(events)).not.toContain("private-value");
});

it("retains HTTP failures and thrown failures without exposing error text", async () => {
  vi.stubEnv("NODE_ENV", "development"); vi.stubEnv("ERPX_PERF_LOG", "1");
  const log = vi.spyOn(console, "info").mockImplementation(() => {});
  expect((await measureReadRequest("denied", async () => new Response(null, { status: 403 }))).status).toBe(403);
  await expect(measureReadRequest("failed", async () => { throw new Error("private-credential"); })).rejects.toThrow("private-credential");
  const events = log.mock.calls.map(call => JSON.parse(String(call[0])));
  expect(events).toHaveLength(2);
  expect(events.every(event => event.failed)).toBe(true);
  expect(events[0].status).toBe(403);
  expect(events[1].status).toBeUndefined();
  expect(JSON.stringify(events)).not.toContain("private-credential");
});

it("leaves production responses and logging alone", async () => {
  vi.stubEnv("NODE_ENV", "production"); vi.stubEnv("ERPX_PERF_LOG", "1");
  const log = vi.spyOn(console, "info").mockImplementation(() => {});
  const original = new Response("ok");
  expect(await measureReadRequest("production", async () => original)).toBe(original);
  expect(original.headers.has("server-timing")).toBe(false);
  expect(log).not.toHaveBeenCalled();
});
