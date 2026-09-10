import { z } from "zod";
export type BetaEnvironment = {
  ERPX_PUBLIC_BETA?: string;
  ERPX_RESOURCE_SLOTS?: string;
  BETA_MEMBER_LIMIT?: string;
  BETA_BRANCH_LIMIT?: string;
  BETA_AI_LIMIT?: string;
  BETA_STORAGE_BYTES?: string;
  RESEND_API_KEY?: string;
  RESEND_FROM?: string;
  AI_BASE_URL?: string;
  AI_MODEL?: string;
  AI_API_KEY?: string;
  AI_CREDENTIAL_KEY_ID?: string;
  AI_CREDENTIAL_KEYS?: string;
  STRIPE_SECRET_KEY?: string;
  STRIPE_WEBHOOK_SECRET?: string;
  STRIPE_PRICE_ID?: string;
};
const positive = (v: string | undefined, fallback: number) =>
  z.coerce
    .number()
    .int()
    .positive()
    .max(Number.MAX_SAFE_INTEGER)
    .parse(v ?? fallback);
export function betaLimits(env: BetaEnvironment) {
  return {
    members: positive(env.BETA_MEMBER_LIMIT, 20),
    branches: positive(env.BETA_BRANCH_LIMIT, 5),
    ai: positive(env.BETA_AI_LIMIT, 100),
    storage: positive(env.BETA_STORAGE_BYTES, 1024 ** 3),
  };
}
