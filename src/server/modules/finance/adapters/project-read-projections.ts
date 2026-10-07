/** Native joined facts preserve the caller's transaction and original SQL snapshot. */ export const projectReadProjection = {
    "allocationLine": "JOIN finance_journal_lines l ON l.id=a.journal_line_id",
    "expenseJournal": "JOIN finance_journals j ON j.id=l.journal_id",
    "expenseAccount": "JOIN finance_accounts ac ON ac.id=l.account_id",
    "costLine": "JOIN finance_journal_lines l ON l.id=c.journal_line_id",
    "actualLine": "JOIN finance_journal_lines line ON line.id=c.journal_line_id",
    "actualJournal": "JOIN finance_journals j ON j.id=line.journal_id"
} as const;
