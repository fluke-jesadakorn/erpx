import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import type { Page, TestInfo } from "@playwright/test";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../../../scripts/local/source-fingerprint";

type BrowserState = {
  run: string;
  prefix: string;
  origin: string;
  port: number;
  ready: boolean;
  diagnostic?: boolean;
  sourceFingerprint: string;
  softwareVersion: string;
  release: { id: string; fileManifestSha256: string };
};

const hash = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");

/** Captures a screenshot and a minimal, accurately scoped browser observation after the caller's workflow assertions pass. */
export async function attachProductMigrationUiObservation(
  page: Page,
  info: TestInfo,
  input: { label: string; subject: string; audience: string; authority: string; checks: readonly string[] },
) {
  const root = resolve(process.cwd());
  const run = process.env.ERPX_BROWSER_SIMULATION_RUN ?? process.env.ERPX_DEMO_RUN;
  if (!run || !/^[a-f0-9]{12}$/.test(run) || !input.checks.length || input.checks.some(check => !check.trim()))
    throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_INPUT_REQUIRED");
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(input.label)) throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_LABEL_REQUIRED");

  const statePath = resolve(root, ".runtime/browser", run, "state.json"), stateStat = lstatSync(statePath);
  if (!stateStat.isFile() || stateStat.isSymbolicLink() || realpathSync.native(statePath) !== statePath || (stateStat.mode & 0o077) !== 0)
    throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_STATE_REQUIRED");
  const stateBytes = readFileSync(statePath), state = JSON.parse(stateBytes.toString("utf8")) as BrowserState;
  if (state.run !== run || state.prefix !== `erpx_browser_${run}` || state.origin !== "http://127.0.0.1:3104" || state.port !== 3104 ||
      state.ready !== true || state.diagnostic === true || !state.release || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,100}$/.test(state.release.id) ||
      !/^[a-f0-9]{64}$/.test(state.release.fileManifestSha256)) throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_COHORT_REQUIRED");

  const sourceFingerprint = await runtimeSourceFingerprint(root);
  const verificationFingerprint = await verificationSourceFingerprint(root, sourceFingerprint);
  const candidate = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  if (state.sourceFingerprint !== sourceFingerprint || state.softwareVersion !== `release-${sourceFingerprint}` || !/^[a-f0-9]{40}$/.test(candidate))
    throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_FINGERPRINT_REQUIRED");
  const url = new URL(page.url());
  if (url.origin !== state.origin || url.username || url.password) throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_ORIGIN_REQUIRED");

  const screenshotPath = info.outputPath(`${input.label}.png`);
  const observationPath = info.outputPath(`${input.label}.json`);
  if (existsSync(screenshotPath) || existsSync(observationPath)) throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_PATH_EXISTS");
  await page.screenshot({ path: screenshotPath, type: "png", fullPage: false, animations: "disabled" });
  await info.attach("product-migration-ui-screenshot", { path: screenshotPath, contentType: "image/png" });
  const screenshotAttachment = info.attachments.at(-1);
  if (screenshotAttachment?.name !== "product-migration-ui-screenshot" || screenshotAttachment.contentType !== "image/png" || !screenshotAttachment.path)
    throw new Error("PRODUCT_MIGRATION_UI_SCREENSHOT_ATTACHMENT_REQUIRED");
  const screenshotBytes = readFileSync(screenshotAttachment.path);
  if (!screenshotBytes.length || !screenshotAttachment.path.startsWith(dirname(screenshotPath) + "/") || !realpathSync.native(screenshotAttachment.path).startsWith(root + "/"))
    throw new Error("PRODUCT_MIGRATION_UI_SCREENSHOT_BINDING_REQUIRED");

  const screenshot = { path: relative(root, screenshotAttachment.path).replaceAll("\\", "/"), sha256: hash(screenshotBytes), bytes: screenshotBytes.length };
  const observation = {
    schema: "erpx-ui-browser-observation/v1",
    result: "passed",
    run,
    candidate,
    sourceFingerprint,
    verificationFingerprint,
    releaseId: state.release.id,
    releaseManifestSha256: state.release.fileManifestSha256,
    url: url.href,
    subject: input.subject,
    audience: input.audience,
    authority: input.authority,
    checks: [...input.checks],
    screenshot,
  };
  writeFileSync(observationPath, JSON.stringify(observation, null, 2) + "\n", { flag: "wx", mode: 0o600 });
  await info.attach("erpx-ui-observation", { path: observationPath, contentType: "application/json" });
  const attachedObservation = info.attachments.at(-1);
  if (attachedObservation?.name !== "erpx-ui-observation" || attachedObservation.contentType !== "application/json" || !attachedObservation.path ||
      !readFileSync(attachedObservation.path).equals(Buffer.from(JSON.stringify(observation, null, 2) + "\n")))
    throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_ATTACHMENT_REQUIRED");
  if (!readFileSync(statePath).equals(stateBytes) || await runtimeSourceFingerprint(root) !== sourceFingerprint ||
      await verificationSourceFingerprint(root, sourceFingerprint) !== verificationFingerprint)
    throw new Error("PRODUCT_MIGRATION_UI_OBSERVATION_SOURCE_CHANGED");
}
