import registry from "../src/data/module-registry.json";
import { reviewedCapabilityManifest } from "../src/data/capability-gate-reviews";
import { validateAcceptanceEvidence } from "../src/data/acceptance-evidence";
import { createHash } from "node:crypto";
import { readFileSync, realpathSync } from "node:fs";
import { resolve, sep } from "node:path";
import {
  capabilityDelivery,
  type DeliveryStatus,
} from "../src/data/capability-delivery";

const evidenceLedger = validateAcceptanceEvidence(JSON.parse(readFileSync("docs/status/business-acceptance-evidence.json", "utf8")));
const capabilitiesWithReviews = reviewedCapabilityManifest(JSON.parse(readFileSync("docs/status/capability-gate-reviews.json", "utf8")));
const evidenceRoot = resolve("docs/testing/evidence") + sep;
for (const record of evidenceLedger.records) {
  for (const evidence of record.evidence) {
    if (!realpathSync(evidence.path).startsWith(evidenceRoot)) throw new Error(`Evidence escapes repository directory: ${evidence.path}`);
    const digest = createHash("sha256").update(readFileSync(evidence.path)).digest("hex");
    if (digest !== evidence.sha256) throw new Error(`Evidence checksum mismatch: ${evidence.path}`);
  }
}

const checkOnly=process.argv.includes('--check');
async function writeOrCheck(path:string,content:string) {
  if(checkOnly){if(readFileSync(path,'utf8')!==content)throw new Error(`Stale capability artifact: ${path}. Run bun run status:capabilities.`);}
  else await Bun.write(path,content);
}

const labels: Record<DeliveryStatus, string> = {
  implemented: "Implemented",
  flow_accepted: "Flow accepted",
  sandbox_verified: "Sandbox verified",
  live_verified: "Live verified",
  deferred: "Deferred",
};

const capabilities = registry.flatMap((module) =>
  module.sections
    .filter((section) => section.capability)
    .map((section) => {
      const evidence = capabilityDelivery(module.id, section.id);
      if (!evidence)
        throw new Error(`Missing delivery evidence for ${module.id}/${section.id}`);
      return { module, section, evidence };
    }),
);

const counts = Object.fromEntries(
  Object.keys(labels).map((status) => [
    status,
    capabilities.filter((item) => item.evidence.status === status).length,
  ]),
) as Record<DeliveryStatus, number>;

const lines = [
  "# ERPX capability delivery matrix",
  "",
  "Generated from `src/data/module-registry.json` and `src/data/capability-delivery.ts`. Includes Legal. See business-acceptance-manifest.json for independent acceptance gates.",
  "Generic record CRUD and preview pages do not qualify as domain implementation. Registry status alone does not constitute production sign-off. This is an inventory, not the launch scope; see [the current production plan](../production-plan.md).",
  "",
  "## Status rules",
  "",
  "- **Implemented:** persisted domain behavior exists, but the catalog capability still has stated acceptance gaps.",
  "- **Flow accepted:** the end-to-end domain journey has integration or browser acceptance evidence in this workspace.",
  "- **Sandbox verified:** the external-provider journey passed against an approved sandbox, not live production.",
  "- **Live verified:** the named external integration was exercised against its approved live or isolated real service.",
  "- **Deferred:** no owner-specific domain flow exists; preview/generic CRUD is not completion.",
  "",
  `Coverage: **${capabilities.length}** capabilities — ${counts.flow_accepted} Flow accepted, ${counts.sandbox_verified} Sandbox verified, ${counts.live_verified} Live verified, ${counts.implemented} Implemented, ${counts.deferred} Deferred.`,
  "",
  "## Delivery inventory",
  "",
  "| Stage | Module | Capability | Status | UI | API | Service | Schema | Test | Remaining gap |",
  "| ---: | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  ...capabilities.map(({ module, section, evidence }) =>
    [
      `P${evidence.stage}`,
      module.name,
      section.label,
      labels[evidence.status],
      `\`${evidence.ui}\``,
      `\`${evidence.api}\``,
      `\`${evidence.service}\``,
      `\`${evidence.schema}\``,
      `\`${evidence.test}\``,
      evidence.gap.replaceAll("|", "\\|"),
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"),
  ),
  "",
  "## Completion gate",
  "",
  "A capability can advance only when its owner-specific lifecycle supports creation, validation or review, execution, correction or cancellation, history/reporting, authorization, idempotency, audit/outbox, private evidence where applicable, and a representative end-to-end acceptance journey. Provider-dependent capabilities keep Sandbox or Live status separate from internal flow acceptance.",
  "",
];

await writeOrCheck("docs/status/capability-delivery-matrix.md", lines.join("\n"));
await writeOrCheck("docs/status/business-acceptance-manifest.json", JSON.stringify({
  scope: "Reference inventory S01-S21; current release scope is docs/production-plan.md",
  note: "Gate defaults are pending; explicit reviews come from capability-gate-reviews.json. Observations never promote gates. Company release acceptance is stored separately in PostgreSQL and cannot be granted by this generator.",
  capabilities: capabilitiesWithReviews.map(capability => ({ ...capability,
    evidenceRecordIds: evidenceLedger.records.filter(record => record.capabilityIds.includes(capability.id)).map(record => record.id),
  })),
}, null, 2) + "\n");
console.log(`${checkOnly?"Verified":"Wrote"} ${capabilities.length} capabilities to docs/status/capability-delivery-matrix.md`);

// Bundled, checksum-verified observations for the read-only readiness dashboard.
// This is workspace simulation evidence, never a Company production sign-off.
await writeOrCheck("src/data/business-readiness.generated.json", JSON.stringify({
  evidenceLedgerSha256:createHash("sha256").update(readFileSync("docs/status/business-acceptance-evidence.json")).digest("hex"),
  capabilities:capabilitiesWithReviews,
  observations:evidenceLedger.records,
},null,2)+"\n");
