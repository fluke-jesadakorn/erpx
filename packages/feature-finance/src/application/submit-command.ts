import { financeSubmissionIdentity, type FinanceSubmittedInput, type FinanceSubmissionPurpose, type PendingFinanceSubmission } from "../domain/submission";

export type FinanceSubmissionReply<Result> =
  | { readonly kind: "rejected"; readonly definitive: boolean; readonly message: string }
  | { readonly kind: "acknowledged"; readonly data: Result | undefined; acknowledge(): boolean };

export type FinanceSubmissionPorts<Completion, Result> = {
  hasAttempt(input: FinanceSubmittedInput): boolean;
  retainedCompletion(input: FinanceSubmittedInput): Completion | undefined;
  captureCompletion(): Completion | undefined;
  canBegin(purpose: FinanceSubmissionPurpose): boolean;
  trackPending(token: object, completion: Completion | undefined, pending: boolean): void;
  captureContext(): () => void;
  submit(input: FinanceSubmittedInput): Promise<FinanceSubmissionReply<Result>>;
  completionUnchanged(completion: Completion | undefined): boolean;
  refresh(): void;
  setPending(pending: boolean): void;
  setHasAttempt(pending: boolean): void;
  setError(message: string): void;
};

/** Interaction state is independent from rendering, HTTP, DOM and backend authority. */
export function createFinanceSubmission<Completion, Result>(
  attempts: Map<string, PendingFinanceSubmission<Completion>>,
  ports: FinanceSubmissionPorts<Completion, Result>,
) {
  const resolveAttempt = (identity: string, attempt: PendingFinanceSubmission<Completion>) => {
    if (attempts.get(identity) !== attempt) return;
    attempts.delete(identity);
    ports.trackPending(attempt.token, attempt.completion, false);
  };

  return async (input: FinanceSubmittedInput, purpose: FinanceSubmissionPurpose): Promise<Result | null> => {
    const assertCurrent = ports.captureContext();
    const contextStillCurrent = () => {
      try { assertCurrent(); return true; } catch { return false; }
    };
    const identity = financeSubmissionIdentity(input);
    if ([...attempts.keys()].some(previous => previous !== identity)) {
      ports.setError("Retry the original Finance action before submitting a different action. Your newer draft is retained.");
      return null;
    }
    const retry = ports.hasAttempt(input);
    if (!ports.canBegin(purpose) && !retry) {
      ports.setError("This Finance action is unavailable for the current package. Your draft is retained.");
      return null;
    }
    const previous = attempts.get(identity);
    const completion = previous?.completion ?? (retry ? ports.retainedCompletion(input) : ports.captureCompletion());
    const attempt = previous ?? { token: {}, completion, uncertain: false };
    attempts.set(identity, attempt);
    ports.setPending(true);
    ports.setError("");
    ports.setHasAttempt(true);
    ports.trackPending(attempt.token, attempt.completion, true);
    try {
      assertCurrent();
      const reply = await ports.submit(input);
      // Rejections and transport errors can also arrive after an account/role
      // change. They cannot release the old attempt or alter the new view.
      assertCurrent();
      if (reply.kind === "rejected") {
        if (!attempt.uncertain && reply.definitive) resolveAttempt(identity, attempt);
        throw new Error(reply.message);
      }
      if (reply.data === undefined) throw new Error("Finance acknowledgement is missing. Retry the same action.");
      assertCurrent();
      if (!completion && ports.captureCompletion()) {
        attempt.uncertain = true;
        ports.setError("The submitted Finance action was saved. Your newer draft is retained.");
        return null;
      }
      if (!reply.acknowledge()) throw new Error("Finance acknowledgement could not be verified. Retry the same action.");
      resolveAttempt(identity, attempt);
      ports.setHasAttempt(false);
      if (!ports.completionUnchanged(completion)) {
        ports.setError("The submitted Finance action was saved. Your newer draft is retained.");
        return null;
      }
      ports.refresh();
      return reply.data;
    } catch (cause) {
      if (attempts.get(identity) === attempt) attempt.uncertain = true;
      if (!contextStillCurrent()) return null;
      ports.setError(cause instanceof Error ? cause.message : "Connection interrupted. Retry the same action safely.");
      return null;
    } finally {
      if (contextStillCurrent()) ports.setPending(false);
    }
  };
}
