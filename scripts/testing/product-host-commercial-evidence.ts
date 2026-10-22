import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import type { BrowserRunCaseReceipt, BrowserRunReceipt } from "./browser-runner";
import { browserCases, assertBrowserCasePassed, sameBrowserFileIdentity } from "./browser-results";
import { assignedBrowserShard } from "./browser-sharding";
import type { BrowserCiCohortReceipt } from "./browser-sharding";
import { companyAdminOwnerReadsProductHostCase, productHostBusinessWorkflowForCase, productHostReadOnlyWorkflowForCase, type ProductHostBusinessWorkflow } from "./product-host-cohort";

export const productHostSuiteId = "identity_access.product-hosts.e2e";
export const productHostBrowserFile = "tests/e2e/identity_access/product-hosts.spec.ts";
const commercialAttachmentName = "isolated-normal-commercial";
const readOnlyAttachmentName = "company-admin-owner-reads";
const readOnlyPages = [
  { owner: "brand", path: "/holdings/simulation/administration/brand/identity", screenshot: "company-admin-brand-identity" },
  { owner: "organization", path: "/holdings/simulation/administration/technology/organization", screenshot: "company-admin-organization" },
  { owner: "iam", path: "/holdings/simulation/administration/technology/settings/roles", screenshot: "company-admin-iam-roles" },
] as const;
const commercialSchemaKeys = ["changes", "noInstalledWrites", "noSqlGrants", "normalOwnerEndpoints", "release", "run", "sourceFingerprint"];
const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const digest = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const hash = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");

export type ProductHostEvidenceBinding = { path: string; sha256: string; bytes: number };
export type ProductHostRelease = { id: string; fileManifestSha256: string };

type RawAttachment = { name?: unknown; contentType?: unknown; path?: unknown };
type RawTest = {
  projectName: string;
  expectedStatus: string;
  status?: string;
  annotations?: { type: string; description?: string }[];
  results?: { status: string; attachments?: RawAttachment[] }[];
};
type RawSpec = { file: string; title: string; tests: RawTest[] };
type RawSuite = { title?: string; suites?: RawSuite[]; specs?: RawSpec[] };
type BrowserReport = { suites: RawSuite[] };

function reportTest(report: BrowserReport, wanted: { file?: string; title?: string; project: "desktop" | "mobile" }) {
  const matches: RawTest[] = [];
  function visit(suite: RawSuite, parents: string[]) {
    const isFile = typeof suite.title === "string" && /\.spec\.[cm]?[jt]sx?$/.test(suite.title);
    const names = suite.title && !isFile ? [...parents, suite.title] : parents;
    for (const spec of suite.specs ?? []) {
      const title = [...names, spec.title].join(" ");
      if (wanted.file && sameBrowserFileIdentity(spec.file, wanted.file) && title === wanted.title) {
        for (const test of spec.tests ?? []) if (test.projectName === wanted.project) matches.push(test);
      }
    }
    for (const child of suite.suites ?? []) visit(child, names);
  }
  for (const suite of report.suites) visit(suite, []);
  if (matches.length !== 1) throw new Error("PRODUCT_HOST_REPORT_CASE_IDENTITY_REQUIRED");
  return matches[0];
}

