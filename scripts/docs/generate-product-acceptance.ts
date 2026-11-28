import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve, relative } from "node:path";
import { moduleRegistry } from "@erpx/platform-catalogue/module-registry";
import { modulePlacement, products, productIds, sharedHostKeys, developmentProductOrigin } from "@erpx/platform-contracts/product-registry";
import { commercialPackages } from "@erpx/commercial-server/domain/packages";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { assertCanonicalRepositoryFilePath, productMigrationReceiptMatchesCurrent, productWorkflowReceiptCases, productWorkflowReceiptIds, productWorkflowReceiptMapping, readProductMigrationReceiptEvidence, requiredProductMigrationCases, validateProductMigrationEvidence, validateProductMigrationReceiptManifest, type ProductMigrationReceiptReference } from "./product-migration-receipts";
import { writeProductAcceptanceOutputs } from "./product-acceptance-output";
import { loadRequiredCases } from "../testing/required-case-inventory";

const root = resolve(import.meta.dirname, "../.."), check = process.argv.includes("--check"), acceptance = process.argv.includes("--acceptance");
type Receipt = ProductMigrationReceiptReference;
const receiptsPath = await assertCanonicalRepositoryFilePath(root, "docs/status/product-migration-receipts-v2.json", { requireFile: true });
const receipts = validateProductMigrationReceiptManifest(JSON.parse(readFileSync(receiptsPath, "utf8")));
const capabilities = (JSON.parse(readFileSync(resolve(root, "platform/catalogue/src/business-readiness.generated.json"), "utf8")) as {
  capabilities: { id: string; label: string; implementation: { ui: string; api: string; service: string; schema: string; test: string } }[];
}).capabilities;
const suites = (JSON.parse(readFileSync(resolve(root, "tests/catalogue/suite-manifest.json"), "utf8")) as { suites: { id: string; lane: string; modules: string[]; files: string[] }[] }).suites;
const packages: Record<keyof typeof modulePlacement, string[]> = {
  brand: commercialPackages.filter(item => item.kind === "base").map(item => item.id), commercial: [], identity_access: commercialPackages.filter(item => item.kind === "base").map(item => item.id), organization: commercialPackages.filter(item => item.kind === "base").map(item => item.id),
  platform_administration: [], manage_erpx: [], customer_360: ["sales", "pos", "pms", "support", "service"], customer_authority: ["sales-portal", "support-portal"],
  support: ["support", "support-portal", "service"], finance: ["finance", "finance-advanced", "pos", "pms", "sales", "supply", "assets", "service", "manufacturing"],
  pos: ["pos", "pos-restaurant", "pos-hotel-charge"], pms: ["pms", "pms-minibar-linen"], sales: ["sales"], crm: ["sales"],
  assets: ["assets"], maintenance: ["assets", "assets-preventive", "assets-parts-costs"], inventory: ["supply", "supply-advanced", "pos", "pms", "sales", "manufacturing", "service", "service-parts-assets"],
  procurement: ["supply"], quality: ["supply-quality", "manufacturing"], operations: ["work", "work-routines"], collaboration: ["work-social", "work-messages"],
  legal: ["legal"], people_payroll: ["people", "people-workforce", "people-payroll", "people-expenses", "people-performance", "projects", "learn"], benefits: ["people-benefits"],
  marketing: ["sales-marketing"], ecommerce: ["sales-ecommerce"], manufacturing: ["manufacturing"], planning: ["manufacturing", "manufacturing-mrp"],
  projects: ["projects", "projects-costs"], learning: ["learn"], talent: ["recruit"], field_service: ["service", "service-parts-assets"], agent: [],
};
type SurfaceBrowserCase = {
  workflow: string;
  runner: "playwright";
  suite: string;
  file: string;
  title: string;
  project: "desktop" | "mobile";
  executionOrigin: "local-simulation-only" | "isolated-forms-host";
};
type Surface = {
  id: string;
  workflows: string[];
  pages: string[];
  apis: string[];
  services?: string[];
  schema?: string[];
  tests?: string[];
  browserCases?: SurfaceBrowserCase[];
};
const formsWorkflow = "tenant-published-version-consent-submit-and-replay";
const formsBrowserTitle = "isolated anonymous Forms host submits the published version and replays exact consent once";
const formsBrowserCases: SurfaceBrowserCase[] = (["desktop", "mobile"] as const).map(project => ({
  workflow: formsWorkflow,
  runner: "playwright",
  suite: "marketing.forms-host.e2e",
  file: "tests/e2e/marketing/forms-host.spec.ts",
  title: formsBrowserTitle,
  project,
  executionOrigin: "isolated-forms-host",
}));
const portalWorkflow = "tenant-token-invitation-document-and-private-files";
const portalExternalSignupTitle = "native retail and customer authority engineering workflows external customer portal invitation signup independently approves a binding verifies a newly created account accepts its one approved document revokes access and retains immutable invitation history";
const portalExternalSignupBrowserCases: SurfaceBrowserCase[] = (["desktop", "mobile"] as const).map(project => ({
  workflow: portalWorkflow,
  runner: "playwright",
  suite: "customer_authority.demo-retail-customer-authority.journeys",
  file: "tests/journeys/customer_authority/demo-retail-customer-authority.spec.ts",
  title: portalExternalSignupTitle,
  project,
  executionOrigin: "local-simulation-only",
}));
const surfaces: Surface[] = [
  { id: "apps", workflows: ["all-company-branch-department-product-selection", "single-product-direct-entry", "dirty-product-switch"], pages: ["platform/workplace/src/entries/workspace.tsx", "apps/launcher/src/app/applications/page.tsx", "platform/workplace/src/routes/choose-work-role/page.tsx"], apis: ["platform/workplace/src/routes/launch/route.ts"] },
  { id: "account", workflows: ["central-login-and-directory-link", "profile-language-theme", "role-company-account-switch", "mfa-expiry-replay-family-all-device-revocation", "authorized-package-area"], pages: ["platform/workplace/src/routes/login/page.tsx", "platform/workplace/src/routes/product-entry/page.tsx"], apis: ["platform/workplace/src/routes/sso/[action]/route.ts", "platform/workplace/src/routes/api/auth/[...all]/route.ts", "products/account/ui/src/routes/api/v1/account/profile/route.ts", "platform/workplace/src/routes/api/v1/account/preferences/route.ts"] },
  { id: "admin", workflows: ["company-brand-organization-iam", "platform-only-entry", "provider-authority-separation", "legacy-preview-apply-and-audit"], pages: ["platform/workplace/src/routes/product-entry/page.tsx", "products/admin/ui/src/routes/provider/commercial/page.tsx", "products/admin/ui/src/routes/provider/commercial/packages/page.tsx"], apis: ["products/admin/ui/src/routes/api/v1/platform/commercial/[[...path]]/route.ts"] },
  { id: "booking", workflows: ["tenant-room-availability-reservation-consent-payment-callback"], pages: ["products/booking/ui/src/routes/book/[holdingSlug]/page.tsx"], apis: ["products/booking/ui/src/routes/api/public/booking/[holdingSlug]/[action]/route.ts"], services: ["modules/pms/server/src/public/public-booking-body.ts"] },
  { id: "portal", workflows: [portalWorkflow], pages: ["products/portal/ui/src/routes/customer-portal/[holdingSlug]/page.tsx", "products/portal/ui/src/routes/customer-portal/[holdingSlug]/tickets/page.tsx", "products/portal/ui/src/routes/customer-portal/enroll/page.tsx", "modules/customer-authority/ui/src/components/support-ticket-portal.tsx", "modules/customer-authority/ui/src/components/customer-portal-enrollment.tsx"], tests: ["tests/integration/customer_authority/customer-access.test.ts", "tests/unit/customer_authority/portal-invitations.test.ts", "tests/unit/shared.business-contracts/customer-portal-application.test.ts", "tests/e2e/customer_authority/portal-invitation.spec.ts", "tests/journeys/customer_authority/demo-retail-customer-authority.spec.ts"], apis: ["products/portal/ui/src/routes/api/customer-portal/[holdingSlug]/route.ts", "products/portal/ui/src/routes/api/customer-portal/enroll/route.ts", "products/portal/ui/src/routes/api/customer-portal/[holdingSlug]/tickets/route.ts", "products/portal/ui/src/routes/api/customer-portal/[holdingSlug]/tickets/[bindingId]/acknowledgements/route.ts", "products/portal/ui/src/routes/api/customer-portal/[holdingSlug]/tickets/[bindingId]/files/[linkId]/content/route.ts"], services: ["modules/customer-authority/server/src/application/portal-invitations.ts", "modules/customer-authority/server/src/application/portal-invitation-delivery.ts", "modules/customer-authority/server/src/domain/portal-invitation.ts", "modules/customer-authority/server/src/adapters/portal-invitations.ts", "modules/customer-authority/server/src/adapters/portal-invitation-delivery.ts", "modules/customer-authority/server/src/adapters/portal-enrollment.ts", "modules/customer-authority/server/src/adapters/portal-invitation-crypto.ts"], schema: ["migrations/postgres/holding-write/1071_customer_portal_invitations.sql"], browserCases: portalExternalSignupBrowserCases },
  { id: "legal-portal", workflows: ["scoped-shared-version-access-and-revocation", "private-document-download-and-access-history"], pages: ["products/legal-portal/ui/src/routes/legal-portal/[holdingSlug]/page.tsx", "platform/workplace/src/routes/legal-portal/enroll/page.tsx", "modules/legal/ui/src/components/legal-portal.tsx"], tests: ["tests/integration/legal/legal.test.ts", "tests/integration/legal/legal-download-route.test.ts", "tests/integration/legal/legal-portal-download-session.test.ts", "tests/unit/legal/legal-command-client.test.ts"], apis: ["products/legal-portal/ui/src/routes/api/legal-portal/[holdingSlug]/[[...path]]/route.ts"] },
  { id: "forms", workflows: [formsWorkflow], pages: ["products/forms/ui/src/routes/forms/[holdingSlug]/[formId]/page.tsx"], apis: ["products/forms/ui/src/routes/api/public/marketing/[holdingSlug]/[formId]/route.ts"], services: ["modules/marketing/server/src/adapters/public-form-body.ts", "modules/marketing/server/src/public/public-form-body.ts"], browserCases: formsBrowserCases },
];
// Existing Company Executive is runtime-owned Company work, distinct from the
// Module catalogue's Platform Executive. It still needs explicit cutover proof.
type ProductWorkflow = {
  id: string;
  product: typeof productIds[number];
  owner: string;
  pages: string[];
  apis: string[];
  services: string[];
  schema?: string[];
  tests: string[];
};
const productWorkflows: ProductWorkflow[] = [{
  // Stable historical receipt identity; the live product and origin are Apps.
  id: "work:company-executive-and-scoped-mcp", product: "apps", owner: "runtime",
  pages: ["products/apps/ui/src/routes/holdings/[holdingSlug]/insights/executive/page.tsx", "products/apps/ui/src/routes/holdings/[holdingSlug]/insights/executive/reports/[reportId]/page.tsx"],
  apis: ["products/apps/ui/src/routes/api/v1/holdings/[holdingSlug]/executive/[[...path]]/route.ts", "products/apps/ui/src/routes/api/v1/holdings/[holdingSlug]/executive/connections/route.ts", "products/apps/ui/src/routes/api/mcp/executive/route.ts"],
  services: ["platform/runtime/server/src/adapters/executive/reports.ts", "platform/runtime/server/src/adapters/executive/mcp.ts", "modules/identity-access/server/src/application/product-workspace.ts", "modules/work-projection/server/src/domain/product-landings.ts"],
  tests: ["tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/workplace-navigation.test.ts", "tests/integration/shared.executive/executive-canvas.test.ts", "tests/integration/shared.executive/executive-mcp.test.ts", "tests/e2e/shared.executive/executive-work-mcp.spec.ts"],
}, {
  id: "finance:sales-invoice-workspace", product: "finance", owner: "finance",
  pages: ["modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/finance/sales/screen.tsx", "modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/sales/new/page.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/finance/sales/new/screen.tsx", "modules/finance/ui/src/components/sales-controls.tsx"],
  apis: ["platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts"],
  services: ["modules/finance/server/src/adapters/sales.ts", "modules/finance/server/src/adapters/document-purchases.ts"],
  schema: ["migrations/postgres/holding-write/1069_finance_sales_standalone_invoice_source.sql", "migrations/postgres/holding-write/1070_finance_sales_standalone_evidence_indexes.sql"],
  tests: ["tests/integration/finance/finance-sales-workspace.test.ts", "tests/e2e/finance/finance-sales-workspace.spec.ts"],
}];
const workflowIds = new Set([...capabilities.map(capability => capability.id), ...surfaces.flatMap(surface => surface.workflows.map(workflow => `${surface.id}:${workflow}`)), ...productWorkflows.map(workflow => workflow.id)]);
const errors: string[] = [];
const requiredCases = loadRequiredCases(root);
const caseInInventory = (expected: { runner: string; file: string; title: string; project?: string }) =>
  requiredCases.some(test => test.runner === expected.runner && test.file === expected.file && test.title === expected.title && test.project === expected.project);
