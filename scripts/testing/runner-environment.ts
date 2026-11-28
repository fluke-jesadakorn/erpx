import { delimiter } from "node:path";

/** Pure units cannot inherit a database administrator or ambient PostgreSQL connection. */
export function vitestChildEnvironment(environment: Record<string, string | undefined>, postgres: boolean): Record<string, string | undefined> {
  const selected = { ...environment };
  // Native owner fixtures choose their mode explicitly. Never inherit an
  // installed development rollout flag into otherwise legacy regression cases.
  delete selected.ERPX_SUPPORT_PACKAGE_ENFORCEMENT;
  delete selected.ERPX_SERVICE_PACKAGE_ENFORCEMENT;
  delete selected.ERPX_HOLDING_ROUTING;
  const postgresTools = selected.ERPX_TEST_PG_BIN?.trim();
  if (postgresTools) {
    const existingPath = selected.PATH?.split(delimiter).filter(Boolean) ?? [];
    selected.PATH = [postgresTools, ...existingPath.filter(entry => entry !== postgresTools)].join(delimiter);
  }
  if (!postgres) for (const key of ["ERPX_TEST_ADMIN_URL", "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD", "PGSERVICE", "PGSERVICEFILE", "PGPASSFILE"]) delete selected[key];
  return selected;
}

type Environment = Record<string, string | undefined>;
const providerConfiguration = /^(AI_|RESEND_|STRIPE_|POS_STRIPE_|SITEMINDER_|ERPX_MINIO_|ERPX_IAM_|ERPX_POS_STRIPE_|OPENAI_|ANTHROPIC_|GEMINI_|AWS_|MINIO_)/;
const ambientRuntimeConfiguration = /^(ERPX_ENGINEERING_|ERPX_DEVELOPMENT_HOST_|ERPX_BROWSER_|ERPX_E2E_|ERPX_DEMO_|ERPX_UAT_|ERPX_TEST_RELEASE_|ERPX_SERVER_SCRIPT_)/;
const ambientRuntimeNames = new Set([
  "ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "ERPX_SERVICE_PACKAGE_ENFORCEMENT", "ERPX_HOLDING_ROUTING", "ERPX_DATABASE_MANIFEST", "ERPX_MIGRATION_DATABASE_MANIFEST", "ERPX_DATA_DIR", "ERPX_PROCESS_ROLE", "ERPX_ENVIRONMENT",
  "ERPX_PUBLIC_ORIGIN", "ERPX_MARKETING_ORIGIN", "ERPX_FRONT_PMS_ORIGIN", "ERPX_FRONT_POS_ORIGIN", "ERPX_PUBLIC_BETA",
  "ERPX_EMAIL_ADAPTER", "ERPX_AI_ADAPTER", "ERPX_PAYROLL_ADAPTER", "ERPX_POS_ROOM_CHARGE_PILOT_ENABLED",
  "ERPX_PMS_PROVIDER_FIXTURE", "PMS_STRIPE_BOOKING_ENABLED",
  "ERPX_ATTACHMENTS_V2", "ERPX_ATTACHMENT_UPLOAD_FREEZE", "BETTER_AUTH_URL", "BETTER_AUTH_SECRET", "NEXT_DEPLOYMENT_ID",
  "PORT", "HOSTNAME", "DATABASE_URL", "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD", "PGSERVICE", "PGSERVICEFILE", "PGPASSFILE",
  "GOOGLE_APPLICATION_CREDENTIALS", "NODE_EXTRA_CA_CERTS",
]);

/**
 * Ambient workspace settings never choose a browser cohort or a live provider.
 * The second argument contains only the explicitly selected case/producer
 * settings. Keep the restricted test administrator and installed tool paths.
 */
