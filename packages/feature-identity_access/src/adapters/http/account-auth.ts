import type { AccountAuthPort, AccountAuthResult } from "../../application/account-auth-port";
import type { AccountAuthError, AccountOperationResponse, AccountSessionResponse, AccountUserDto, DeviceAccountsDto, EmailSignInResponse } from "@erpx/contracts/account-auth";

type FetchPort = (input: string, init: RequestInit) => Promise<Response>;
type Validator<T> = (input: unknown) => input is T;
const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const keys = (value: Record<string, unknown>, expected: readonly string[]) => Object.keys(value).every(key => expected.includes(key)) && expected.every(key => key in value);
const text = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 4096;
const nullableText = (value: unknown): value is string | null => value === null || text(value);
const safeRevision = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const handle = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
const user: Validator<AccountUserDto> = (value): value is AccountUserDto => record(value) && keys(value, ["id", "name", "email", "emailVerified", "image"]) && text(value.id) && text(value.name) && text(value.email) && typeof value.emailVerified === "boolean" && nullableText(value.image);
const signedIn: Validator<EmailSignInResponse> = (value): value is EmailSignInResponse => record(value) && keys(value, ["user"]) && user(value.user);
const operation: Validator<AccountOperationResponse> = (value): value is AccountOperationResponse => record(value) && keys(value, ["status"]) && value.status === true;
const date = (value: unknown): value is string => typeof value === "string" && /^\d{4}-\d{2}-\d{2}T/.test(value) && Number.isFinite(Date.parse(value));
const session: Validator<AccountSessionResponse> = (value): value is AccountSessionResponse => record(value) && keys(value, ["user", "session"]) && user(value.user) && record(value.session) && keys(value.session, ["id", "userId", "expiresAt", "createdAt", "updatedAt"]) && text(value.session.id) && value.session.userId === value.user.id && date(value.session.expiresAt) && date(value.session.createdAt) && date(value.session.updatedAt);
const devices: Validator<DeviceAccountsDto> = (value): value is DeviceAccountsDto => record(value) && keys(value, ["initialized", "revision", "maximum", "accounts"]) && typeof value.initialized === "boolean" && safeRevision(value.revision) && value.maximum === 5 && Array.isArray(value.accounts) && value.accounts.length <= 5 && value.accounts.every(account => record(account) && keys(account, ["handle", "name", "email", "image", "current"]) && handle(account.handle) && text(account.name) && text(account.email) && nullableText(account.image) && typeof account.current === "boolean") && new Set(value.accounts.map(account => account.handle)).size === value.accounts.length && value.accounts.filter(account => account.current).length <= 1;
const authError = (value: unknown): AccountAuthError | null => record(value) && keys(value, ["error"]) && record(value.error) && keys(value.error, ["code", "message"]) && text(value.error.code) && text(value.error.message) ? { code: value.error.code, message: value.error.message } : null;
const failed = <T>(code: string, message: string): AccountAuthResult<T> => ({ data: null, error: { code, message } });

/** No backend imports, token persistence or compatibility fallback. Responses must match Rust contracts. */
export function createAccountAuthHttpPort(fetcher: FetchPort = (input, init) => globalThis.fetch(input, init)): AccountAuthPort {
  async function request<T>(path: string, method: "GET" | "POST", validate: Validator<T>, body?: object, nullable = false): Promise<AccountAuthResult<T>> {
    try {
      const response = await fetcher(path, { method, credentials: "include", cache: "no-store", redirect: "error", headers: { accept: "application/json", ...(body ? { "content-type": "application/json" } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
      const value: unknown = await response.json();
      if (!response.ok) return { data: null, error: authError(value) ?? { code: "ACCOUNT_REQUEST_FAILED", message: "The Account request could not be completed." } };
      if (value === null && nullable) return { data: null, error: null };
      return validate(value) ? { data: value, error: null } : failed("ACCOUNT_RESPONSE_INVALID", "The Account response did not match its contract.");
    } catch { return failed("ACCOUNT_NETWORK_UNAVAILABLE", "Could not connect to Account. Please try again."); }
  }
  return {
    signIn: { email: input => input.revision !== undefined && !safeRevision(input.revision) ? Promise.resolve(failed("INVALID_DEVICE_REVISION", "Review your current saved accounts.")) : request("/api/auth/sign-in/email", "POST", signedIn, input) },
    signUp: { email: input => request("/api/auth/sign-up/email", "POST", operation, input) },
    signOut: () => request("/api/auth/sign-out", "POST", operation),
    getSession: () => request("/api/auth/get-session", "GET", session, undefined, true),
    requestPasswordReset: input => request("/api/auth/request-password-reset", "POST", operation, input),
    resetPassword: input => request("/api/auth/reset-password", "POST", operation, input),
    sendVerificationEmail: input => request("/api/auth/send-verification-email", "POST", operation, input),
    verifyEmail: input => input.revision !== undefined && !safeRevision(input.revision) ? Promise.resolve(failed("INVALID_DEVICE_REVISION", "Review your current saved accounts.")) : request("/api/auth/verify-email", "POST", signedIn, input),
    changePassword: input => request("/api/auth/change-password", "POST", operation, input),
    multiSession: {
      listDeviceSessions: () => request("/api/v1/account/device-accounts", "GET", devices),
      setActive: input => !safeRevision(input.revision) || !handle(input.handle) ? Promise.resolve(failed("INVALID_DEVICE_SELECTION", "Review your current saved accounts.")) : request("/api/v1/account/device-accounts/active", "POST", devices, input),
    },
  };
}
