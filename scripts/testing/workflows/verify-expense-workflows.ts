// The v2 browser journey is the canonical local expense acceptance suite.
// Keep the existing business-verification entry point, using an isolated company.
const origin = process.env.ERPX_TEST_BASE_URL ?? "http://localhost:3100";
const target = new URL(origin);
if (
  !["localhost", "127.0.0.1"].includes(target.hostname) ||
  ["3000", "3001", "8080"].includes(target.port)
)
  throw new Error("Use the local ERPX service on port 3100 or 8787.");
const node = Bun.which("node");
if (!node) throw new Error("Node.js is required for Playwright.");
for (const command of [
  [process.execPath, "scripts/seed/seed-expense-e2e.ts"],
  [
    node,
    "node_modules/@playwright/test/cli.js",
    "test",
    "--config",
    "tests/config/playwright.config.ts",
    "tests/e2e/people_payroll/expense-v2.spec.ts",
    "--project=desktop",
  ],
]) {
  const child = Bun.spawn(command, {
    stdout: "inherit",
    stderr: "inherit",
    env: { ...process.env, ERPX_E2E_BASE_URL: origin },
  });
  if (await child.exited) process.exit(1);
}
export {};