const nonOwnerWorkflowIds = [...surfaces.flatMap(surface => surface.workflows.map(workflow => `${surface.id}:${workflow}`)), ...productWorkflows.map(workflow => workflow.id)];
const receiptWorkflowIds = productWorkflowReceiptIds();
if (new Set(nonOwnerWorkflowIds).size !== nonOwnerWorkflowIds.length || new Set(receiptWorkflowIds).size !== receiptWorkflowIds.length ||
  receiptWorkflowIds.length !== nonOwnerWorkflowIds.length || receiptWorkflowIds.some(id => !nonOwnerWorkflowIds.includes(id)) ||
  nonOwnerWorkflowIds.some(id => !receiptWorkflowIds.includes(id)))
  errors.push("Non-owner product workflows must have exactly one receipt mapping entry");
for (const id of receiptWorkflowIds) {
  try {
    const mapping = productWorkflowReceiptMapping(id);
    const cases = productWorkflowReceiptCases(id);
    for (const expected of cases) {
      if (!caseInInventory(expected)) errors.push(`Product workflow witness is absent from the required case inventory: ${id}/${expected.file}/${expected.title}/${expected.project ?? ""}`);
      const owners = suites.filter(suite => suite.id === expected.suite && suite.files.includes(expected.file) &&
        (expected.runner === "vitest" ? ["unit", "integration"].includes(suite.lane) : ["e2e", "journeys", "uat"].includes(suite.lane)));
      if (owners.length !== 1) errors.push(`Product workflow witness suite binding is invalid: ${id}/${expected.suite}/${expected.file}`);
    }
    if (mapping.native.state === "mapped") {
      const native = requiredProductMigrationCases(id, "implemented").filter(test => test.runner === "vitest");
      if (JSON.stringify(native.map(test => [test.suite, test.file, test.title]).sort()) !==
        JSON.stringify(cases.filter(test => test.runner === "vitest").map(test => [test.suite, test.file, test.title]).sort()))
        errors.push(`Product workflow native map differs from the exact receipt register: ${id}`);
    } else if (!mapping.native.blocker.trim()) errors.push(`Unmapped native workflow requires a concrete blocker: ${id}`);
    if (mapping.browser.state === "mapped") {
      const verified = requiredProductMigrationCases(id, "verified").filter(test => test.runner === "playwright");
      if (JSON.stringify(verified.map(test => [test.suite, test.file, test.title, test.project]).sort()) !==
        JSON.stringify(cases.filter(test => test.runner === "playwright").map(test => [test.suite, test.file, test.title, test.project]).sort()))
        errors.push(`Product workflow browser map differs from the exact receipt register: ${id}`);
    } else if (!mapping.browser.blocker.trim()) errors.push(`Pending browser workflow requires a concrete blocker: ${id}`);
  } catch (error) {
    errors.push(`Product workflow receipt mapping is invalid: ${id}:${error instanceof Error ? error.message : "INVALID"}`);
  }
}
for (const surface of surfaces) {
  const seen = new Set<string>();
  for (const expected of surface.browserCases ?? []) {
    const key = JSON.stringify([expected.workflow, expected.runner, expected.suite, expected.file, expected.title, expected.project]);
    if (!surface.workflows.includes(expected.workflow) || seen.has(key)) errors.push(`Invalid or duplicate surface browser mapping: ${surface.id}/${expected.workflow}/${expected.title}`);
    seen.add(key);
    if (!caseInInventory(expected)) errors.push(`Surface browser witness is absent from the required case inventory: ${surface.id}/${expected.workflow}/${expected.file}/${expected.title}/${expected.project}`);
    const owners = suites.filter(suite => suite.id === expected.suite && ["e2e", "journeys", "uat"].includes(suite.lane) && suite.files.includes(expected.file));
    if (owners.length !== 1) errors.push(`Surface browser witness suite binding is invalid: ${surface.id}/${expected.workflow}/${expected.suite}/${expected.file}`);
  }
  for (const workflow of surface.workflows) {
    const projects = (surface.browserCases ?? []).filter(test => test.workflow === workflow).map(test => test.project).sort();
    if (projects.length && JSON.stringify(projects) !== JSON.stringify(["desktop", "mobile"]))
      errors.push(`Surface browser witness must cover desktop and mobile exactly once: ${surface.id}/${workflow}`);
  }
}
const formsSurface = surfaces.find(surface => surface.id === "forms");
const formsReceiptMapping = productWorkflowReceiptMapping(`forms:${formsWorkflow}`);
if (formsSurface?.browserCases?.length !== 2 || formsSurface.browserCases.some(test => test.workflow !== formsWorkflow ||
  test.executionOrigin !== "isolated-forms-host" || test.suite !== "marketing.forms-host.e2e" ||
  test.file !== "tests/e2e/marketing/forms-host.spec.ts" || test.title !== formsBrowserTitle) ||
  formsReceiptMapping.native.state !== "mapped" || formsReceiptMapping.browser.state !== "mapped")
  errors.push("Forms traceability must map the exact isolated Forms-host browser receipt");
