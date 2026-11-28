export type { FinanceCommandIdentity, FinanceCommandAttempt } from "../domain/command-attempt";
export type { FinanceSubmissionPurpose, FinanceSubmittedInput } from "../domain/submission";

/** Render model for owner-computed GL amounts; no calculation policy here. */
export type FinancePostingPreview =
 | {state:'voided'}
 | {state:'no-charge-confirmed'}
 | {state:'configuration-required'}
 | {state:'ready';posted:boolean;accounts:Record<string,string|number|null>[];lines:{id:string;debit:number;credit:number}[];debit:number;credit:number};
