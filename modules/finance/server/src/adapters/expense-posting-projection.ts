import 'server-only';
/**
 * The two Finance-local expense posting reads are now owned by the native lane
 * 1 axum router. This adapter is a thin transport re-export; it contains no
 * SQL, no projection and no fallback. The public package keeps its original
 * export names and row shape for the People Payroll consumer.
 */
export {
  expenseJournalProjection,
  expensePostingAccounts,
  type ExpenseLedgerRow,
} from './native-lane-1/client';
