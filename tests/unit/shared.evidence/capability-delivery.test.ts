import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import registry from "@erpx/catalogue/module-registry.json";
import { businessAcceptanceManifest } from "@erpx/catalogue/business-acceptance";
import {
  capabilityDelivery,
  deliveryModules,
  type DeliveryStatus,
} from "@erpx/catalogue/capability-delivery";

const statuses = new Set<DeliveryStatus>([
  "implemented",
  "flow_accepted",
  "sandbox_verified",
  "live_verified",
  "deferred",
]);

describe("capability delivery inventory", () => {

  it("accounts for every catalog capability including Legal", () => {
    const capabilities = registry.flatMap((module) =>
      module.sections
        .filter((section) => section.capability)
        .map((section) => [module.id, section.id] as const),
    );
    expect(capabilities).toHaveLength(businessAcceptanceManifest().length);
    for (const [moduleId, capabilityId] of capabilities) {
      const evidence = capabilityDelivery(moduleId, capabilityId);
      expect(evidence, `${moduleId}/${capabilityId}`).toBeDefined();
      expect(statuses.has(evidence!.status)).toBe(true);
      expect(evidence!.gap.trim().length).toBeGreaterThan(10);
    }
  });

  it("includes Legal and has no stale module definitions", () => {
    expect(new Set(deliveryModules())).toEqual(
      new Set(registry.map((module) => module.id)),
    );
  });

  it("links implemented evidence to repository paths", () => {
    for (const definition of registry) {
      for (const section of definition.sections.filter((item) => item.capability)) {
        const evidence = capabilityDelivery(definition.id, section.id);
        if (!evidence || evidence.status === "deferred") continue;
        for (const key of ["ui", "api", "service", "schema", "test"] as const) {
          for (const path of evidence[key].split(/[;,]/).map(value => value.trim()).filter(Boolean))
            expect(existsSync(path), `${definition.id}/${section.id} ${key}: ${path}`).toBe(true);
        }
      }
    }
  });
});