/** Bind the Admin UI proof to the exact existing Company authority, with no Commercial or authority writes. */
export async function validateProductHostReadOnlyCase(input: {
  root: string;
  receipt: BrowserRunReceipt;
  test: BrowserRunCaseReceipt;
  report: unknown;
  lease: unknown;
  leaseBinding: { path: string; sha256: string };
  reportBinding: { path: string; sha256: string };
  readOnlyBinding: ProductHostEvidenceBinding;
  readOnlyAttachmentPath: string;
  expected: { runtimeFingerprint: string; verificationFingerprint: string; release: ProductHostRelease };
}) {
  const { receipt, test, expected } = input;
  const workflow = test.file ? productHostReadOnlyWorkflowForCase(test.file, test.title ?? "") : null;
  if (workflow !== "admin.company-brand-organization-iam" || test.suite !== productHostSuiteId || !test.file ||
      !sameBrowserFileIdentity(test.file, productHostBrowserFile) || test.title !== companyAdminOwnerReadsProductHostCase ||
      !test.run || !/^[a-f0-9]{12}$/.test(test.run) || test.passed !== true || test.required === false ||
      !["desktop", "mobile"].includes(test.project) || !digest(test.reportSha256) ||
      !digest(input.readOnlyBinding.sha256) || !Number.isSafeInteger(input.readOnlyBinding.bytes) || input.readOnlyBinding.bytes < 1 ||
      !digest(input.leaseBinding.sha256) || !digest(input.reportBinding.sha256)) {
    throw new Error("PRODUCT_HOST_READ_ONLY_CASE_RECEIPT_REQUIRED");
  }
  if (!record(input.report) || !Array.isArray(input.report.suites)) throw new Error("PRODUCT_HOST_REPORT_REQUIRED");
  if (input.reportBinding.sha256 !== test.reportSha256 || !test.productHostLease ||
      resolve(test.productHostLease.path) !== resolve(input.leaseBinding.path) || test.productHostLease.sha256 !== input.leaseBinding.sha256 ||
      !sameBinding(test.productHostReadOnly, input.readOnlyBinding) || test.productHostCommercial !== undefined) {
    throw new Error("PRODUCT_HOST_READ_ONLY_EVIDENCE_BINDING_REQUIRED");
  }
  const report = input.report as BrowserReport, cases = browserCases(report);
  if (cases.length !== 1 || !sameBrowserFileIdentity(cases[0].file, test.file) || cases[0].title !== test.title || cases[0].project !== test.project)
    throw new Error("PRODUCT_HOST_REPORT_CASE_IDENTITY_REQUIRED");
  assertBrowserCasePassed(report as Parameters<typeof assertBrowserCasePassed>[0], cases[0]);
  const rawTest = reportTest(report, test), attachments = rawTest.results?.[0]?.attachments ?? [];
  if (rawTest.results?.length !== 1 || rawTest.results[0].status !== "passed") throw new Error("PRODUCT_HOST_REPORT_PASS_REQUIRED");
  if (attachments.some(item => item.name === commercialAttachmentName)) throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_UNEXPECTED");
  const proofAttachments = attachments.filter(item => item.name === readOnlyAttachmentName);
  if (proofAttachments.length !== 1 || proofAttachments[0].contentType !== "application/json" || typeof proofAttachments[0].path !== "string")
    throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_REQUIRED");
  const proofPath = resolve(String(proofAttachments[0].path));
  if (!isAbsolute(String(proofAttachments[0].path)) || proofPath !== resolve(input.readOnlyAttachmentPath) ||
      !isAbsolute(input.reportBinding.path) || resolve(test.report ?? "") !== resolve(input.reportBinding.path) || !isAbsolute(input.leaseBinding.path))
    throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_BINDING_REQUIRED");
  const proofFile = await readBoundLeaf(resolve(input.root), proofPath, resolve(dirname(input.reportBinding.path), "artifacts"));
  if (!sameBinding(input.readOnlyBinding, proofFile.binding)) throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_BINDING_REQUIRED");
  let proof: unknown;
  try { proof = JSON.parse(proofFile.bytes.toString("utf8")); }
  catch (error) { throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_INVALID", { cause: error }); }
  const pageAttachments = new Map<string, string>();
  for (const page of readOnlyPages) {
    const matches = attachments.filter(item => item.name === page.screenshot);
    if (matches.length !== 1 || matches[0].contentType !== "image/png" || typeof matches[0].path !== "string")
      throw new Error("PRODUCT_HOST_READ_ONLY_SCREENSHOT_REQUIRED");
    const screenshotPath = resolve(String(matches[0].path));
    if (!isAbsolute(String(matches[0].path))) throw new Error("PRODUCT_HOST_READ_ONLY_SCREENSHOT_BINDING_REQUIRED");
    const screenshot = await readBoundLeaf(resolve(input.root), screenshotPath, resolve(dirname(input.reportBinding.path), "artifacts"));
    pageAttachments.set(page.screenshot, screenshot.binding.path);
  }
  const value = proof as Record<string, unknown>, authority = value.authority;
  if (!record(value) || Object.keys(value).sort().join(",") !== ["authority", "blockedWrites", "pages", "project", "release", "result", "run", "schema", "sourceFingerprint", "verificationFingerprint", "workflow"].sort().join(",") ||
      value.schema !== "erpx-product-host-company-admin-owner-reads/v1" || value.result !== "passed" || value.workflow !== workflow ||
      value.run !== test.run || value.sourceFingerprint !== expected.runtimeFingerprint || value.verificationFingerprint !== expected.verificationFingerprint ||
      value.project !== test.project || !record(value.release) || value.release.id !== expected.release.id ||
      value.release.fileManifestSha256 !== expected.release.fileManifestSha256 || !record(authority) ||
      Object.keys(authority).sort().join(",") !== ["assignmentId", "companyId", "ownerAssignmentUnchanged", "role", "sameOwnerAssignmentAcrossPages", "userId"].sort().join(",") ||
      typeof authority.companyId !== "string" || !authority.companyId || typeof authority.userId !== "string" || !authority.userId ||
      typeof authority.assignmentId !== "string" || !authority.assignmentId || authority.role !== "holding_owner" ||
      authority.ownerAssignmentUnchanged !== true || authority.sameOwnerAssignmentAcrossPages !== true ||
      !Array.isArray(value.blockedWrites) || value.blockedWrites.length !== 0 || !Array.isArray(value.pages) || value.pages.length !== readOnlyPages.length) {
    throw new Error("PRODUCT_HOST_READ_ONLY_PROOF_MISMATCH");
  }
  for (let index = 0; index < readOnlyPages.length; index++) {
    const expectedPage = readOnlyPages[index], page = value.pages[index];
    if (!record(page) || Object.keys(page).sort().join(",") !== ["observation", "owner", "path", "screenshot"].sort().join(",") ||
        page.owner !== expectedPage.owner || page.path !== expectedPage.path || page.screenshot !== expectedPage.screenshot ||
        typeof page.observation !== "string" || !page.observation.trim() || !pageAttachments.has(expectedPage.screenshot))
      throw new Error("PRODUCT_HOST_READ_ONLY_PROOF_MISMATCH");
  }
  const lease = input.lease;
  if (!record(lease) || lease.schema !== "erpx-owned-product-host-browser/v1" || lease.run !== test.run ||
      lease.sourceFingerprint !== expected.runtimeFingerprint || lease.verificationFingerprint !== expected.verificationFingerprint ||
      !record(lease.release) || lease.release.id !== expected.release.id || lease.release.fileManifestSha256 !== expected.release.fileManifestSha256 ||
      lease.readOnlyBusiness !== true || lease.scopedBusinessWorkflow !== null || lease.readOnlyWorkflow !== workflow ||
      lease.isolatedNormalCommercial !== false || lease.wholeProductAcceptance !== false)
    throw new Error("PRODUCT_HOST_READ_ONLY_LEASE_SCOPE_REQUIRED");
  if (receipt.runtimeFingerprint !== expected.runtimeFingerprint || receipt.verificationFingerprint !== expected.verificationFingerprint)
    throw new Error("PRODUCT_HOST_RECEIPT_FINGERPRINT_MISMATCH");
  if (test.uiEvidence && (resolve(input.root, test.uiEvidence.report.path) !== resolve(input.reportBinding.path) ||
      test.uiEvidence.report.sha256 !== input.reportBinding.sha256 || test.uiEvidence.productHostLease?.sha256 !== input.leaseBinding.sha256))
    throw new Error("PRODUCT_HOST_UI_WITNESS_BINDING_MISMATCH");
  return { suite: test.suite, file: test.file, title: test.title, project: test.project, run: test.run, workflow };
}

/** Pure contract check for one exact passing Product Host case and its retained commercial claim. */
export function validateProductHostCommercialCase(input: {
  receipt: BrowserRunReceipt;
  test: BrowserRunCaseReceipt;
  report: unknown;
  lease: unknown;
  commercial: unknown;
  leaseBinding: { path: string; sha256: string };
  reportBinding: { path: string; sha256: string };
  commercialBinding: ProductHostEvidenceBinding;
  commercialAttachmentPath: string;
  expected: { runtimeFingerprint: string; verificationFingerprint: string; release: ProductHostRelease };
}) {
  const { receipt, test, expected } = input;
  if (test.suite !== productHostSuiteId || !test.file || !sameBrowserFileIdentity(test.file, productHostBrowserFile) || !test.run ||
      !/^[a-f0-9]{12}$/.test(test.run) || test.passed !== true || test.required === false ||
      !test.title || !["desktop", "mobile"].includes(test.project) ||
      !digest(input.reportBinding.sha256) || !digest(input.leaseBinding.sha256) || !digest(input.commercialBinding.sha256) ||
      !Number.isSafeInteger(input.commercialBinding.bytes) || input.commercialBinding.bytes < 1) {
    throw new Error("PRODUCT_HOST_CASE_RECEIPT_REQUIRED");
  }
  if (!record(input.report) || !Array.isArray(input.report.suites)) throw new Error("PRODUCT_HOST_REPORT_REQUIRED");
  if (input.reportBinding.sha256 !== test.reportSha256 || !test.productHostLease ||
      resolve(test.productHostLease.path) !== resolve(input.leaseBinding.path) || test.productHostLease.sha256 !== input.leaseBinding.sha256 ||
      !sameBinding(test.productHostCommercial, input.commercialBinding))
    throw new Error("PRODUCT_HOST_CASE_EVIDENCE_BINDING_REQUIRED");
  const report = input.report as BrowserReport;
  const cases = browserCases(report);
  if (cases.length !== 1 || !sameBrowserFileIdentity(cases[0].file, test.file) || cases[0].title !== test.title || cases[0].project !== test.project)
    throw new Error("PRODUCT_HOST_REPORT_CASE_IDENTITY_REQUIRED");
  assertBrowserCasePassed(report as Parameters<typeof assertBrowserCasePassed>[0], cases[0]);
  const rawTest = reportTest(report, test);
  if (rawTest.results?.length !== 1 || rawTest.results[0].status !== "passed" || !Array.isArray(rawTest.results[0].attachments))
    throw new Error("PRODUCT_HOST_REPORT_PASS_REQUIRED");
  const attachments = rawTest.results[0].attachments.filter(item => item.name === commercialAttachmentName);
  if (attachments.length !== 1 || attachments[0].contentType !== "application/json" || typeof attachments[0].path !== "string")
    throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_REQUIRED");
  const attachmentPath = resolve(String(attachments[0].path));
  if (!isAbsolute(String(attachments[0].path)) || attachmentPath !== resolve(input.commercialAttachmentPath) ||
      !isAbsolute(input.reportBinding.path) || resolve(test.report ?? "") !== resolve(input.reportBinding.path) ||
      !isAbsolute(input.leaseBinding.path)) throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_BINDING_REQUIRED");

  const lease = input.lease;
  const workflow: ProductHostBusinessWorkflow | null = productHostBusinessWorkflowForCase(test.file, test.title);
  if (!record(lease) || lease.schema !== "erpx-owned-product-host-browser/v1" || lease.run !== test.run ||
      lease.sourceFingerprint !== expected.runtimeFingerprint || lease.verificationFingerprint !== expected.verificationFingerprint ||
      !record(lease.release) || lease.release.id !== expected.release.id || lease.release.fileManifestSha256 !== expected.release.fileManifestSha256 ||
      lease.isolatedNormalCommercial !== true || lease.wholeProductAcceptance !== false ||
      lease.scopedBusinessWorkflow !== workflow || lease.readOnlyBusiness !== (workflow === null) || lease.readOnlyWorkflow !== null) {
    throw new Error("PRODUCT_HOST_LEASE_SCOPE_OR_CANDIDATE_MISMATCH");
  }
  if (receipt.runtimeFingerprint !== expected.runtimeFingerprint || receipt.verificationFingerprint !== expected.verificationFingerprint)
    throw new Error("PRODUCT_HOST_RECEIPT_FINGERPRINT_MISMATCH");

  const commercial = input.commercial;
  if (!record(commercial) || Object.keys(commercial).sort().join(",") !== [...commercialSchemaKeys].sort().join(",") ||
      commercial.run !== test.run || commercial.sourceFingerprint !== expected.runtimeFingerprint ||
      !record(commercial.release) || commercial.release.id !== expected.release.id ||
      commercial.release.fileManifestSha256 !== expected.release.fileManifestSha256 ||
      !Array.isArray(commercial.changes) || commercial.noInstalledWrites !== true ||
      commercial.noSqlGrants !== true || commercial.normalOwnerEndpoints !== true) {
    throw new Error("PRODUCT_HOST_COMMERCIAL_GATE_MISMATCH");
  }
  for (const change of commercial.changes) {
    if (!record(change) || typeof change.holdingId !== "string" || !change.holdingId ||
        typeof change.companyId !== "string" || !change.companyId ||
        !digest(change.requestDigest) || !digest(change.receiptDigest)) {
      throw new Error("PRODUCT_HOST_COMMERCIAL_CHANGE_EVIDENCE_INVALID");
    }
  }
  return { suite: test.suite, file: test.file, title: test.title, project: test.project, run: test.run };
}

async function readBoundLeaf(root: string, pathInput: string, within: string) {
  if (!pathInput || pathInput.includes("\\")) throw new Error("PRODUCT_HOST_EVIDENCE_PATH_REQUIRED");
  const rootReal = resolve(root), path = resolve(rootReal, pathInput), withinReal = resolve(within);
  if (!path.startsWith(rootReal + sep) || !path.startsWith(withinReal + sep)) throw new Error("PRODUCT_HOST_EVIDENCE_PATH_REQUIRED");
  try {
    const before = await lstat(path, { bigint: true });
    if (!before.isFile() || before.isSymbolicLink() || before.size > BigInt(64 * 1024 * 1024) || await realpath(path) !== path)
      throw new Error("PRODUCT_HOST_EVIDENCE_FILE_REQUIRED");
    const bytes = await readFile(path), after = await lstat(path, { bigint: true });
    if (!after.isFile() || after.isSymbolicLink() || before.dev !== after.dev || before.ino !== after.ino ||
        before.size !== after.size || before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs ||
        BigInt(bytes.length) !== after.size) throw new Error("PRODUCT_HOST_EVIDENCE_FILE_CHANGED");
    return { path, bytes, binding: { path: relative(rootReal, path).replaceAll("\\", "/"), sha256: hash(bytes), bytes: bytes.length } };
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("PRODUCT_HOST_EVIDENCE_")) throw error;
    throw new Error("PRODUCT_HOST_EVIDENCE_FILE_REQUIRED", { cause: error });
  }
}

