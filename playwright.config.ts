import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.ERPX_E2E_BASE_URL ?? "http://localhost:8787";

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 2 : 0,
  use: {
    baseURL,
    trace: "retain-on-failure",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
  webServer: process.env.ERPX_E2E_BASE_URL ? undefined : {
    command: "bun run preview -- --port 8787",
    url: "http://localhost:8787/login",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
