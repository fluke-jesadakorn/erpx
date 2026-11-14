/** The submitted bytes and rendered authority scope identify a retryable UI attempt. */
export type FinanceCommandIdentity = {
  readonly target: string;
  readonly method: string;
  readonly scope: readonly (string | undefined)[];
  readonly body: string;
};

export function financeCommandSignature(identity: FinanceCommandIdentity): string {
  return JSON.stringify([identity.target, identity.method, identity.scope, identity.body]);
}

export type FinanceCommandAttempt<Completion> = {
  readonly key: string;
  readonly completion: Completion;
};
