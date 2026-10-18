import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { moduleRegistry } from "../../src/data/module-registry";
import { developmentProductOrigin, futureProductOrigin, modulePlacement, productIds, products, sharedHostKeys, validateProductRegistry } from "../../src/lib/commercial/product-registry";
import { commercialPackages } from "../../src/server/modules/commercial/domain/packages";

const root = resolve(import.meta.dirname, "../..");
const errors = validateProductRegistry(moduleRegistry.map(module => module.id));
if (errors.length) throw new Error(errors.join("\n"));
const manifest = {
  version: 1,
  status: "foundation-implementation-not-accepted",
  environment: "development",
  zone: "jesadakorn.com",
  tunnel: { name: "erpx-local", uuid: null, accountVerified: false },
  tls: { status: "not-verified", advancedCertificateManager: "purchase-declined", totalTls: "do-not-enable", requestedSANs: ["jesadakorn.com", "*.jesadakorn.com"] },
  hosts: [...productIds, ...sharedHostKeys].map(key => ({ key, origin: developmentProductOrigin(key), service: "http://127.0.0.1:3100", published: false, dnsVerified: false, tlsVerified: false, browserAccepted: false, futureDevelopmentOrigin: futureProductOrigin(key, "development"), futureProductionOrigin: futureProductOrigin(key, "production"), futureOriginsTrusted: false })),
  preservedAliases: ["console-dev-erpx.jesadakorn.com", "dev-pos.jesadakorn.com", "dev-pms.jesadakorn.com"],
  products,
  modulePlacement,
  packages: commercialPackages,
};
const path = resolve(root, "docs/contracts/next/product-registry.generated.json");
const expected = JSON.stringify(manifest, null, 2) + "\n";
if (process.argv.includes("--check")) {
  if (readFileSync(path, "utf8") !== expected) throw new Error("PRODUCT_MANIFEST_STALE");
} else writeFileSync(path, expected);
console.log(`Product manifest ${process.argv.includes("--check") ? "checked" : "generated"}: ${products.length} products, ${manifest.hosts.length} hosts, ${Object.keys(modulePlacement).length} modules. No DNS/TLS or rollout acceptance inferred.`);
