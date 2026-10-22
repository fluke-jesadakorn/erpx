import {BusinessRuleError, type Clock} from '@erpx/server-shared/index';
import {accountTimePreferenceInput, defaultTimePreference, type AccountTimePreferenceInput, type TimePreference} from '@erpx/server-identity_access/domain/account-time-preference';

export interface AccountTimePreferencePorts {
  clock: Clock;
  ready(): Promise<void>;
  read(userId: string): Promise<TimePreference | null>;
  create(userId: string, input: AccountTimePreferenceInput, now: number): Promise<TimePreference | null>;
  change(userId: string, input: AccountTimePreferenceInput, now: number): Promise<TimePreference | null>;
}
export function createAccountTimePreferenceApplication(ports: AccountTimePreferencePorts) {
  return {
    async read(userId: string) { await ports.ready(); return await ports.read(userId) ?? defaultTimePreference; },
    async change(userId: string, raw: unknown) {
      const parsed = accountTimePreferenceInput.safeParse(raw);
      if (!parsed.success) throw new BusinessRuleError(422, 'INVALID_TIME_PREFERENCE', parsed.error.issues[0]?.message ?? 'Invalid time preference.');
      const input = parsed.data;
      await ports.ready();
      const now = ports.clock.now();
      const row = input.version === 0 ? await ports.create(userId, input, now) : await ports.change(userId, input, now);
      if (!row) throw new BusinessRuleError(409, 'TIME_PREFERENCE_CHANGED', 'Reload your time settings and try again.');
      return row;
    },
  };
}
