import { BusinessRuleError } from "../../shared";
import { accountPreferencesInput, accountPreferencePatchInput, type AccountPreferencesInput, type AccountProfile } from "../domain/account-preferences";

export type AccountPreferenceUnit = {
  authorize(): Promise<void>;
  current(): Promise<AccountProfile | null>;
  save(input: AccountPreferencesInput): Promise<AccountProfile>;
  flush(): Promise<void>;
};
export function createAccountPreferencesApplication(ports: {
  accountKey: string;
  transaction<Result>(run: (unit: AccountPreferenceUnit) => Promise<Result>): Promise<Result>;
}) {
  return async (raw: unknown) => {
    const parsed = accountPreferencesInput.safeParse(raw);
    if (!parsed.success) throw new BusinessRuleError(422, "INVALID_ACCOUNT_PREFERENCES", "Review your name, language and theme.");
    const input = parsed.data;
    if (input.accountKey !== ports.accountKey) throw new BusinessRuleError(409, "ACCOUNT_CONTEXT_CHANGED", "Your account changed. Reload before saving these settings.");
    return ports.transaction(async unit => {
      await unit.authorize();
      const current = await unit.current();
      if (!current) throw new BusinessRuleError(401, "SIGN_IN_REQUIRED", "Sign in again to edit your account.");
      if (current.name !== input.expectedName || current.preferences.version !== input.expectedVersion)
        throw new BusinessRuleError(409, "ACCOUNT_PREFERENCES_CHANGED", "Your account settings changed. Reload and review the current settings.");
      const result = await unit.save(input);
      await unit.flush();
      await unit.authorize();
      return result;
    });
  };
}

export function createAccountPreferencePatch(ports: Parameters<typeof createAccountPreferencesApplication>[0]) {
  return async (raw: unknown) => {
    const parsed = accountPreferencePatchInput.safeParse(raw);
    if (!parsed.success) throw new BusinessRuleError(422, "INVALID_ACCOUNT_PREFERENCES", "Review your language and theme.");
    const input = parsed.data;
    if (input.accountKey !== ports.accountKey) throw new BusinessRuleError(409, "ACCOUNT_CONTEXT_CHANGED", "Your account changed. Reload before saving these settings.");
    return ports.transaction(async unit => {
      await unit.authorize();
      const current = await unit.current();
      if (!current) throw new BusinessRuleError(401, "SIGN_IN_REQUIRED", "Sign in again to edit your account.");
      if (current.preferences.version !== input.expectedVersion)
        throw new BusinessRuleError(409, "ACCOUNT_PREFERENCES_CHANGED", "Your account settings changed. Review the current settings.");
      const result = await unit.save({ ...input, name: current.name, expectedName: current.name,
        language: input.language ?? current.preferences.language, appearance: input.appearance ?? current.preferences.appearance });
      await unit.flush(); await unit.authorize();
      return result.preferences;
    });
  };
}
