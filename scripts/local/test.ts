import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

// Keep the database credential out of shell arguments and logs. Vitest sets
// NODE_ENV=test, for which Bun does not automatically load .env.local.
if (Bun.version !== "1.2.17") throw new Error("Tests require Bun 1.2.17.");
const root = resolve(import.meta.dir, "../..");
const candidates = [
  process.env.ERPX_NODE_BIN,
  resolve(root, ".runtime/node/node-v24.20.0-darwin-arm64/bin/node"),
  resolve(root, ".runtime/deployment/runtime/node/bin/node"),
].filter((path): path is string => !!path && existsSync(path));
const node = candidates[0];
if (!node) throw new Error("Pinned Node is missing. Set ERPX_NODE_BIN to Node 24.20.0.");
const version = Bun.spawnSync([node, "--version"]);
if (version.exitCode !== 0 || version.stdout.toString().trim() !== "v24.20.0")
  throw new Error("Tests require Node 24.20.0.");

// Disposable fixtures choose their own attachment generation. Legacy fixture
// cases retain the compatibility path; V2 intake/owner cases explicitly enable
// V2 and prove its native bind/scan guards. A development rollout must not
// silently change either fixture generation or freeze the test Companies.
const attachmentMode = process.env.ERPX_TEST_ATTACHMENTS_V2 ?? "false";
if (!["false", "true"].includes(attachmentMode)) throw new Error("INVALID_TEST_ATTACHMENT_MODE");
const env: Record<string, string | undefined> = { ...process.env, NODE_ENV: "test", ERPX_ATTACHMENTS_V2: attachmentMode, ERPX_ATTACHMENT_UPLOAD_FREEZE: "false" };
if (env.ERPX_TEST_STRIPE_SANDBOX === "1") {
  // A selected existing Sandbox key is scoped to this disposable test process;
  // it does not replace deployment credentials or permit a live-mode fallback.
  const binding = env.ERPX_TEST_STRIPE_KEY_BINDING ?? "STRIPE_SECRET_KEY";
  if (!["STRIPE_SECRET_KEY", "POS_STRIPE_TEST_SECRET_KEY"].includes(binding) || !env[binding]?.startsWith("sk_test_"))
    throw new Error("EXISTING_STRIPE_SANDBOX_TEST_KEY_REQUIRED");
  env.STRIPE_SECRET_KEY = env[binding];
}
if (!env.ERPX_TEST_ADMIN_URL) {
  for (const file of [".env.test.local", ".env.test", ".env.local"]) {
    const path = resolve(root, file);
    if (!existsSync(path)) continue;
    const value = readFileSync(path, "utf8").match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1];
    if (value) {
      env.ERPX_TEST_ADMIN_URL = value.replace(/^(["'])(.*)\1$/, "$2");
      break;
    }
  }
}
if (!env.ERPX_TEST_ADMIN_URL)
  throw new Error("Configure ERPX_TEST_ADMIN_URL for the isolated local test administrator.");
// tests/runtime-env.ts validates host, port, role, ownership and isolation.
const child = Bun.spawn([node, resolve(root, "node_modules/vitest/vitest.mjs"), "run", "--config", "vitest.config.mts", ...process.argv.slice(2)], {
  cwd: root, env, stdin: "inherit", stdout: "inherit", stderr: "inherit",
});
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
