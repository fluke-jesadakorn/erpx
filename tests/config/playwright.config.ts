import { chromium, defineConfig, devices } from "@playwright/test";
import { join } from "node:path";
import { readFileSync, realpathSync, lstatSync } from "node:fs";
import { createHash } from "node:crypto";

// All package and catalogue runners execute from the repository root. Keep
// report/artifact locations stable when Playwright loads this config as CJS.
const root = process.cwd();

// Use the persistent development server. Starting/stopping it belongs to the
// workspace lifecycle, not Playwright (the public Tunnel also uses this server).
const baseURL = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
const uatRouteSweep = Boolean(process.env.ERPX_UAT_AUTH_DIR);
// Only the runner-produced exact case lease can select the private product-host proxy.
const leasePath=process.env.ERPX_PRODUCT_HOST_LEASE;
const lease=leasePath?JSON.parse(readFileSync(leasePath,"utf8")):null;
if(lease){
  if(!leasePath!.startsWith(join(root,".scratch/dev/test-suites/browser/"))||realpathSync(leasePath!)!==leasePath||!lstatSync(leasePath!).isFile()||(lstatSync(leasePath!).mode&0o077)!==0||lease.schema!=="erpx-owned-product-host-browser/v1"||lease.run!==process.env.ERPX_DEMO_RUN||lease.port!==3102||lease.upstream!=="http://127.0.0.1:3104"||!/^[a-f0-9]{64}$/.test(lease.spki))throw Error("EXACT_PRODUCT_HOST_PLAYWRIGHT_LEASE_REQUIRED");
  if(lease.chromium.path!==chromium.executablePath())throw Error("PRODUCT_HOST_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
  process.kill(lease.pid,0);
  for(const binding of [lease.plan,lease.ca,lease.chromium])if(realpathSync(binding.path)!==binding.path||createHash("sha256").update(readFileSync(binding.path)).digest("hex")!==binding.sha256)throw Error("PRODUCT_HOST_PLAYWRIGHT_LEASE_CHANGED");
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
    launchOptions: {
      ...(lease?{proxy:{server:"http://127.0.0.1:3102"},executablePath:lease.chromium.path}:{}),
      args: ["--host-resolver-rules=MAP dev-pms.localhost 127.0.0.1,MAP dev-pos.localhost 127.0.0.1,MAP dev-erpx.localhost 127.0.0.1",...(lease?["--proxy-bypass-list=<-loopback>",`--ignore-certificate-errors-spki-list=${Buffer.from(lease.spki,"hex").toString("base64")}`]:[])],
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