for (const capability of capabilities) {
  try {
    for (const expected of requiredProductMigrationCases(capability.id, "implemented")) {
      if (!caseInInventory(expected)) errors.push("Workflow witness is absent from the required case inventory: " + capability.id + "/" + expected.file + "/" + expected.title);
    }
    try {
      for (const expected of requiredProductMigrationCases(capability.id, "verified")) {
        if (!caseInInventory(expected)) errors.push("Browser witness is absent from the required case inventory: " + capability.id + "/" + expected.file + "/" + expected.title);
      }
    } catch (error) {
      if (!(error instanceof Error) || error.message !== "PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED") throw error;
    }
  } catch (error) {
    errors.push("Workflow witness mapping is invalid: " + capability.id + ":" + (error instanceof Error ? error.message : "INVALID"));
  }
}
const fingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, fingerprint);
const current = { runtimeFingerprint: fingerprint, verificationFingerprint };
const validReceipts: Receipt[] = [];
for (const receipt of receipts.records) {
  const key = `${receipt.id}:${receipt.phase}:${receipt.candidateSha}`;
  if (!workflowIds.has(receipt.id) || receipt.phase === "enabled") errors.push(`Unsupported product receipt phase: ${key}`);
  try {
    const evidenceBytes = await readProductMigrationReceiptEvidence(root, receipt.path, receipt.sha256);
    const evidence = validateProductMigrationEvidence(JSON.parse(evidenceBytes.toString("utf8")));
    if (evidence.id !== receipt.id || evidence.phase !== receipt.phase || evidence.candidateSha !== receipt.candidateSha ||
      evidence.runtimeFingerprint !== receipt.runtimeFingerprint || evidence.verificationFingerprint !== receipt.verificationFingerprint ||
      evidence.release.id !== receipt.release.id || evidence.release.fileManifestSha256 !== receipt.release.fileManifestSha256)
      throw new Error("PRODUCT_MIGRATION_RECEIPT_REFERENCE_MISMATCH");
    validReceipts.push(receipt);
  } catch (error) { errors.push(`Invalid receipt payload: ${key}:${error instanceof Error ? error.message : "INVALID"}`); }
  if (receipt.phase === "verified" && !receipts.records.some(row => row.id === receipt.id && row.phase === "implemented" &&
    row.candidateSha === receipt.candidateSha &&
    row.runtimeFingerprint === receipt.runtimeFingerprint && row.verificationFingerprint === receipt.verificationFingerprint))
    errors.push(`Missing same-source implementation: ${key}`);
  if (receipt.phase === "enabled") errors.push(`Enabled phase lacks independent Company grant and MFA protocol: ${key}`);
}
const currentReceipt = (id: string, phase: string) => validReceipts.find(row => row.id === id && row.phase === phase && productMigrationReceiptMatchesCurrent(row, current));
const state = (id: string) => Object.fromEntries(["implemented", "verified", "enabled"].map(phase => {
  const receipt = currentReceipt(id, phase), previous = [...validReceipts].reverse().find(row => row.id === id && row.phase === phase);
  return [phase, { state: receipt ? "recorded" : previous ? "source-changed" : "pending", evidence: receipt ?? previous ?? null }];
}));
const refs = (value: string) => [...new Set(value.split(/[;,]\s*/).map(path => path.trim()).filter(Boolean))];
const talentDetailPages = [
  "products/recruit/ui/src/routes/holdings/[holdingSlug]/people-organization/talent/[id]/page.tsx",
  "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/talent/[id]/screen.tsx",
  "modules/records/ui/src/components/loaders/modules--talent-console.tsx",
  "modules/talent/ui/src/components/talent-console.tsx",
];
// Current product paths supplement the retained owner catalogue references.
// They share the existing workflow identity and require fresh candidate receipts;
// adding traceability never promotes a historical engineering observation.
const currentProductWorkflowSources: Record<string, Partial<Record<"pages"|"apis"|"services"|"schema"|"tests",string[]>>> = {
  "manufacturing/orders": {
    pages: ["modules/manufacturing/ui/src/components/production-console.tsx"],
    apis: ["modules/manufacturing/ui/src/routes/api/v1/holdings/[holdingSlug]/production/route.ts"],
    services: ["modules/manufacturing/server/src/application/production.ts", "modules/manufacturing/server/src/adapters/package-obligations.ts", "modules/manufacturing/ui/src/components/use-production-command.ts", "modules/manufacturing/ui/src/lib/command-fetch.ts"],
    tests: ["tests/integration/manufacturing/production.test.ts", "tests/unit/planning/production-services.test.ts"],
  },
  "procurement/purchase-orders": {
    pages: ["products/supply/ui/src/routes/holdings/[holdingSlug]/purchasing/setup/page.tsx", "products/supply/ui/src/routes/holdings/[holdingSlug]/purchasing/setup/posting/page.tsx"],
  },
  "projects/feature-projects-wbs": {
    pages: ["modules/projects/ui/src/routes/holdings/[holdingSlug]/projects-routines/projects/settings/page.tsx", "modules/projects/ui/src/routes/holdings/[holdingSlug]/projects-routines/projects/settings/workers/page.tsx", "modules/projects/ui/src/routes/holdings/[holdingSlug]/projects-routines/projects/settings/workers/[personId]/page.tsx", "modules/projects/ui/src/components/worker-startup.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/projects/screen.tsx"],
    apis: ["modules/projects/ui/src/routes/api/v1/holdings/[holdingSlug]/people/project-worker-startup/[[...path]]/route.ts"],
    services: ["modules/people-payroll/server/src/adapters/project-worker-startup.ts", "modules/people-payroll/server/src/adapters/project-worker-startup-unit.ts", "modules/people-payroll/server/src/adapters/project-worker-startup-binding.ts", "modules/people-payroll/server/src/adapters/project-worker-startup-jobs.ts", "modules/identity-access/server/src/adapters/project-worker-startup-subject.ts", "modules/identity-access/server/src/adapters/product-workspace.ts", "modules/work-projection/server/src/adapters/product-launcher.ts"],
    schema: ["modules/people-payroll/server/src/domain/project-worker-startup.ts", "migrations/postgres/holding-write/1063_project_worker_startup_source.sql"],
    tests: ["tests/integration/projects/project-structure.test.ts", "tests/unit/projects/project-services.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "field_service/service-orders": {
    pages: ["products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/page.tsx", "products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/parts/page.tsx", "products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx", "products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/[id]/page.tsx", "modules/field-service/ui/src/components/parts-setup.tsx", "modules/field-service/ui/src/components/initial-physical-stock-count.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/field-service/screen.tsx"],
    apis: ["products/service/ui/src/routes/api/v1/holdings/[holdingSlug]/field-service/parts-setup/[[...path]]/route.ts", "products/service/ui/src/routes/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts"],
    services: ["platform/composition/src/service_parts_posting_setup.rs", "platform/composition/src/service_parts_prerequisite_setup.rs", "platform/composition/src/service_stock_setup.rs", "modules/commerce/server/src/adapters/service_parts_posting.rs", "modules/commerce/server/src/adapters/service_stock_masters.rs", "modules/finance/server/src/adapters/service_parts_prerequisites.rs", "modules/commerce/server/src/adapters/service-parts-posting-setup.ts", "modules/finance/server/src/adapters/approved-sales-posting-defaults.ts", "modules/inventory/server/src/adapters/service-initial-physical-stock-count.ts", "modules/inventory/server/src/adapters/service-initial-physical-stock-count-source.ts", "modules/inventory/server/src/adapters/service-package-obligations.ts"],
    schema: ["modules/commerce/server/src/domain/service_parts_posting.rs", "modules/commerce/server/src/domain/service_stock_masters.rs", "modules/finance/server/src/domain/service_parts_prerequisites.rs", "modules/inventory/server/src/domain/service_initial_physical_count.rs", "migrations/postgres/holding-write/1062_service_parts_initial_physical_stock.sql"],
    tests: ["tests/integration/field_service/field-service.test.ts", "tests/unit/field_service/field-service-application.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "pos/feature-register-session": {
    pages: ["modules/pos/ui/src/routes/holdings/[holdingSlug]/pos/settings/startup/page.tsx", "modules/pos/ui/src/components/pos-startup.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/pos/screen.tsx"],
    apis: ["modules/pos/ui/src/routes/api/v1/holdings/[holdingSlug]/pos/startup/route.ts", "modules/pos/ui/src/routes/api/v1/holdings/[holdingSlug]/pos/startup/[...path]/route.ts"],
    services: ["modules/pos/server/src/adapters/startup.ts", "modules/pos/server/src/adapters/startup-entry.ts", "modules/commerce/server/src/adapters/pos-posting-setup.ts", "modules/commerce/server/src/adapters/pos-startup-masters.ts", "modules/finance/server/src/adapters/pos-startup-prerequisites.ts"],
    schema: ["modules/pos/server/src/domain/startup.ts", "modules/commerce/server/src/domain/pos-posting-setup.ts"],
    tests: ["tests/integration/pos/pos.test.ts", "tests/unit/pos/pos-application.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "inventory/feature-stock-register": {
    pages: ["modules/pos/ui/src/routes/holdings/[holdingSlug]/pos/settings/initial-stock/page.tsx", "modules/pos/ui/src/routes/holdings/[holdingSlug]/pos/settings/initial-stock/[id]/page.tsx", "modules/pos/ui/src/components/pos-initial-physical-stock-count.tsx", "modules/pos/ui/src/components/legacy-count-draft-boundary.tsx", "modules/pos/ui/src/components/legacy-commerce-draft-boundary.tsx", "modules/manufacturing/ui/src/routes/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock/page.tsx", "modules/manufacturing/ui/src/routes/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock/[id]/page.tsx", "modules/inventory/ui/src/components/manufacturing-initial-physical-stock-count.tsx", "modules/sales/ui/src/routes/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock/page.tsx", "modules/sales/ui/src/routes/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock/[id]/page.tsx", "modules/inventory/ui/src/components/initial-physical-stock-count.tsx", "products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx", "products/service/ui/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/[id]/page.tsx", "modules/field-service/ui/src/components/initial-physical-stock-count.tsx"],
    apis: ["modules/pos/ui/src/routes/api/v1/holdings/[holdingSlug]/pos/initial-physical-stock/[[...path]]/route.ts", "modules/manufacturing/ui/src/routes/api/v1/holdings/[holdingSlug]/production/initial-physical-stock/[[...path]]/route.ts", "platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts", "products/service/ui/src/routes/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts"],
    services: ["modules/inventory/server/src/adapters/pos-initial-physical-stock-count.ts", "modules/inventory/server/src/adapters/pos-initial-physical-stock-count-source.ts", "modules/commerce/server/src/adapters/pos-initial-physical-stock-sources.ts", "modules/inventory/server/src/adapters/manufacturing-initial-physical-stock-count.ts", "modules/inventory/server/src/adapters/manufacturing-initial-physical-stock-count-source.ts", "modules/commerce/server/src/adapters/manufacturing-initial-physical-stock-masters.ts", "modules/finance/server/src/adapters/manufacturing-initial-stock-journal-read.ts", "modules/inventory/server/src/adapters/initial-physical-stock-count.ts", "modules/inventory/server/src/adapters/initial-physical-stock-count-source.ts", "modules/inventory/server/src/adapters/sales-package-obligations.ts", "modules/inventory/server/src/adapters/supply-package-obligations.ts", "modules/inventory/server/src/adapters/service-initial-physical-stock-count.ts", "modules/inventory/server/src/adapters/service-initial-physical-stock-count-source.ts"],
    schema: ["modules/inventory/server/src/domain/pos-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1061_pos_initial_physical_stock_count_source.sql", "modules/inventory/server/src/domain/manufacturing-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1059_manufacturing_initial_physical_stock_count_source.sql", "modules/inventory/server/src/domain/initial-physical-stock-count.ts", "migrations/postgres/holding-write/1058_initial_physical_stock_count_source.sql", "modules/inventory/server/src/domain/service_initial_physical_count.rs", "migrations/postgres/holding-write/1062_service_parts_initial_physical_stock.sql"],
    tests: ["tests/integration/pos/pos.test.ts", "tests/unit/inventory/initial-physical-stock-client.test.ts", "tests/integration/manufacturing/production.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/integration/sales/refund-bank-evidence-migration.test.ts", "tests/unit/inventory/initial-physical-stock-transport.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/collaboration/collaboration-services.test.ts", "tests/integration/field_service/field-service.test.ts", "tests/unit/field_service/field-service-application.test.ts"],
  },
  "assets/asset-register": {
    pages: ["modules/assets/ui/src/routes/holdings/[holdingSlug]/services/assets/accounting/page.tsx", "modules/assets/ui/src/routes/holdings/[holdingSlug]/services/assets/accounting/[id]/page.tsx", "modules/assets/ui/src/components/asset-accounting.tsx", "modules/assets/ui/src/components/asset-maintenance-console.tsx", "modules/records/ui/src/components/record-drawer.tsx"],
    apis: ["modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts", "modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts"],
    services: ["modules/finance/server/src/adapters/assets-financial-workflow.ts", "modules/finance/server/src/adapters/assets-financial-unit.ts", "modules/finance/server/src/adapters/assets-posting-prerequisites.ts", "modules/finance/server/src/adapters/finance-assets.ts"],
    schema: ["modules/finance/server/src/domain/assets-financial-workflow.ts", "modules/finance/server/src/domain/assets-posting-prerequisites.ts"],
    tests: ["tests/integration/assets/asset-maintenance.test.ts", "tests/integration/finance/finance-assets.test.ts", "tests/unit/finance/finance-asset-services.test.ts", "tests/unit/shared.business-contracts/asset-maintenance-applications.test.ts"],
  },
  "assets/feature-retirement": {
    pages: ["modules/assets/ui/src/components/asset-accounting.tsx", "modules/assets/ui/src/components/asset-maintenance-console.tsx", "modules/assets/ui/src/components/asset-editor.tsx"],
    apis: ["modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts", "modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts"],
    services: ["modules/finance/server/src/adapters/finance-assets.ts", "modules/finance/server/src/adapters/assets-financial-sources.ts", "modules/assets/server/src/adapters/financial-workflow-sources.ts"],
    tests: ["tests/integration/assets/asset-maintenance.test.ts", "tests/unit/shared.business-contracts/asset-maintenance-applications.test.ts"],
  },
  "talent/feature-hire-handoffs": {
    pages: ["modules/talent/ui/src/components/hire-handoff-editor.tsx", "modules/talent/ui/src/components/hire-pay-group-setup.tsx"],
    apis: ["platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/people/hire-handoffs/[id]/pay-groups/route.ts"],
    services: ["modules/people-payroll/server/src/adapters/hire-handoff-pay-group.ts", "modules/people-payroll/server/src/adapters/hire-handoff-workspace.ts"],
    schema: ["modules/people-payroll/server/src/domain/hire-handoff-pay-group.ts"],
    tests: ["tests/integration/talent/talent.test.ts", "tests/unit/learning/learning-talent-services.test.ts"],
  },
  "talent/applications": {
    pages: talentDetailPages,
  },
  "talent/offers": {
    pages: talentDetailPages,
  },
  "talent/feature-proposals": {
    pages: talentDetailPages,
  },
  "talent/feature-candidates-privacy": {
    pages: ["products/recruit/ui/src/routes/holdings/[holdingSlug]/people-organization/talent/candidates/[id]/privacy/page.tsx", "modules/talent/ui/src/components/candidate-privacy-page.tsx", "modules/talent/ui/src/components/candidate-privacy-editor.tsx"],
    services: ["modules/talent/server/src/public/candidate-privacy-workspace.ts", "modules/talent/server/src/adapters/candidate-privacy-workspace.ts", "modules/talent/server/src/public/people-hire-handoff-facts.ts", "modules/talent/server/src/adapters/people-hire-handoff-facts.ts", "modules/people-payroll/server/src/public/people-handoffs.ts", "modules/people-payroll/server/src/adapters/people-handoffs.ts", "modules/people-payroll/server/src/application/people-handoffs.ts"],
    schema: ["modules/people-payroll/server/src/domain/people-handoffs.ts"],
    tests: ["tests/unit/learning/learning-talent-services.test.ts"],
  },
  "people_payroll/feature-expenses": {
    pages: ["modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/expenses/page.tsx", "modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/expenses/[id]/page.tsx", "modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/expenses/new/page.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/expenses/screen.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/expenses/[id]/screen.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/expenses/new/screen.tsx"],
    tests: ["tests/e2e/people_payroll/expense-v2.spec.ts"],
  },
  "support/tickets": {
    pages: ["modules/support/ui/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/portal/page.tsx", "modules/support/ui/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/portal/[bindingId]/page.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/[id]/screen.tsx", "modules/support/ui/src/components/ticket-portal.tsx"],
    apis: ["modules/support/ui/src/routes/api/v1/holdings/[holdingSlug]/support/ticket-portal/[[...path]]/route.ts"],
    services: ["modules/support/server/src/adapters/ticket-portal.ts", "modules/support/server/src/adapters/ticket-portal-customer.ts", "modules/support/server/src/adapters/ticket-portal-request.ts", "modules/support/server/src/adapters/ticket-portal-availability.ts", "modules/support/server/src/adapters/package-obligations.ts", "modules/files/server/src/adapters/support-ticket-portal-files.ts"],
    schema: ["modules/support/server/src/domain/ticket-portal.ts", "migrations/postgres/holding-write/1065_support_ticket_portal.sql"],
    tests: ["tests/integration/customer_authority/customer-access.test.ts", "tests/unit/support/support-services.test.ts", "tests/unit/shared.business-contracts/customer-portal-application.test.ts", "tests/unit/identity_access/public-surface-entry.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "learning/course-catalog": {
    pages: ["platform/workplace/src/routes/holdings/[holdingSlug]/knowledge/learning/courses/_screen.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/knowledge/learning/assignments/_screen.tsx", "platform/workplace/src/routes/holdings/[holdingSlug]/knowledge/learning/mine/_screen.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/settings/page.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/settings/learners/page.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/settings/learners/[personId]/page.tsx", "modules/learning/ui/src/components/worker-startup.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/page.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/[recordSection]/page.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/page.tsx", "products/learn/ui/src/routes/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/[recordSection]/page.tsx"],
    apis: ["products/learn/ui/src/routes/api/v1/holdings/[holdingSlug]/people/learning-worker-startup/[[...path]]/route.ts"],
    services: ["modules/people-payroll/server/src/adapters/learning-worker-startup.ts", "modules/people-payroll/server/src/adapters/worker-startup.ts", "modules/people-payroll/server/src/adapters/learning-worker-startup-entry.ts", "modules/people-payroll/server/src/adapters/project-worker-startup-jobs.ts", "modules/people-payroll/server/src/adapters/learning-package-obligations.ts"],
    schema: ["modules/people-payroll/server/src/domain/learning-worker-startup.ts", "migrations/postgres/holding-write/1064_learning_worker_startup_source.sql"],
    tests: ["tests/unit/learning/learning-product-screens.test.tsx", "tests/integration/learning/learning.test.ts"],
  },
  "pms/reservations": {
    services: ["modules/pms/server/src/adapters/pms-booking.ts", "modules/pms/server/src/adapters/booking-email-delivery.ts", "platform/runtime/server/src/adapters/runtime/jobs.ts"],
    tests: ["tests/unit/pms/pms-application.test.ts"],
  },
  "manufacturing/feature-costs-genealogy": {
    pages: ["modules/manufacturing/ui/src/components/existing-posting-setup.tsx", "modules/manufacturing/ui/src/routes/holdings/[holdingSlug]/production/manufacturing/settings/page.tsx", "modules/manufacturing/ui/src/routes/holdings/[holdingSlug]/production/manufacturing/settings/posting/page.tsx", "modules/manufacturing/ui/src/routes/holdings/[holdingSlug]/@record/production/manufacturing/settings/posting/page.tsx", "modules/manufacturing/ui/src/components/posting-setup.tsx", "modules/manufacturing/ui/src/components/posting-preparation.tsx"],
    apis: ["modules/manufacturing/ui/src/routes/api/v1/holdings/[holdingSlug]/production/posting-setup/route.ts", "modules/manufacturing/ui/src/routes/api/v1/holdings/[holdingSlug]/production/posting-setup/[...path]/route.ts"],
    services: ["modules/commerce/server/src/adapters/manufacturing-existing-posting-setup.ts", "modules/finance/server/src/adapters/manufacturing-existing-posting-setup.ts", "modules/manufacturing/server/src/adapters/existing-posting-sources.ts", "modules/commerce/server/src/adapters/manufacturing-posting-setup.ts", "modules/finance/server/src/adapters/manufacturing-posting-prerequisites.ts", "modules/manufacturing/server/src/adapters/posting-profiles.ts", "modules/manufacturing/server/src/adapters/posting-setup-entry.ts"],
    schema: ["modules/commerce/server/src/domain/manufacturing-existing-posting-setup.ts", "migrations/postgres/holding-write/1060_manufacturing_existing_posting_confirmation.sql", "modules/commerce/server/src/domain/manufacturing-posting-setup.ts", "modules/finance/server/src/domain/manufacturing-posting-prerequisites.ts", "modules/manufacturing/server/src/domain/posting-profiles.ts"],
    tests: ["tests/integration/manufacturing/production.test.ts", "tests/unit/planning/production-services.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },

  "sales/feature-revenue-handoff": {
    pages: ["modules/sales/ui/src/routes/holdings/[holdingSlug]/customers-sales/sales/settings/page.tsx", "modules/sales/ui/src/routes/holdings/[holdingSlug]/customers-sales/sales/settings/posting/page.tsx", "modules/sales/ui/src/components/sales-posting-setup.tsx", "modules/sales/ui/src/components/sales-goods-posting-setup.tsx"],
    apis: ["platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts"],
    services: ["modules/finance/server/src/adapters/sales-posting-setup.ts", "modules/commerce/server/src/adapters/sales-goods-posting-setup.ts", "modules/finance/server/src/adapters/sales-goods-posting-setup.ts"],
    schema: ["modules/finance/server/src/domain/sales-posting-setup.ts", "modules/commerce/server/src/domain/sales-goods-posting-setup.ts", "migrations/postgres/holding-write/1056_company_service_return_refunds.sql"],
    tests: ["tests/integration/finance/finance-sales-workspace.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/unit/sales/sales-services.test.ts", "tests/unit/commercial/product-registry.test.ts", "tests/integration/sales/company-service-refund-migration.test.ts"],
  },
  "finance/feature-payments": {
    pages: ["modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/documents/sales/receipts/[id]/page.tsx", "modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/cash/payments/[id]/page.tsx", "modules/finance/ui/src/routes/holdings/[holdingSlug]/finance/cash/refunds/[id]/page.tsx", "modules/finance/ui/src/components/refund-console.tsx", "modules/finance/ui/src/components/refund-bank-evidence.tsx"],
    apis: ["platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts"],
    services: ["modules/finance/server/src/adapters/sales-payment-reversal.ts", "modules/finance/server/src/adapters/sales-customer-receipts.ts", "modules/commerce/server/src/adapters/finance-record-sources.ts", "modules/finance/server/src/adapters/refunds.ts", "modules/commerce/server/src/adapters/finance-refund-source.ts", "modules/finance/server/src/adapters/refund-bank-evidence.ts", "modules/finance/server/src/adapters/refund-purchases.ts", "modules/finance/server/src/adapters/purchase-boundary.ts"],
    schema: ["modules/finance/server/src/domain/refund-bank-evidence.ts", "migrations/postgres/holding-write/1057_refund_bank_evidence_consumption.sql"],
    tests: ["tests/integration/finance/finance-sales-workspace.test.ts", "tests/unit/finance/finance-navigation.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/finance/finance-refund-services.test.ts", "tests/unit/finance/refund-bank-evidence-transport.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/integration/sales/company-service-refund-migration.test.ts", "tests/integration/sales/refund-bank-evidence-migration.test.ts"],
  },
};
const workflowRefs = (id:string, category:"pages"|"apis"|"services"|"schema"|"tests", retained:string) =>
  [...new Set([...refs(retained), ...(currentProductWorkflowSources[id]?.[category] ?? [])])];
const owners = JSON.parse(readFileSync(resolve(root, "platform/contracts/src/ownership.json"), "utf8")) as { owners: { id: string; modules: string[] }[] };
const modules = moduleRegistry.map(module => {
  if (!(module.id in modulePlacement) || !packages[module.id as keyof typeof packages]) throw new Error(`UNMAPPED_MODULE:${module.id}`);
  const placement = modulePlacement[module.id as keyof typeof modulePlacement];
  const ownerIds = owners.owners.filter(owner => owner.modules.includes(module.id)).map(owner => owner.id);
  const jobs = ownerIds.flatMap(owner => {
    const directory = resolve(root, "packages", `server-${owner}`, "src");
    return !existsSync(directory) ? [] : readdirSync(directory, { recursive: true, withFileTypes: true }).filter(entry => entry.isFile() && /(?:jobs|scheduled|worker).*\.ts$/.test(entry.name)).map(entry => relative(root, resolve(entry.parentPath, entry.name)));
  });
  const workflows = capabilities.filter(capability => capability.id.split("/")[0] === module.id).map(capability => ({
    id: capability.id, label: capability.label, pages: workflowRefs(capability.id,"pages",capability.implementation.ui), apis: workflowRefs(capability.id,"apis",capability.implementation.api),
    services: workflowRefs(capability.id,"services",capability.implementation.service), schema: workflowRefs(capability.id,"schema",capability.implementation.schema), tests: workflowRefs(capability.id,"tests",capability.implementation.test), acceptance: state(capability.id),
  }));
  return { moduleId: module.id, placement, packages: packages[module.id as keyof typeof packages],
    packageMappingPurpose: "traceability only; does not authorize a grant or dependency management UI",
    landing: products.find(product => product.id === placement)?.landing ?? null,
    productOrigin: developmentProductOrigin(placement === "provider" ? "admin" : placement === "shared" ? "apps" : placement),
    jobs: [...new Set(jobs)].sort(), jobDispatcher: "platform/runtime/server/src/adapters/runtime/jobs.ts", workflows,
    regressionSuites: suites.filter(suite => suite.modules.includes(module.id)).map(suite => suite.id) };
});
for (const row of modules) {
  if (!row.workflows.length) errors.push(`No workflows: ${row.moduleId}`);
  for (const id of row.packages) if (!commercialPackages.some(definition => definition.id === id)) errors.push(`Unknown package: ${row.moduleId}/${id}`);
  for (const workflow of row.workflows) for (const category of ["pages", "apis", "services", "schema", "tests"] as const) {
    if (!workflow[category].length) errors.push(`Missing ${category}: ${workflow.id}`);
    for (const path of workflow[category]) if (!existsSync(resolve(root, path))) errors.push(`Missing workflow source: ${workflow.id}/${path}`);
  }
}
for (const surface of surfaces) for (const path of [...surface.pages, ...surface.apis, ...(surface.services ?? []), ...(surface.schema ?? []), ...(surface.tests ?? [])]) if (!existsSync(resolve(root, path))) errors.push(`Missing surface source: ${surface.id}/${path}`);
for (const workflow of productWorkflows) {
  if (!productIds.some(id => id === workflow.product)) errors.push(`Unknown product workflow: ${workflow.id}`);
  for (const path of [...workflow.pages, ...workflow.apis, ...workflow.services, ...(workflow.schema ?? []), ...workflow.tests]) if (!existsSync(resolve(root, path))) errors.push(`Missing product workflow source: ${workflow.id}/${path}`);
}
if (modules.length !== 33 || new Set(modules.map(row => row.moduleId)).size !== 33 || surfaces.length !== sharedHostKeys.length + 1) errors.push("INCOMPLETE_PRODUCT_COVERAGE");
const register = { version: 3, environment: "development", deliveryPlan: "docs/production-plan.md",
  sourceFingerprint: fingerprint, verificationFingerprint,
  baseline: { source: ".scratch/dev/product-migration-baseline-2026-10-14/", privateAuthority: "read-only per-database snapshots; no secrets or personal authority rows in this register", performance: "compiled baseline retained; observations pending" },
  modules, productWorkflows: productWorkflows.map(workflow => ({ ...workflow, origin: developmentProductOrigin(workflow.product as typeof productIds[number]),
    acceptance: state(workflow.id), receiptMapping: productWorkflowReceiptMapping(workflow.id) })),
  surfaces: surfaces.map(surface => ({ ...surface, origin: developmentProductOrigin(surface.id as typeof sharedHostKeys[number]),
    workflows: surface.workflows.map(id => ({ id: `${surface.id}:${id}`, acceptance: state(`${surface.id}:${id}`), receiptMapping: productWorkflowReceiptMapping(`${surface.id}:${id}`) })) })) };
const target = await assertCanonicalRepositoryFilePath(root, "docs/status/product-migration-acceptance.json");
// Current-source status follows runtime and verification fingerprints. Candidate
// SHAs remain receipt provenance and exact-import bindings, not register identity.
const bytes = JSON.stringify(register, null, 2) + "\n";
if (check || acceptance) {
  if (!existsSync(target) || readFileSync(target, "utf8") !== bytes) errors.push("PRODUCT_ACCEPTANCE_REGISTER_STALE");
}
const plan = await assertCanonicalRepositoryFilePath(root, "docs/production-plan.md", { requireFile: true }), planSource = readFileSync(plan, "utf8");
const begin = "<!-- product-migration-coverage:start -->", end = "<!-- product-migration-coverage:end -->";
if (!planSource.includes(begin) || !planSource.includes(end)) errors.push("PRODUCT_COVERAGE_PLAN_MARKERS_REQUIRED");
const progress = (ids: string[], phase: Receipt["phase"]) => `${ids.filter(id => Boolean(currentReceipt(id, phase))).length}/${ids.length}`;
const table = [begin, "", "| Module | Product / shared area | Owner workflows | Implemented | Verified | Enabled |",
  "| --- | --- | ---: | ---: | ---: | ---: |",
  ...modules.map(module => { const ids = module.workflows.map(workflow => workflow.id); return `| ${module.moduleId} | ${module.placement} | ${ids.length} | ${progress(ids, "implemented")} | ${progress(ids, "verified")} | ${progress(ids, "enabled")} |`; }),
  "", "| Product workflow outside Module catalogue | Product | Implemented | Verified | Enabled |",
  "| --- | --- | ---: | ---: | ---: |",
  ...productWorkflows.map(workflow => `| ${workflow.id} | ${workflow.product} | ${progress([workflow.id], "implemented")} | ${progress([workflow.id], "verified")} | ${progress([workflow.id], "enabled")} |`),
  "", end].join("\n");
const nextPlan = planSource.replace(new RegExp(`${begin}[\\s\\S]*?${end}`), table);
if (check || acceptance) { if (nextPlan !== planSource) errors.push("PRODUCT_COVERAGE_PLAN_STALE"); }
if (acceptance) for (const id of workflowIds) for (const phase of ["implemented", "verified", "enabled"] as const) {
  if (!currentReceipt(id, phase)) errors.push(`Pending current-source ${phase}: ${id}`);
}
if (check || acceptance) writeProductAcceptanceOutputs(errors, []);
else writeProductAcceptanceOutputs(errors, [
  { path: target, contents: bytes },
  ...(nextPlan !== planSource ? [{ path: plan, contents: nextPlan }] : []),
]);
const receiptMappingCounts = receiptWorkflowIds.map(id => productWorkflowReceiptMapping(id));
const nativeMapped = receiptMappingCounts.filter(mapping => mapping.native.state === "mapped").length;
const browserMapped = receiptMappingCounts.filter(mapping => mapping.browser.state === "mapped").length;
console.log(`PRODUCT_ACCEPTANCE_REGISTER:${modules.length} modules/${capabilities.length} owner workflows/${receiptWorkflowIds.length} non-owner workflow maps (${nativeMapped} native mapped/${receiptWorkflowIds.length - nativeMapped} blocked; ${browserMapped} browser mapped/${receiptWorkflowIds.length - browserMapped} pending)/${surfaces.length} shared-public surfaces; current-source receipts ${validReceipts.filter(row => productMigrationReceiptMatchesCurrent(row, current)).length}.`);
