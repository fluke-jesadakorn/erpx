import { createHash } from "node:crypto";

/** Seeded record identities stay immutable; only this document's cohort metadata changes. */
export function clonedFixtureMetadata(value: unknown, templateRun: string, run: string, templateDirectory: string, directory: string): unknown {
  if (!/^[a-f0-9]{12}$/.test(templateRun) || !/^[a-f0-9]{12}$/.test(run) || templateRun === run) throw new Error("EXACT_DISTINCT_BROWSER_RUNS_REQUIRED");
  const copy = JSON.parse(JSON.stringify(value).replaceAll(templateDirectory, directory)) as Record<string, unknown>;
  if (!copy || typeof copy !== "object" || Array.isArray(copy)) return copy;
  if (copy.run === templateRun) { copy.run = run; copy.seedRun = templateRun; }
  const environment = copy.environment as Record<string, unknown> | undefined;
  if (environment && environment.run === templateRun) { environment.run = run; environment.seedRun = templateRun; }
  const reset = copy.reset as { commands?: unknown } | undefined;
  if (reset && Array.isArray(reset.commands)) reset.commands = reset.commands.map(command => typeof command === "string" ? command.replace(`demo:destroy -- ${templateRun}`, `demo:destroy -- ${run}`) : command);
  const verification = copy.acceptance as { browserCommand?: unknown } | undefined;
  if (verification && typeof verification.browserCommand === "string") verification.browserCommand = verification.browserCommand.replace(`ERPX_DEMO_RUN=${templateRun}`, `ERPX_DEMO_RUN=${run}`);
  return copy;
}

export function isClonedFixtureDocument(name: string) {
  return ["credentials.json", "fixture.json", "demo-guide.json", "hospitality-guide.json", "legal-guide.json", "legal-credentials.json", "engineering-money-people.json", "engineering-thai-payroll.json", "engineering-finance-close.json", "workplace-platform-guide.json", "workplace-platform-credentials.json"].includes(name);
}

/** Execution evidence must be produced by the new case, never inherited and relabelled. */
export function isCaseExecutionEvidence(name: string) {
  return !name.includes("/") && !name.includes("\\") && (/(?:-evidence|-control-fixture)\.json$/.test(name) || /^(?:engineering-attestation|attachment-final|jobs-(?:pause|resume))\.json$/.test(name) || name === "route-results.jsonl" || /^legal-(?:destruction|preservation)-/.test(name));
}

/** The encryption ring belongs to the cohort; it contains no external provider credential. */
export function isolatedAiCredentialEnvironment(secret: string) {
  if (!/^[a-f0-9]{64}$/.test(secret)) throw new Error("PRIVATE_BROWSER_SECRET_REQUIRED");
  const id = "browser-v1";
  return { AI_CREDENTIAL_KEY_ID: id, AI_CREDENTIAL_KEYS: JSON.stringify({ [id]: createHash("sha256").update(`simulation-ai:${secret}`).digest("base64") }) };
}

/** Normal staffing profiles retain the existing Acme workspace identity. */
export function moduleBrowserHoldingDisplayName(profile: string): string {
  return ["local-seed", "staff-work"].includes(profile) ? "Acme Group" : "Isolated Acme fixture";
}

/** Only the declared normal staffing profiles replace legacy local login defaults. */
export function localBrowserActorEnvironment(profile: string, fixture: { actors?: { hr?: { email: string; password: string }; reader?: { email: string; password: string } } }): Record<string, string> {
  if (!["local-seed", "staff-work"].includes(profile)) return {};
  const { hr, reader } = fixture.actors ?? {};
  if (hr?.email !== "demo.bangkok.people.manager@erpx.dev" || reader?.email !== "demo.bangkok.operations.staff@erpx.dev" || !hr.password || !reader.password) throw new Error("LOCAL_BROWSER_ACME_ACTORS_REQUIRED");
  return { ERPX_E2E_HR_EMAIL: hr.email, ERPX_E2E_HR_PASSWORD: hr.password, ERPX_E2E_READER_EMAIL: reader.email, ERPX_E2E_READER_PASSWORD: reader.password };
}

/** Fresh browser cohorts must keep Front navigation on their own loopback port. */
export function browserFrontOrigin(base: string, app: "pos" | "pms", explicitOrigin?: string): string {
  const url = new URL(base);
  if (["127.0.0.1", "localhost"].includes(url.hostname)) {
    if (!["3100", "3102", "3104"].includes(url.port) || url.protocol !== "http:") throw new Error("LOCAL_BROWSER_FRONT_ORIGIN_REQUIRED");
    const origin = `http://dev-${app}.localhost:${url.port}`;
    if (explicitOrigin && new URL(explicitOrigin).origin !== origin) throw new Error("LOCAL_BROWSER_FRONT_ORIGIN_REQUIRED");
    return origin;
  }
  if (!explicitOrigin) throw new Error("EXPLICIT_BROWSER_FRONT_ORIGIN_REQUIRED");
  const front = new URL(explicitOrigin);
  if (front.protocol !== "https:" || front.pathname !== "/" || front.search || front.hash || front.username || front.password) throw new Error("EXPLICIT_BROWSER_FRONT_ORIGIN_REQUIRED");
  return front.origin;
}
