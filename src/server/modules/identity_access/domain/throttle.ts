import { BusinessRuleError } from "../../shared/domain/error";

export function throttleIdentity(identity: string): string { return identity.toLowerCase(); }
export function assertThrottleAllowance(attempts: number | null, limit: number): void {
  if (attempts === null || attempts > limit) throw new BusinessRuleError(429, "IAM_RATE_LIMIT", "Too many attempts. Wait five minutes before trying again.");
}