function sameBinding(left: ProductHostEvidenceBinding | undefined, right: ProductHostEvidenceBinding) {
  return Boolean(left && left.path === right.path && left.sha256 === right.sha256 && left.bytes === right.bytes);
}

/** Read and bind every required Product Host case in one forward or reverse acceptance receipt. */
export async function validateProductHostCommercialCohort(
  rootInput: string,
  receipt: BrowserRunReceipt,
  expected: { runtimeFingerprint: string; verificationFingerprint: string; release: ProductHostRelease },
) {
  const root = resolve(rootInput);
  if (await realpath(root) !== root || !receipt.evidencePath ||
      !/^\.scratch\/dev\/test-suites\/browser\/\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/.test(receipt.evidencePath))
    throw new Error("PRODUCT_HOST_EVIDENCE_ROOT_REQUIRED");
  const productInventory = receipt.inventory.filter(test => test.suite === productHostSuiteId);
  const assignedProductInventory = productInventory.filter(test =>
    !receipt.shard || assignedBrowserShard(test.suite, test, receipt.shard));
  const inventory = assignedProductInventory.filter(test => test.required);
  const productReceipts = receipt.receipts.filter(test => test.suite === productHostSuiteId);
  const tests = productReceipts.filter(test => test.required !== false);
  if (!productInventory.length) throw new Error("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");
  // A sharded cohort can legitimately own no Product Host cases. Its complete
  // inventory still names the suite, but no assigned Product Host receipt may
  // appear in either order. Unsharded/full receipts stay strictly non-empty.
  if (receipt.shard && assignedProductInventory.length === 0) {
    if (productReceipts.length !== 0) throw new Error("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");
    return [];
  }
  if (!inventory.length || tests.length !== inventory.length ||
      new Set(tests.map(test => JSON.stringify([test.file, test.title, test.project]))).size !== tests.length) {
    throw new Error("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");
  }
  const evidenceDirectory = resolve(root, receipt.evidencePath);
  if (!evidenceDirectory.startsWith(resolve(root, ".scratch/dev/test-suites/browser") + sep))
    throw new Error("PRODUCT_HOST_EVIDENCE_ROOT_REQUIRED");
  const accepted: (ReturnType<typeof validateProductHostCommercialCase> | Awaited<ReturnType<typeof validateProductHostReadOnlyCase>>)[] = [];
  for (const test of tests) {
    if (!test.file || !sameBrowserFileIdentity(test.file, productHostBrowserFile) || typeof test.report !== "string" ||
        typeof test.run !== "string" || !/^[a-f0-9]{12}$/.test(test.run) ||
        !test.productHostLease || !digest(test.productHostLease.sha256) || !digest(test.reportSha256))
      throw new Error("PRODUCT_HOST_CASE_RECEIPT_REQUIRED");
    const caseDirectory = resolve(evidenceDirectory, test.run), expectedReportPath = resolve(caseDirectory, "result.json"),
      expectedLeasePath = resolve(caseDirectory, "product-host-lease.json"), artifactsDirectory = resolve(caseDirectory, "artifacts");
    if (resolve(test.report) !== expectedReportPath || resolve(test.productHostLease.path) !== expectedLeasePath)
      throw new Error("PRODUCT_HOST_CASE_PATH_BINDING_REQUIRED");
    const reportFile = await readBoundLeaf(root, expectedReportPath, caseDirectory);
    if (reportFile.binding.sha256 !== test.reportSha256) throw new Error("PRODUCT_HOST_REPORT_DIGEST_MISMATCH");
    const leaseFile = await readBoundLeaf(root, expectedLeasePath, caseDirectory);
    const leaseStat = await lstat(expectedLeasePath);
    if (leaseFile.binding.sha256 !== test.productHostLease.sha256 || (leaseStat.mode & 0o077) !== 0)
      throw new Error("PRODUCT_HOST_LEASE_DIGEST_MISMATCH");
    let report: unknown, lease: unknown;
    try { report = JSON.parse(reportFile.bytes.toString("utf8")); lease = JSON.parse(leaseFile.bytes.toString("utf8")); }
    catch (error) { throw new Error("PRODUCT_HOST_REPORT_OR_LEASE_INVALID", { cause: error }); }
    if (!record(report) || !Array.isArray(report.suites)) throw new Error("PRODUCT_HOST_REPORT_REQUIRED");
    const reportCases = browserCases(report as BrowserReport);
    if (reportCases.length !== 1) throw new Error("PRODUCT_HOST_REPORT_CASE_IDENTITY_REQUIRED");
    const rawTest = reportTest(report as BrowserReport, { file: test.file, title: test.title, project: test.project });
    const attachments = rawTest.results?.[0]?.attachments ?? [];
    const businessWorkflow = productHostBusinessWorkflowForCase(test.file,test.title??"");
    const readOnlyWorkflow = productHostReadOnlyWorkflowForCase(test.file, test.title ?? "");
    if (readOnlyWorkflow) {
      if (!test.productHostReadOnly || test.productHostCommercial !== undefined) throw new Error("PRODUCT_HOST_READ_ONLY_CASE_RECEIPT_REQUIRED");
      const matching = attachments.filter(item => item.name === readOnlyAttachmentName);
      if (matching.length !== 1 || matching[0].contentType !== "application/json" || typeof matching[0].path !== "string")
        throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_REQUIRED");
      const readOnlyPath = resolve(String(matching[0].path));
      if (!isAbsolute(String(matching[0].path))) throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_BINDING_REQUIRED");
      const readOnlyFile = await readBoundLeaf(root, readOnlyPath, artifactsDirectory);
      const readOnlyBinding = { ...readOnlyFile.binding };
      if (!sameBinding(test.productHostReadOnly, readOnlyBinding)) throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_BINDING_REQUIRED");
      if (test.uiEvidence) {
        const witness = test.uiEvidence;
        if (resolve(root, witness.report.path) !== expectedReportPath || witness.report.sha256 !== reportFile.binding.sha256 ||
            witness.productHostLease?.sha256 !== leaseFile.binding.sha256 ||
            (witness.productHostLease && resolve(root, witness.productHostLease.path) !== expectedLeasePath))
          throw new Error("PRODUCT_HOST_UI_WITNESS_BINDING_MISMATCH");
      }
      accepted.push(await validateProductHostReadOnlyCase({
        root, receipt, test, report, lease,
        leaseBinding: { path: expectedLeasePath, sha256: leaseFile.binding.sha256 },
        reportBinding: { path: expectedReportPath, sha256: reportFile.binding.sha256 },
        readOnlyBinding, readOnlyAttachmentPath: readOnlyFile.path, expected,
      }));
      continue;
    }
    if (test.productHostReadOnly !== undefined) throw new Error("PRODUCT_HOST_READ_ONLY_CASE_UNEXPECTED");
    const matching = attachments.filter(item => item.name === commercialAttachmentName);
    if (matching.length !== 1 || matching[0].contentType !== "application/json" || typeof matching[0].path !== "string")
      throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_REQUIRED");
    const commercialPath = resolve(String(matching[0].path));
    const commercialFile = await readBoundLeaf(root, commercialPath, artifactsDirectory);
    const commercialBinding = { ...commercialFile.binding };
    if (!test.productHostCommercial || !sameBinding(test.productHostCommercial, commercialBinding))
      throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_BINDING_REQUIRED");
    let commercial: unknown;
    try { commercial = JSON.parse(commercialFile.bytes.toString("utf8")); }
    catch (error) { throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_INVALID", { cause: error }); }
    if (test.uiEvidence) {
      const witness = test.uiEvidence;
      if (resolve(root, witness.report.path) !== expectedReportPath || witness.report.sha256 !== reportFile.binding.sha256 ||
          witness.productHostLease?.sha256 !== leaseFile.binding.sha256 ||
          (witness.productHostLease && resolve(root, witness.productHostLease.path) !== expectedLeasePath))
        throw new Error("PRODUCT_HOST_UI_WITNESS_BINDING_MISMATCH");
    }
    accepted.push(validateProductHostCommercialCase({
      receipt, test, report, lease, commercial,
      leaseBinding: { path: expectedLeasePath, sha256: leaseFile.binding.sha256 },
      reportBinding: { path: expectedReportPath, sha256: reportFile.binding.sha256 }, commercialBinding,
      commercialAttachmentPath: commercialFile.path,
      expected,
    }));
  }
  const expectedCases = inventory.map(test => JSON.stringify([test.file, test.title, test.project])).sort();
  const actualCases = tests.map(test => JSON.stringify([test.file, test.title, test.project])).sort();
  if (JSON.stringify(expectedCases) !== JSON.stringify(actualCases)) throw new Error("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");
  return accepted;
}

/** CI aggregate entry point: require the product-host group when its suite is required and validate both orders. */
export async function validateProductHostCommercialCohorts(
  root: string,
  cohorts: readonly BrowserCiCohortReceipt[],
  expected: { runtimeFingerprint: string; verificationFingerprint: string; release: ProductHostRelease },
  required: boolean,
) {
  const groups = cohorts.filter(cohort => cohort.group === "product-host");
  if ((required && groups.length !== 1) || (!required && groups.length !== 0))
    throw new Error("PRODUCT_HOST_ACCEPTANCE_GROUP_REQUIRED");
  if (!required) return [];
  const cohort = groups[0];
  if (cohort.passed !== true || !cohort.forward || !cohort.reverse)
    throw new Error("PRODUCT_HOST_ACCEPTANCE_PROOF_REQUIRED");
  if (JSON.stringify(cohort.forward.shard) !== JSON.stringify(cohort.reverse.shard))
    throw new Error("PRODUCT_HOST_ACCEPTANCE_SHARD_MISMATCH");
  const forward = await validateProductHostCommercialCohort(root, cohort.forward, expected);
  const reverse = await validateProductHostCommercialCohort(root, cohort.reverse, expected);
  return [...forward, ...reverse];
}

/** Create the receipt bindings at the producer while the report and attachments are still in the owned case folder. */
export async function captureProductHostReceiptBindings(root: string, evidenceDirectory: string, run: string, reportPath: string, report: unknown) {
  if (!/^[a-f0-9]{12}$/.test(run)) throw new Error("PRODUCT_HOST_CASE_RUN_ID_REQUIRED");
  const reportFile = await readBoundLeaf(resolve(root), reportPath, resolve(evidenceDirectory, run));
  if (!record(report) || !Array.isArray(report.suites)) throw new Error("PRODUCT_HOST_REPORT_REQUIRED");
  const cases = browserCases(report as BrowserReport);
  if (cases.length !== 1 || !sameBrowserFileIdentity(cases[0].file, productHostBrowserFile)) throw new Error("PRODUCT_HOST_REPORT_CASE_IDENTITY_REQUIRED");
  const rawTest = reportTest(report as BrowserReport, cases[0]);
  const attachments = rawTest.results?.[0]?.attachments ?? [], readOnlyWorkflow = productHostReadOnlyWorkflowForCase(cases[0].file, cases[0].title), businessWorkflow = productHostBusinessWorkflowForCase(cases[0].file,cases[0].title);
  const leasePath = resolve(evidenceDirectory, run, "product-host-lease.json"), lease = await readBoundLeaf(resolve(root), leasePath, resolve(evidenceDirectory, run));
  if (readOnlyWorkflow) {
    const matchingReadOnly = attachments.filter(item => item.name === readOnlyAttachmentName);
    if (attachments.some(item => item.name === commercialAttachmentName)) throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_UNEXPECTED");
    if (matchingReadOnly.length !== 1 || matchingReadOnly[0].contentType !== "application/json" || typeof matchingReadOnly[0].path !== "string")
      throw new Error("PRODUCT_HOST_READ_ONLY_ATTACHMENT_REQUIRED");
    const readOnly = await readBoundLeaf(resolve(root), String(matchingReadOnly[0].path), resolve(evidenceDirectory, run, "artifacts"));
    return { reportSha256: reportFile.binding.sha256, productHostReadOnly: readOnly.binding, leaseSha256: lease.binding.sha256 };
  }
  const matching = attachments.filter(item => item.name === commercialAttachmentName);
  if (matching.length !== 1 || matching[0].contentType !== "application/json" || typeof matching[0].path !== "string")
    throw new Error("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_REQUIRED");
  const commercial = await readBoundLeaf(resolve(root), String(matching[0].path), resolve(evidenceDirectory, run, "artifacts"));
  if(businessWorkflow==='finance.sales-invoice'){
    const proof=attachments.filter(item=>item.name==='finance-product-host-sales-invoice'),fixtureState=attachments.filter(item=>item.name==='finance-product-host-simulation-state'),screenshot=attachments.filter(item=>item.name==='finance-product-host-sales-invoice-screen');
    if(proof.length!==1||proof[0].contentType!=='application/json'||typeof proof[0].path!=='string'||fixtureState.length!==1||fixtureState[0].contentType!=='application/json'||typeof fixtureState[0].path!=='string'||screenshot.length!==1||screenshot[0].contentType!=='image/png'||typeof screenshot[0].path!=='string')throw new Error('PRODUCT_HOST_FINANCE_INVOICE_ATTACHMENTS_REQUIRED');
    const artifactDirectory=resolve(evidenceDirectory,run,'artifacts'),[proofFile,stateFile,screenshotFile]=await Promise.all([
      readBoundLeaf(resolve(root),String(proof[0].path),artifactDirectory),readBoundLeaf(resolve(root),String(fixtureState[0].path),artifactDirectory),readBoundLeaf(resolve(root),String(screenshot[0].path),artifactDirectory),
    ]);
    return {reportSha256:reportFile.binding.sha256,productHostCommercial:commercial.binding,productHostBusiness:{proof:proofFile.binding,fixtureState:stateFile.binding,screenshot:screenshotFile.binding},leaseSha256:lease.binding.sha256};
  }
  return { reportSha256: reportFile.binding.sha256, productHostCommercial: commercial.binding, leaseSha256: lease.binding.sha256 };
}
