import { chromium, defineConfig, devices } from "@playwright/test";
import { join } from "node:path";
import { readFileSync, realpathSync, lstatSync } from "node:fs";
import { createHash } from "node:crypto";
import { formsHostOrigin, formsHostWorkflow } from "../../scripts/testing/forms-host-cohort";
import { readDevelopmentHostLease } from "../../scripts/testing/development-host-profile";

// All package and catalogue runners execute from the repository root. Keep
// report/artifact locations stable when Playwright loads this config as CJS.
const root = process.cwd();
const developmentLease = readDevelopmentHostLease(root, process.env);

// Use the persistent development server. Starting/stopping it belongs to the
// workspace lifecycle, not Playwright (the public Tunnel also uses this server).
const productLeasePath=process.env.ERPX_PRODUCT_HOST_LEASE;
const executiveWorkLeasePath=process.env.ERPX_EXECUTIVE_WORK_HOST_LEASE;
const formsHostLeasePath=process.env.ERPX_FORMS_HOST_LEASE;
if(developmentLease&&(productLeasePath||executiveWorkLeasePath||formsHostLeasePath))throw Error('DEVELOPMENT_HOST_LEASE_MODE_IS_EXCLUSIVE');
if(productLeasePath&&executiveWorkLeasePath)throw Error("PRODUCT_AND_EXECUTIVE_WORK_HOST_MODES_ARE_EXCLUSIVE");
if(formsHostLeasePath&&(productLeasePath||executiveWorkLeasePath))throw Error("FORMS_HOST_LEASE_MODE_IS_EXCLUSIVE");
if(formsHostLeasePath&&process.env.ERPX_E2E_BASE_URL!==formsHostOrigin)throw Error("FORMS_HOST_EXACT_HTTPS_ORIGIN_REQUIRED");
const baseURL = formsHostLeasePath ? formsHostOrigin : executiveWorkLeasePath ? "https://work-dev-erpx.jesadakorn.com" : process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
const uatRouteSweep = Boolean(process.env.ERPX_UAT_AUTH_DIR);
// Only the runner-produced exact case lease can select the private product-host proxy.
const leasePath=productLeasePath;
const lease=leasePath?JSON.parse(readFileSync(leasePath,"utf8")):null;
if(lease){
  if(!leasePath!.startsWith(join(root,".scratch/dev/test-suites/browser/"))||realpathSync(leasePath!)!==leasePath||!lstatSync(leasePath!).isFile()||(lstatSync(leasePath!).mode&0o077)!==0||lease.schema!=="erpx-owned-product-host-browser/v1"||lease.run!==process.env.ERPX_DEMO_RUN||lease.port!==3102||lease.upstream!=="http://127.0.0.1:3104"||!/^[a-f0-9]{64}$/.test(lease.spki))throw Error("EXACT_PRODUCT_HOST_PLAYWRIGHT_LEASE_REQUIRED");
  if(lease.chromium.path!==chromium.executablePath())throw Error("PRODUCT_HOST_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
  process.kill(lease.pid,0);
  for(const binding of [lease.plan,lease.ca,lease.chromium])if(realpathSync(binding.path)!==binding.path||createHash("sha256").update(readFileSync(binding.path)).digest("hex")!==binding.sha256)throw Error("PRODUCT_HOST_PLAYWRIGHT_LEASE_CHANGED");
}
const executiveWorkLease=executiveWorkLeasePath?JSON.parse(readFileSync(executiveWorkLeasePath,"utf8")):null;
if(executiveWorkLease){
  const expectedOrigins=["https://work-dev-erpx.jesadakorn.com","https://account-dev-erpx.jesadakorn.com"];
  const expectedMethods=["initialize","notifications/initialized","tools/list","tools/call"];
  const expectedTools=["list_metrics","query_metric"];
  if(!executiveWorkLeasePath!.startsWith(join(root,".scratch/dev/test-suites/browser/"))||realpathSync(executiveWorkLeasePath!)!==executiveWorkLeasePath||!lstatSync(executiveWorkLeasePath!).isFile()||(lstatSync(executiveWorkLeasePath!).mode&0o077)!==0||executiveWorkLease.schema!=="erpx-owned-executive-work-host-browser/v1"||executiveWorkLease.run!==process.env.ERPX_DEMO_RUN||executiveWorkLease.port!==3102||executiveWorkLease.upstream!=="http://127.0.0.1:3104"||JSON.stringify(executiveWorkLease.origins)!==JSON.stringify(expectedOrigins)||executiveWorkLease.workOnly!==true||executiveWorkLease.accountOnlyForSso!==true||executiveWorkLease.commercialMfa!==false||executiveWorkLease.commercialApply!==false||executiveWorkLease.packageEnforcement!==false||executiveWorkLease.installedWrites!==false||executiveWorkLease.businessWrites!==false||JSON.stringify(executiveWorkLease.mcp?.methods)!==JSON.stringify(expectedMethods)||JSON.stringify(executiveWorkLease.mcp?.tools)!==JSON.stringify(expectedTools)||executiveWorkLease.mcp?.companyScope!==true||executiveWorkLease.mcp?.credentialBound!==true||executiveWorkLease.mcp?.businessReadOnly!==true||executiveWorkLease.denialProbe?.method!=="POST"||executiveWorkLease.denialProbe?.status!==403||executiveWorkLease.denialProbe?.count!==1||!/^[a-f0-9]{64}$/.test(executiveWorkLease.spki))throw Error("EXACT_EXECUTIVE_WORK_HOST_PLAYWRIGHT_LEASE_REQUIRED");
  if(executiveWorkLease.chromium.path!==chromium.executablePath())throw Error("EXECUTIVE_WORK_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
  process.kill(executiveWorkLease.pid,0);
  for(const binding of [executiveWorkLease.ca,executiveWorkLease.certificate,executiveWorkLease.chromium,executiveWorkLease.node])if(realpathSync(binding.path)!==binding.path||createHash("sha256").update(readFileSync(binding.path)).digest("hex")!==binding.sha256)throw Error("EXECUTIVE_WORK_PLAYWRIGHT_LEASE_CHANGED");
}
const formsHostLease=formsHostLeasePath?JSON.parse(readFileSync(formsHostLeasePath,"utf8")):null;
if(formsHostLease){
  const formsRun=process.env.ERPX_DEMO_RUN;
  if(!formsRun||!/^[a-f0-9]{12}$/.test(formsRun))throw Error("FORMS_HOST_RUN_BINDING_REQUIRED");
  const statePath=join(root,".runtime/browser",formsRun,"state.json"),formsStateBytes=readFileSync(statePath),formsState=JSON.parse(formsStateBytes.toString("utf8"));
  if(!formsHostLeasePath!.startsWith(join(root,".scratch/dev/test-suites/browser/"))||!formsHostLeasePath!.endsWith(`/${formsRun}/forms-host-lease.json`)||realpathSync(formsHostLeasePath!)!==formsHostLeasePath||!lstatSync(formsHostLeasePath!).isFile()||(lstatSync(formsHostLeasePath!).mode&0o077)!==0||formsHostLease.schema!=="erpx-owned-forms-host-browser/v1"||formsHostLease.run!==formsRun||formsHostLease.port!==3102||formsHostLease.upstream!=="http://127.0.0.1:3104"||formsHostLease.workflow!==formsHostWorkflow||formsHostLease.origin!==formsHostOrigin||formsHostLease.formsOnly!==true||formsHostLease.productHosts!==false||formsHostLease.executiveWorkHosts!==false||formsHostLease.commercial!==false||formsHostLease.mfa!==false||formsHostLease.apply!==false||formsHostLease.packageEnforcement!==false||formsHostLease.wholeProductAcceptance!==false||formsHostLease.requestEvidence?.acceptedPosts!==0||formsHostLease.requestEvidence?.replayCount!==0||formsHostLease.requestEvidence?.refusalCount!==0||formsState.run!==formsRun||formsState.port!==3104||formsState.profile!=="engineering"||formsState.formsHosts!==true||formsState.productHosts!==false||formsState.executiveWorkHosts!==false||formsState.sourceFingerprint!==formsHostLease.sourceFingerprint||formsHostLease.release?.id!==formsState.release?.id||formsHostLease.release?.fileManifestSha256!==formsState.release?.fileManifestSha256||formsHostLease.ui?.listener?.address!=="127.0.0.1"||formsHostLease.ui?.listener?.port!==3102||formsHostLease.ui?.listener?.pid!==formsHostLease.pid||formsHostLease.ui?.backend?.pid!==formsState.pid||formsHostLease.ui?.backend?.jobsPid!==formsState.jobsPid||formsHostLease.ui?.backend?.state?.path!==statePath||formsHostLease.ui?.backend?.state?.sha256!==createHash("sha256").update(formsStateBytes).digest("hex")||formsHostLease.ui?.backend?.state?.bytes!==formsStateBytes.length||!/^[a-f0-9]{64}$/.test(formsHostLease.spki))throw Error("EXACT_FORMS_HOST_PLAYWRIGHT_LEASE_REQUIRED");
  if(formsHostLease.chromium.path!==chromium.executablePath())throw Error("FORMS_HOST_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
  process.kill(formsHostLease.pid,0);
  for(const binding of [formsHostLease.plan,formsHostLease.certificate,formsHostLease.key,formsHostLease.ca,formsHostLease.chromium,formsHostLease.databases,formsHostLease.fixtureGuide,formsHostLease.ui?.source,formsHostLease.ui?.node])if(!binding||realpathSync(binding.path)!==binding.path||!lstatSync(binding.path).isFile()||createHash("sha256").update(readFileSync(binding.path)).digest("hex")!==binding.sha256)throw Error("FORMS_HOST_PLAYWRIGHT_LEASE_CHANGED");
  for(const binding of [formsHostLease.key,formsHostLease.databases,formsHostLease.fixtureGuide])if((lstatSync(binding.path).mode&0o077)!==0)throw Error("FORMS_HOST_PRIVATE_EVIDENCE_REQUIRED");
}
const selectedFiles: string[] | undefined = process.env.ERPX_TEST_BROWSER_FILES
  ? JSON.parse(process.env.ERPX_TEST_BROWSER_FILES)
  : undefined;

export default defineConfig({
  testDir: "..",
  outputDir: join(root, "test-results"),
  ...(selectedFiles ? { testMatch: selectedFiles.map(file => `**/${file.replace(/^tests\//, "")}`) }
    : { testMatch: "**/{e2e,journeys,uat}/**/*.spec.ts", testIgnore: ["**/uat-route-matrix.spec.ts", "**/legal-uat.spec.ts"] }),
  fullyParallel: uatRouteSweep,
  workers: uatRouteSweep ? 4 : 1,
  forbidOnly: !!process.env.CI,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  retries: 0,
  reporter: [["list"], ["html", { open: "never", outputFolder: join(root, "playwright-report") }]],
  use: {
    baseURL,
    ...(developmentLease ? { serviceWorkers: "block", ignoreHTTPSErrors: false } : {}),
    launchOptions: {
      ...(developmentLease ? {proxy:{server:"http://127.0.0.1:3102"},executablePath:chromium.executablePath()} : {}),
      ...(lease||executiveWorkLease||formsHostLease?{proxy:{server:"http://127.0.0.1:3102"},executablePath:(lease??executiveWorkLease??formsHostLease).chromium.path}:{}),
      args: ["--host-resolver-rules=MAP dev-pms.localhost 127.0.0.1,MAP dev-pos.localhost 127.0.0.1,MAP dev-erpx.localhost 127.0.0.1",
        ...(developmentLease?["--proxy-bypass-list=<-loopback>",`--ignore-certificate-errors-spki-list=${Buffer.from(developmentLease.spki,"hex").toString("base64")}`,"--disable-background-networking","--disable-component-update","--disable-sync"]:[]),
        ...(lease||executiveWorkLease||formsHostLease?["--proxy-bypass-list=<-loopback>",`--ignore-certificate-errors-spki-list=${Buffer.from((lease??executiveWorkLease??formsHostLease).spki,"hex").toString("base64")}`]:[]),
        ...(executiveWorkLease||formsHostLease?["--disable-background-networking","--disable-component-update","--disable-sync","--no-first-run","--disable-domain-reliability","--disable-features=AutofillServerCommunication,OptimizationHints"]:[])],
    },
    trace: "retain-on-failure",
    screenshot: process.env.ERPX_ENGINEERING_BROWSER_EVIDENCE === "1" ? "on" : "only-on-failure",
    video: process.env.ERPX_ENGINEERING_BROWSER_EVIDENCE === "1" ? "on" : "off",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
});
