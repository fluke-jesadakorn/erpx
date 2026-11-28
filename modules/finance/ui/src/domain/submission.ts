export type FinanceSubmissionPurpose = "new-work" | "settle-existing";

export type PendingFinanceSubmission<Completion> = {
  readonly token: object;
  readonly completion: Completion | undefined;
  uncertain: boolean;
};

export type FinanceSubmittedInput = {
  readonly target: string;
  readonly body: string;
};

export function financeSubmissionIdentity(input: FinanceSubmittedInput): string {
  return JSON.stringify([input.target, input.body]);
}
