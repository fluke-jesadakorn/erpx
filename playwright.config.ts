import { defineConfig, devices } from "@playwright/test";

// Use the persistent development server. Starting/stopping it belongs to the
// workspace lifecycle, not Playwright (the public Tunnel also uses this server).
const baseURL = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
const uatRouteSweep = Boolean(process.env.ERPX_UAT_AUTH_DIR);

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: uatRouteSweep,
  workers: uatRouteSweep ? 4 : 1,
  forbidOnly: !!process.env.CI,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 2 : 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL,
    launchOptions: {
      args: ["--host-resolver-rules=MAP dev-pms.localhost 127.0.0.1,MAP dev-pos.localhost 127.0.0.1,MAP dev-erpx.localhost 127.0.0.1"],
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
