import type { Clock } from "@erpx/server-shared/index";
import { assertThrottleAllowance, throttleIdentity } from "@erpx/server-identity_access/domain/throttle";

export interface ThrottleRepository { increment(key: string, now: number, windowBefore: number): Promise<number | null> }
export function createIamThrottleApplication(repository: ThrottleRepository, clock: Clock, opaqueKey: (category: string, identity: string) => string) {
  return {
    async check(category: string, identity: string, limit = 8, windowMs = 300000): Promise<void> {
      const now = clock.now();
      const attempts = await repository.increment(opaqueKey(category, throttleIdentity(identity)), now, now - windowMs);
      assertThrottleAllowance(attempts, limit);
    },
  };
}