export function browserChildEnvironment(ambient: Environment, overrides: Environment = {}): NodeJS.ProcessEnv {
  const selected = Object.fromEntries(Object.entries(ambient).filter(([name]) =>
    !providerConfiguration.test(name) && !ambientRuntimeConfiguration.test(name) && !ambientRuntimeNames.has(name)));
  for (const [name, value] of Object.entries(overrides)) {
    if (!providerConfiguration.test(name)) selected[name] = value;
  }
  // This ring encrypts declared fixture credentials; it is never an AI API key.
  if (overrides.AI_CREDENTIAL_KEY_ID !== undefined || overrides.AI_CREDENTIAL_KEYS !== undefined) {
    let keys: Record<string, unknown>;
    try { keys = JSON.parse(overrides.AI_CREDENTIAL_KEYS ?? ""); }
    catch { throw new Error("ISOLATED_BROWSER_AI_CREDENTIAL_RING_REQUIRED"); }
    const encoded = keys?.["browser-v1"];
    if (overrides.AI_CREDENTIAL_KEY_ID !== "browser-v1" || !keys || Array.isArray(keys) || Object.keys(keys).length !== 1 || typeof encoded !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(encoded) || Buffer.from(encoded, "base64").length !== 32 || Buffer.from(encoded, "base64").toString("base64") !== encoded)
      throw new Error("ISOLATED_BROWSER_AI_CREDENTIAL_RING_REQUIRED");
    selected.AI_CREDENTIAL_KEY_ID = "browser-v1";
    selected.AI_CREDENTIAL_KEYS = overrides.AI_CREDENTIAL_KEYS;
  }
  // The PMS email adapter refuses network calls under this exact fixture key.
  if (overrides.ERPX_EMAIL_ADAPTER === "sandbox" && overrides.ERPX_ENVIRONMENT === "local" && ["http://127.0.0.1:3102", "http://127.0.0.1:3104"].includes(overrides.ERPX_PUBLIC_ORIGIN ?? "") && /\/\.runtime\/browser\/[a-f0-9]{12}$/.test(overrides.ERPX_DATA_DIR ?? "") && overrides.RESEND_API_KEY === "deterministic-browser-booking") {
    selected.RESEND_API_KEY = overrides.RESEND_API_KEY;
    selected.RESEND_FROM = overrides.RESEND_FROM;
  }
  if (overrides.ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS === "1" && overrides.ERPX_EMAIL_ADAPTER === "sandbox" && overrides.ERPX_ENVIRONMENT === "local" && ["http://127.0.0.1:3102", "http://127.0.0.1:3104"].includes(overrides.ERPX_PUBLIC_ORIGIN ?? "") && /\/\.runtime\/browser\/[a-f0-9]{12}$/.test(overrides.ERPX_DATA_DIR ?? "") && overrides.RESEND_API_KEY === "isolated-sandbox-no-provider-key" && overrides.RESEND_FROM === "Fictional <no-reply@example.test>") {
    selected.RESEND_API_KEY = overrides.RESEND_API_KEY;
    selected.RESEND_FROM = overrides.RESEND_FROM;
  }
  if (overrides.ERPX_PMS_PROVIDER_FIXTURE === '1') {
    if (overrides.ERPX_ENVIRONMENT !== 'local' || overrides.ERPX_PUBLIC_ORIGIN !== 'http://127.0.0.1:3104' ||
        !/\/\.runtime\/browser\/[a-f0-9]{12}$/.test(overrides.ERPX_DATA_DIR ?? '') ||
        overrides.POS_STRIPE_TEST_SECRET_KEY !== 'sk_test_erpx_fictional_provider' ||
        overrides.POS_STRIPE_TEST_WEBHOOK_SECRET !== 'whsec_erpx_fictional_provider' || overrides.POS_STRIPE_LIVE_SECRET_KEY)
      throw new Error('EXACT_FICTIONAL_BROWSER_PROVIDER_REQUIRED');
    selected.POS_STRIPE_TEST_SECRET_KEY = overrides.POS_STRIPE_TEST_SECRET_KEY;
    selected.POS_STRIPE_TEST_WEBHOOK_SECRET = overrides.POS_STRIPE_TEST_WEBHOOK_SECRET;
    selected.PMS_STRIPE_BOOKING_ENABLED = 'true';
  }
  const nodeEnvironment = overrides.NODE_ENV ?? ambient.NODE_ENV ?? "development";
  if (nodeEnvironment !== "development" && nodeEnvironment !== "production" && nodeEnvironment !== "test") throw new Error("INVALID_BROWSER_NODE_ENV");
  return { ...selected, NODE_ENV: nodeEnvironment };
}
