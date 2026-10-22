import { existsSync, readFileSync, readdirSync } from "node:fs";
import { resolve, relative } from "node:path";
import { moduleRegistry } from "../../src/data/module-registry";
import { modulePlacement, products, productIds, sharedHostKeys, developmentProductOrigin } from "../../src/lib/commercial/product-registry";
import { commercialPackages } from "../../src/server/modules/commercial/domain/packages";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { assertCanonicalRepositoryFilePath, productMigrationReceiptMatchesCurrent, productWorkflowReceiptCases, productWorkflowReceiptIds, productWorkflowReceiptMapping, readProductMigrationReceiptEvidence, requiredProductMigrationCases, validateProductMigrationEvidence, validateProductMigrationReceiptManifest, type ProductMigrationReceiptReference } from "./product-migration-receipts";
import { writeProductAcceptanceOutputs } from "./product-acceptance-output";
import { loadRequiredCases } from "../testing/required-case-inventory";

const root = resolve(import.meta.dirname, "../.."), check = process.argv.includes("--check"), acceptance = process.argv.includes("--acceptance");
type Receipt = ProductMigrationReceiptReference;
const receiptsPath = await assertCanonicalRepositoryFilePath(root, "docs/status/product-migration-receipts-v2.json", { requireFile: true });
const receipts = validateProductMigrationReceiptManifest(JSON.parse(readFileSync(receiptsPath, "utf8")));
const capabilities = (JSON.parse(readFileSync(resolve(root, "src/data/business-readiness.generated.json"), "utf8")) as {
  capabilities: { id: string; label: string; implementation: { ui: string; api: string; service: string; schema: string; test: string } }[];
}).capabilities;
const suites = (JSON.parse(readFileSync(resolve(root, "tests/catalogue/suite-manifest.json"), "utf8")) as { suites: { id: string; lane: string; modules: string[]; files: string[] }[] }).suites;
const packages: Record<keyof typeof modulePlacement, string[]> = {
  brand: [...productIds], commercial: [], identity_access: [...productIds], organization: [...productIds],
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
  { id: "apps", workflows: ["all-company-branch-department-product-selection", "single-product-direct-entry", "dirty-product-switch"], pages: ["src/app/product-entry/page.tsx", "src/app/choose-work-role/page.tsx"], apis: ["src/app/launch/route.ts"] },
  { id: "account", workflows: ["central-login-and-directory-link", "profile-language-theme", "role-company-account-switch", "mfa-expiry-replay-family-all-device-revocation", "authorized-package-area"], pages: ["src/app/login/page.tsx", "src/app/product-entry/page.tsx"], apis: ["src/app/sso/[action]/route.ts", "src/app/api/auth/[...all]/route.ts", "src/app/api/v1/account/profile/route.ts", "src/app/api/v1/account/preferences/route.ts"] },
  { id: "admin", workflows: ["company-brand-organization-iam", "platform-only-entry", "provider-authority-separation", "legacy-preview-apply-and-audit"], pages: ["src/app/product-entry/page.tsx", "src/app/provider/commercial/page.tsx", "src/app/provider/commercial/packages/page.tsx"], apis: ["src/app/api/v1/platform/commercial/[[...path]]/route.ts"] },
  { id: "booking", workflows: ["tenant-room-availability-reservation-consent-payment-callback"], pages: ["src/app/book/[holdingSlug]/page.tsx"], apis: ["src/app/api/public/booking/[holdingSlug]/[action]/route.ts"], services: ["src/server/modules/pms/public/public-booking-body.ts"] },
  { id: "portal", workflows: [portalWorkflow], pages: ["src/app/customer-portal/[holdingSlug]/page.tsx", "src/app/customer-portal/[holdingSlug]/tickets/page.tsx", "src/app/customer-portal/enroll/page.tsx", "src/components/customer_authority/support-ticket-portal.tsx", "src/components/customer_authority/customer-portal-enrollment.tsx"], tests: ["tests/integration/customer_authority/customer-access.test.ts", "tests/unit/customer_authority/portal-invitations.test.ts", "tests/unit/shared.business-contracts/customer-portal-application.test.ts", "tests/e2e/customer_authority/portal-invitation.spec.ts", "tests/journeys/customer_authority/demo-retail-customer-authority.spec.ts"], apis: ["src/app/api/customer-portal/[holdingSlug]/route.ts", "src/app/api/customer-portal/enroll/route.ts", "src/app/api/customer-portal/[holdingSlug]/tickets/route.ts", "src/app/api/customer-portal/[holdingSlug]/tickets/[bindingId]/acknowledgements/route.ts", "src/app/api/customer-portal/[holdingSlug]/tickets/[bindingId]/files/[linkId]/content/route.ts"], services: ["src/server/modules/customer_authority/application/portal-invitations.ts", "src/server/modules/customer_authority/application/portal-invitation-delivery.ts", "src/server/modules/customer_authority/domain/portal-invitation.ts", "src/server/modules/customer_authority/adapters/portal-invitations.ts", "src/server/modules/customer_authority/adapters/portal-invitation-delivery.ts", "src/server/modules/customer_authority/adapters/portal-enrollment.ts", "src/server/modules/customer_authority/adapters/portal-invitation-crypto.ts"], schema: ["migrations/postgres/holding-write/1071_customer_portal_invitations.sql"], browserCases: portalExternalSignupBrowserCases },
  { id: "legal-portal", workflows: ["scoped-shared-version-access-and-revocation", "private-document-download-and-access-history"], pages: ["src/app/legal-portal/[holdingSlug]/page.tsx", "src/app/legal-portal/enroll/page.tsx", "src/components/legal/legal-portal.tsx"], tests: ["tests/integration/legal/legal.test.ts", "tests/integration/legal/legal-download-route.test.ts", "tests/integration/legal/legal-portal-download-session.test.ts", "tests/unit/legal/legal-command-client.test.ts"], apis: ["src/app/api/legal-portal/[holdingSlug]/[[...path]]/route.ts"] },
  { id: "forms", workflows: [formsWorkflow], pages: ["src/app/forms/[holdingSlug]/[formId]/page.tsx"], apis: ["src/app/api/public/marketing/[holdingSlug]/[formId]/route.ts"], services: ["src/server/modules/marketing/adapters/public-form-body.ts", "src/server/modules/marketing/public/public-form-body.ts"], browserCases: formsBrowserCases },
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
  id: "work:company-executive-and-scoped-mcp", product: "work", owner: "runtime",
  pages: ["src/app/holdings/[holdingSlug]/insights/executive/page.tsx", "src/app/holdings/[holdingSlug]/insights/executive/reports/[reportId]/page.tsx"],
  apis: ["src/app/api/v1/holdings/[holdingSlug]/executive/[[...path]]/route.ts", "src/app/api/v1/holdings/[holdingSlug]/executive/connections/route.ts", "src/app/api/mcp/executive/route.ts"],
  services: ["src/server/modules/runtime/adapters/executive/reports.ts", "src/server/modules/runtime/adapters/executive/mcp.ts", "src/server/modules/identity_access/application/product-workspace.ts", "src/server/modules/work_projection/domain/product-landings.ts"],
  tests: ["tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/workplace-navigation.test.ts", "tests/integration/shared.executive/executive-canvas.test.ts", "tests/integration/shared.executive/executive-mcp.test.ts", "tests/e2e/shared.executive/executive-work-mcp.spec.ts"],
}, {
  id: "finance:sales-invoice-workspace", product: "finance", owner: "finance",
  pages: ["src/app/holdings/[holdingSlug]/finance/documents/sales/invoices/page.tsx", "src/app/holdings/[holdingSlug]/_presenters/finance/sales/screen.tsx", "src/app/holdings/[holdingSlug]/finance/documents/sales/new/page.tsx", "src/app/holdings/[holdingSlug]/_presenters/finance/sales/new/screen.tsx", "src/components/finance/sales-controls.tsx"],
  apis: ["src/app/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts"],
  services: ["src/server/modules/finance/adapters/sales.ts", "src/server/modules/finance/adapters/document-purchases.ts"],
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
  "src/app/holdings/[holdingSlug]/people-organization/talent/[id]/page.tsx",
  "src/app/holdings/[holdingSlug]/_presenters/talent/[id]/screen.tsx",
  "src/components/records/loaders/modules--talent-console.tsx",
  "src/components/talent/talent-console.tsx",
];
// Current product paths supplement the retained owner catalogue references.
// They share the existing workflow identity and require fresh candidate receipts;
// adding traceability never promotes a historical engineering observation.
const currentProductWorkflowSources: Record<string, Partial<Record<"pages"|"apis"|"services"|"schema"|"tests",string[]>>> = {
  "manufacturing/orders": {
    pages: ["src/components/manufacturing/production-console.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/production/route.ts"],
    services: ["src/server/modules/manufacturing/application/production.ts", "src/server/modules/manufacturing/adapters/package-obligations.ts", "src/components/manufacturing/use-production-command.ts", "src/lib/manufacturing/command-fetch.ts"],
    tests: ["tests/integration/manufacturing/production.test.ts", "tests/unit/planning/production-services.test.ts"],
  },
  "procurement/purchase-orders": {
    pages: ["src/app/holdings/[holdingSlug]/purchasing/setup/page.tsx", "src/app/holdings/[holdingSlug]/purchasing/setup/posting/page.tsx"],
  },
  "projects/feature-projects-wbs": {
    pages: ["src/app/holdings/[holdingSlug]/projects-routines/projects/settings/page.tsx", "src/app/holdings/[holdingSlug]/projects-routines/projects/settings/workers/page.tsx", "src/app/holdings/[holdingSlug]/projects-routines/projects/settings/workers/[personId]/page.tsx", "src/components/projects/worker-startup.tsx", "src/app/holdings/[holdingSlug]/_presenters/projects/screen.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/people/project-worker-startup/[[...path]]/route.ts"],
    services: ["src/server/modules/people_payroll/adapters/project-worker-startup.ts", "src/server/modules/people_payroll/adapters/project-worker-startup-unit.ts", "src/server/modules/people_payroll/adapters/project-worker-startup-binding.ts", "src/server/modules/people_payroll/adapters/project-worker-startup-jobs.ts", "src/server/modules/identity_access/adapters/project-worker-startup-subject.ts", "src/server/modules/identity_access/adapters/product-workspace.ts", "src/server/modules/work_projection/adapters/product-launcher.ts"],
    schema: ["src/server/modules/people_payroll/domain/project-worker-startup.ts", "migrations/postgres/holding-write/1063_project_worker_startup_source.sql"],
    tests: ["tests/integration/projects/project-structure.test.ts", "tests/unit/projects/project-services.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "field_service/service-orders": {
    pages: ["src/app/holdings/[holdingSlug]/services/field-service/settings/page.tsx", "src/app/holdings/[holdingSlug]/services/field-service/settings/parts/page.tsx", "src/app/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx", "src/app/holdings/[holdingSlug]/services/field-service/settings/initial-stock/[id]/page.tsx", "src/components/field_service/parts-setup.tsx", "src/components/field_service/initial-physical-stock-count.tsx", "src/app/holdings/[holdingSlug]/_presenters/field-service/screen.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/field-service/parts-setup/[[...path]]/route.ts", "src/app/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts"],
    services: ["src/server/modules/commerce/adapters/service-parts-posting-setup.ts", "src/server/modules/commerce/adapters/service-parts-posting-sources.ts", "src/server/modules/commerce/adapters/service-initial-physical-stock-masters.ts", "src/server/modules/finance/adapters/service-parts-prerequisites.ts", "src/server/modules/finance/adapters/service-parts-posting-sources.ts", "src/server/modules/finance/adapters/approved-sales-posting-defaults.ts", "src/server/modules/inventory/adapters/service-initial-physical-stock-count.ts", "src/server/modules/inventory/adapters/service-initial-physical-stock-count-source.ts", "src/server/modules/inventory/adapters/service-package-obligations.ts"],
    schema: ["src/server/modules/commerce/domain/service-parts-posting-setup.ts", "src/server/modules/inventory/domain/service-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1062_service_parts_initial_physical_stock.sql"],
    tests: ["tests/integration/field_service/field-service.test.ts", "tests/unit/field_service/field-service-application.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "pos/feature-register-session": {
    pages: ["src/app/holdings/[holdingSlug]/pos/settings/startup/page.tsx", "src/components/pos/pos-startup.tsx", "src/app/holdings/[holdingSlug]/_presenters/pos/screen.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/pos/startup/route.ts", "src/app/api/v1/holdings/[holdingSlug]/pos/startup/[...path]/route.ts"],
    services: ["src/server/modules/pos/adapters/startup.ts", "src/server/modules/pos/adapters/startup-entry.ts", "src/server/modules/commerce/adapters/pos-posting-setup.ts", "src/server/modules/commerce/adapters/pos-startup-masters.ts", "src/server/modules/finance/adapters/pos-startup-prerequisites.ts"],
    schema: ["src/server/modules/pos/domain/startup.ts", "src/server/modules/commerce/domain/pos-posting-setup.ts"],
    tests: ["tests/integration/pos/pos.test.ts", "tests/unit/pos/pos-application.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "inventory/feature-stock-register": {
    pages: ["src/app/holdings/[holdingSlug]/pos/settings/initial-stock/page.tsx", "src/app/holdings/[holdingSlug]/pos/settings/initial-stock/[id]/page.tsx", "src/components/pos/pos-initial-physical-stock-count.tsx", "src/components/pos/legacy-count-draft-boundary.tsx", "src/components/pos/legacy-commerce-draft-boundary.tsx", "src/app/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock/page.tsx", "src/app/holdings/[holdingSlug]/production/manufacturing/settings/initial-stock/[id]/page.tsx", "src/components/inventory/manufacturing-initial-physical-stock-count.tsx", "src/app/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock/page.tsx", "src/app/holdings/[holdingSlug]/customers-sales/sales/settings/initial-stock/[id]/page.tsx", "src/components/inventory/initial-physical-stock-count.tsx", "src/app/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page.tsx", "src/app/holdings/[holdingSlug]/services/field-service/settings/initial-stock/[id]/page.tsx", "src/components/field_service/initial-physical-stock-count.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/pos/initial-physical-stock/[[...path]]/route.ts", "src/app/api/v1/holdings/[holdingSlug]/production/initial-physical-stock/[[...path]]/route.ts", "src/app/api/v1/holdings/[holdingSlug]/stock-counts/[[...path]]/route.ts", "src/app/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts"],
    services: ["src/server/modules/inventory/adapters/pos-initial-physical-stock-count.ts", "src/server/modules/inventory/adapters/pos-initial-physical-stock-count-source.ts", "src/server/modules/commerce/adapters/pos-initial-physical-stock-sources.ts", "src/server/modules/inventory/adapters/manufacturing-initial-physical-stock-count.ts", "src/server/modules/inventory/adapters/manufacturing-initial-physical-stock-count-source.ts", "src/server/modules/commerce/adapters/manufacturing-initial-physical-stock-masters.ts", "src/server/modules/finance/adapters/manufacturing-initial-stock-journal-read.ts", "src/server/modules/inventory/adapters/initial-physical-stock-count.ts", "src/server/modules/inventory/adapters/initial-physical-stock-count-source.ts", "src/server/modules/inventory/adapters/sales-package-obligations.ts", "src/server/modules/inventory/adapters/supply-package-obligations.ts", "src/server/modules/inventory/adapters/service-initial-physical-stock-count.ts", "src/server/modules/inventory/adapters/service-initial-physical-stock-count-source.ts"],
    schema: ["src/server/modules/inventory/domain/pos-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1061_pos_initial_physical_stock_count_source.sql", "src/server/modules/inventory/domain/manufacturing-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1059_manufacturing_initial_physical_stock_count_source.sql", "src/server/modules/inventory/domain/initial-physical-stock-count.ts", "migrations/postgres/holding-write/1058_initial_physical_stock_count_source.sql", "src/server/modules/inventory/domain/service-initial-physical-stock-count.ts", "migrations/postgres/holding-write/1062_service_parts_initial_physical_stock.sql"],
    tests: ["tests/integration/pos/pos.test.ts", "tests/unit/inventory/initial-physical-stock-client.test.ts", "tests/integration/manufacturing/production.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/integration/sales/refund-bank-evidence-migration.test.ts", "tests/unit/inventory/initial-physical-stock-transport.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/collaboration/collaboration-services.test.ts", "tests/integration/field_service/field-service.test.ts", "tests/unit/field_service/field-service-application.test.ts"],
  },
  "assets/asset-register": {
    pages: ["src/app/holdings/[holdingSlug]/services/assets/accounting/page.tsx", "src/app/holdings/[holdingSlug]/services/assets/accounting/[id]/page.tsx", "src/components/assets/asset-accounting.tsx", "src/components/assets/asset-maintenance-console.tsx", "src/components/records/record-drawer.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts", "src/app/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts"],
    services: ["src/server/modules/finance/adapters/assets-financial-workflow.ts", "src/server/modules/finance/adapters/assets-financial-unit.ts", "src/server/modules/finance/adapters/assets-posting-prerequisites.ts", "src/server/modules/finance/adapters/finance-assets.ts"],
    schema: ["src/server/modules/finance/domain/assets-financial-workflow.ts", "src/server/modules/finance/domain/assets-posting-prerequisites.ts"],
    tests: ["tests/integration/assets/asset-maintenance.test.ts", "tests/integration/finance/finance-assets.test.ts", "tests/unit/finance/finance-asset-services.test.ts", "tests/unit/shared.business-contracts/asset-maintenance-applications.test.ts"],
  },
  "assets/feature-retirement": {
    pages: ["src/components/assets/asset-accounting.tsx", "src/components/assets/asset-maintenance-console.tsx", "src/components/assets/asset-editor.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts", "src/app/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts"],
    services: ["src/server/modules/finance/adapters/finance-assets.ts", "src/server/modules/finance/adapters/assets-financial-sources.ts", "src/server/modules/assets/adapters/financial-workflow-sources.ts"],
    tests: ["tests/integration/assets/asset-maintenance.test.ts", "tests/unit/shared.business-contracts/asset-maintenance-applications.test.ts"],
  },
  "talent/feature-hire-handoffs": {
    pages: ["src/components/talent/hire-handoff-editor.tsx", "src/components/talent/hire-pay-group-setup.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/people/hire-handoffs/[id]/pay-groups/route.ts"],
    services: ["src/server/modules/people_payroll/adapters/hire-handoff-pay-group.ts", "src/server/modules/people_payroll/adapters/hire-handoff-workspace.ts"],
    schema: ["src/server/modules/people_payroll/domain/hire-handoff-pay-group.ts"],
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
    pages: ["src/app/holdings/[holdingSlug]/people-organization/talent/candidates/[id]/privacy/page.tsx", "src/components/talent/candidate-privacy-page.tsx", "src/components/talent/candidate-privacy-editor.tsx"],
    services: ["src/server/modules/talent/public/candidate-privacy-workspace.ts", "src/server/modules/talent/adapters/candidate-privacy-workspace.ts", "src/server/modules/talent/public/people-hire-handoff-facts.ts", "src/server/modules/talent/adapters/people-hire-handoff-facts.ts", "src/server/modules/people_payroll/public/people-handoffs.ts", "src/server/modules/people_payroll/adapters/people-handoffs.ts", "src/server/modules/people_payroll/application/people-handoffs.ts"],
    schema: ["src/server/modules/people_payroll/domain/people-handoffs.ts"],
    tests: ["tests/unit/learning/learning-talent-services.test.ts"],
  },
  "people_payroll/feature-expenses": {
    pages: ["src/app/holdings/[holdingSlug]/finance/documents/expenses/page.tsx", "src/app/holdings/[holdingSlug]/finance/documents/expenses/[id]/page.tsx", "src/app/holdings/[holdingSlug]/finance/documents/expenses/new/page.tsx", "src/app/holdings/[holdingSlug]/_presenters/expenses/screen.tsx", "src/app/holdings/[holdingSlug]/_presenters/expenses/[id]/screen.tsx", "src/app/holdings/[holdingSlug]/_presenters/expenses/new/screen.tsx"],
    tests: ["tests/e2e/people_payroll/expense-v2.spec.ts"],
  },
  "support/tickets": {
    pages: ["src/app/holdings/[holdingSlug]/customers-sales/customers/service/portal/page.tsx", "src/app/holdings/[holdingSlug]/customers-sales/customers/service/portal/[bindingId]/page.tsx", "src/app/holdings/[holdingSlug]/_presenters/support/[id]/screen.tsx", "src/components/support/ticket-portal.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/support/ticket-portal/[[...path]]/route.ts"],
    services: ["src/server/modules/support/adapters/ticket-portal.ts", "src/server/modules/support/adapters/ticket-portal-customer.ts", "src/server/modules/support/adapters/ticket-portal-request.ts", "src/server/modules/support/adapters/ticket-portal-availability.ts", "src/server/modules/support/adapters/package-obligations.ts", "src/server/modules/files/adapters/support-ticket-portal-files.ts"],
    schema: ["src/server/modules/support/domain/ticket-portal.ts", "migrations/postgres/holding-write/1065_support_ticket_portal.sql"],
    tests: ["tests/integration/customer_authority/customer-access.test.ts", "tests/unit/support/support-services.test.ts", "tests/unit/shared.business-contracts/customer-portal-application.test.ts", "tests/unit/identity_access/public-surface-entry.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },
  "learning/course-catalog": {
    pages: ["src/app/holdings/[holdingSlug]/knowledge/learning/courses/_screen.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/assignments/_screen.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/mine/_screen.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/settings/page.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/settings/learners/page.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/settings/learners/[personId]/page.tsx", "src/components/learning/worker-startup.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/page.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/[recordSection]/page.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/page.tsx", "src/app/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/[recordSection]/page.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/people/learning-worker-startup/[[...path]]/route.ts"],
    services: ["src/server/modules/people_payroll/adapters/learning-worker-startup.ts", "src/server/modules/people_payroll/adapters/worker-startup.ts", "src/server/modules/people_payroll/adapters/learning-worker-startup-entry.ts", "src/server/modules/people_payroll/adapters/project-worker-startup-jobs.ts", "src/server/modules/people_payroll/adapters/learning-package-obligations.ts"],
    schema: ["src/server/modules/people_payroll/domain/learning-worker-startup.ts", "migrations/postgres/holding-write/1064_learning_worker_startup_source.sql"],
    tests: ["tests/unit/learning/learning-product-screens.test.tsx", "tests/integration/learning/learning.test.ts"],
  },
  "pms/reservations": {
    services: ["src/server/modules/pms/adapters/pms-booking.ts", "src/server/modules/pms/adapters/booking-email-delivery.ts", "src/server/modules/runtime/adapters/runtime/jobs.ts"],
    tests: ["tests/unit/pms/pms-application.test.ts"],
  },
  "manufacturing/feature-costs-genealogy": {
    pages: ["src/components/manufacturing/existing-posting-setup.tsx", "src/app/holdings/[holdingSlug]/production/manufacturing/settings/page.tsx", "src/app/holdings/[holdingSlug]/production/manufacturing/settings/posting/page.tsx", "src/app/holdings/[holdingSlug]/@record/production/manufacturing/settings/posting/page.tsx", "src/components/manufacturing/posting-setup.tsx", "src/components/manufacturing/posting-preparation.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/production/posting-setup/route.ts", "src/app/api/v1/holdings/[holdingSlug]/production/posting-setup/[...path]/route.ts"],
    services: ["src/server/modules/commerce/adapters/manufacturing-existing-posting-setup.ts", "src/server/modules/finance/adapters/manufacturing-existing-posting-setup.ts", "src/server/modules/manufacturing/adapters/existing-posting-sources.ts", "src/server/modules/commerce/adapters/manufacturing-posting-setup.ts", "src/server/modules/finance/adapters/manufacturing-posting-prerequisites.ts", "src/server/modules/manufacturing/adapters/posting-profiles.ts", "src/server/modules/manufacturing/adapters/posting-setup-entry.ts"],
    schema: ["src/server/modules/commerce/domain/manufacturing-existing-posting-setup.ts", "migrations/postgres/holding-write/1060_manufacturing_existing_posting_confirmation.sql", "src/server/modules/commerce/domain/manufacturing-posting-setup.ts", "src/server/modules/finance/domain/manufacturing-posting-prerequisites.ts", "src/server/modules/manufacturing/domain/posting-profiles.ts"],
    tests: ["tests/integration/manufacturing/production.test.ts", "tests/unit/planning/production-services.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/shared.ui/product-launcher.test.ts", "tests/unit/commercial/product-registry.test.ts"],
  },

  "sales/feature-revenue-handoff": {
    pages: ["src/app/holdings/[holdingSlug]/customers-sales/sales/settings/page.tsx", "src/app/holdings/[holdingSlug]/customers-sales/sales/settings/posting/page.tsx", "src/components/sales/sales-posting-setup.tsx", "src/components/sales/sales-goods-posting-setup.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/commerce/[...path]/route.ts"],
    services: ["src/server/modules/finance/adapters/sales-posting-setup.ts", "src/server/modules/commerce/adapters/sales-goods-posting-setup.ts", "src/server/modules/finance/adapters/sales-goods-posting-setup.ts"],
    schema: ["src/server/modules/finance/domain/sales-posting-setup.ts", "src/server/modules/commerce/domain/sales-goods-posting-setup.ts", "migrations/postgres/holding-write/1056_company_service_return_refunds.sql"],
    tests: ["tests/integration/finance/finance-sales-workspace.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/unit/sales/sales-services.test.ts", "tests/unit/commercial/product-registry.test.ts", "tests/integration/sales/company-service-refund-migration.test.ts"],
  },
  "finance/feature-payments": {
    pages: ["src/app/holdings/[holdingSlug]/finance/documents/sales/receipts/[id]/page.tsx", "src/app/holdings/[holdingSlug]/finance/cash/payments/[id]/page.tsx", "src/app/holdings/[holdingSlug]/finance/cash/refunds/[id]/page.tsx", "src/components/finance/refund-console.tsx", "src/components/finance/refund-bank-evidence.tsx"],
    apis: ["src/app/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts"],
    services: ["src/server/modules/finance/adapters/sales-payment-reversal.ts", "src/server/modules/finance/adapters/sales-customer-receipts.ts", "src/server/modules/commerce/adapters/finance-record-sources.ts", "src/server/modules/finance/adapters/refunds.ts", "src/server/modules/commerce/adapters/finance-refund-source.ts", "src/server/modules/finance/adapters/refund-bank-evidence.ts", "src/server/modules/finance/adapters/refund-purchases.ts", "src/server/modules/finance/adapters/purchase-boundary.ts"],
    schema: ["src/server/modules/finance/domain/refund-bank-evidence.ts", "migrations/postgres/holding-write/1057_refund_bank_evidence_consumption.sql"],
    tests: ["tests/integration/finance/finance-sales-workspace.test.ts", "tests/unit/finance/finance-navigation.test.ts", "tests/unit/identity_access/product-workspace.test.ts", "tests/unit/finance/finance-refund-services.test.ts", "tests/unit/finance/refund-bank-evidence-transport.test.ts", "tests/integration/sales/commerce-returns.test.ts", "tests/integration/sales/company-service-refund-migration.test.ts", "tests/integration/sales/refund-bank-evidence-migration.test.ts"],
  },
};
const workflowRefs = (id:string, category:"pages"|"apis"|"services"|"schema"|"tests", retained:string) =>
  [...new Set([...refs(retained), ...(currentProductWorkflowSources[id]?.[category] ?? [])])];
const owners = JSON.parse(readFileSync(resolve(root, "src/server/modules/ownership.json"), "utf8")) as { owners: { id: string; modules: string[] }[] };
const modules = moduleRegistry.map(module => {
  if (!(module.id in modulePlacement) || !packages[module.id as keyof typeof packages]) throw new Error(`UNMAPPED_MODULE:${module.id}`);
  const placement = modulePlacement[module.id as keyof typeof modulePlacement];
  const ownerIds = owners.owners.filter(owner => owner.modules.includes(module.id)).map(owner => owner.id);
  const jobs = ownerIds.flatMap(owner => {
    const directory = resolve(root, "src/server/modules", owner);
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
    jobs: [...new Set(jobs)].sort(), jobDispatcher: "src/workers/jobs.ts", workflows,
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
if (modules.length !== 33 || new Set(modules.map(row => row.moduleId)).size !== 33 || surfaces.length !== sharedHostKeys.length) errors.push("INCOMPLETE_PRODUCT_COVERAGE");
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
