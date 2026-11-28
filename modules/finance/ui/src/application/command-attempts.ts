import type { FinanceCommandAttempt } from "../domain/command-attempt";

export type FinanceCommandAttemptPorts<Completion> = {
  createKey(): string;
  retainCompletion(): Completion;
};

/** A lost or unverified response keeps the original attempt and its completion snapshot. */
export function createFinanceCommandAttempts<Completion>(ports: FinanceCommandAttemptPorts<Completion>) {
  const attempts = new Map<string, FinanceCommandAttempt<Completion>>();

  return {
    retain(signature: string): FinanceCommandAttempt<Completion> {
      const previous = attempts.get(signature);
      if (previous) return previous;
      const attempt = { key: ports.createKey(), completion: ports.retainCompletion() };
      attempts.set(signature, attempt);
      return attempt;
    },
    has(signature: string): boolean {
      return attempts.has(signature);
    },
    completion(signature: string): Completion | undefined {
      return attempts.get(signature)?.completion;
    },
    acknowledge(signature: string, original: FinanceCommandAttempt<Completion>): boolean {
      // A delayed acknowledgement must not release a later attempt with identical input.
      if (attempts.get(signature) !== original) return false;
      attempts.delete(signature);
      return true;
    },
  };
}
