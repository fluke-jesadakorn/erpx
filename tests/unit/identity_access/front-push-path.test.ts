import { describe, expect, it } from "vitest";
import { frontPushLandingPath } from "@erpx/feature-shared/lib/front-host";

describe("Front push landing path", () => {
  it("stays on the Front origin and preserves each authorized scope", () => {
    expect(frontPushLandingPath({ kind: "holding", holdingSlug: "acme" }, "hotel"))
      .toBe("/acme/hotel/holding/notifications");
    expect(frontPushLandingPath({ kind: "branch", holdingSlug: "acme", branchSlug: "bangkok" }, "hotel"))
      .toBe("/acme/hotel/branches/bangkok/notifications");
    expect(frontPushLandingPath({ kind: "department", holdingSlug: "acme", branchSlug: "bangkok", departmentSlug: "rooms" }, "hotel"))
      .toBe("/acme/hotel/branches/bangkok/departments/rooms/notifications");
    expect(frontPushLandingPath({ kind: "department", holdingSlug: "acme", departmentSlug: "rooms" }, "hotel"))
      .toBe("/acme/hotel/departments/rooms/notifications");
  });
  it('uses the app path on a registered Holding address',()=>{
    expect(frontPushLandingPath({kind:'branch',holdingSlug:'acme',branchSlug:'bangkok'},'acme-co','pos')).toBe('/bangkok/pos/notifications');
    expect(frontPushLandingPath({kind:'holding',holdingSlug:'acme'},'acme-co','pms')).toBe('/pms/notifications?company=acme-co');
  });
});

import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

function worker() {
  const handlers: Record<string, (event: any) => void> = {};
  const shown: any[] = [];
  const opened: string[] = [];
  const self = {
    location: { origin: "https://acme.jesadakorn.com" },
    registration: { showNotification: async (_title: string, options: unknown) => { shown.push(options); } },
    addEventListener: (name: string, handler: (event: any) => void) => { handlers[name] = handler; },
  };
  runInNewContext(readFileSync("public/erpx-push-sw.js", "utf8"), {
    self, URL, clients: { matchAll: async () => [], openWindow: async (url: string) => { opened.push(url); } },
  });
  return { handlers, shown, opened };
}

describe("push worker navigation", () => {
  it.each([
    "/bangkok/pos/notifications",
    "/pms/notifications?company=acme-co",
    "/bangkok/rooms/pos/notifications",
    "/departments/rooms/pms/notifications",
    "/work/inbox?tab=updates&company=acme-co",
    "/bangkok/rooms/work/inbox?tab=updates",
    "/acme/hotel/branches/bangkok/notifications",
    "/holdings/acme/inbox?tab=updates",
  ])("delivers and opens %s", async path => {
    const { handlers, shown, opened } = worker();
    let promise: Promise<unknown> = Promise.resolve();
    handlers.push({ data: { json: () => ({ path }) }, waitUntil: (value: Promise<unknown>) => { promise = value; } });
    await promise;
    expect(shown[0].data.path).toBe(path);
    handlers.notificationclick({ notification: { data: shown[0].data, close() {} }, waitUntil: (value: Promise<unknown>) => { promise = value; } });
    await promise;
    expect(opened).toEqual([`https://acme.jesadakorn.com${path}`]);
  });
  it.each(["//evil.example/work/inbox?tab=updates", "https://evil.example/pms/notifications", "/api/pms/notifications", "/__erpx/pms/notifications", "/pms/notifications?company=acme%0a", "/pms/notifications?company=acme&next=https://evil.example", "/work/inbox?tab=updates#x"]) ("rejects %s", async path => {
    const { handlers, shown } = worker();
    let promise: Promise<unknown> = Promise.resolve();
    handlers.push({ data: { json: () => ({ path }) }, waitUntil: (value: Promise<unknown>) => { promise = value; } });
    await promise;
    expect(shown[0].data.path).toBe("/");
  });
  it("does not open a cross-origin notification even if its stored data was tampered with", () => {
    const { handlers, opened } = worker();
    handlers.notificationclick({ notification: { data: { path: "https://evil.example/" }, close() {} }, waitUntil() { throw new Error("unexpected navigation"); } });
    expect(opened).toEqual([]);
  });
});
