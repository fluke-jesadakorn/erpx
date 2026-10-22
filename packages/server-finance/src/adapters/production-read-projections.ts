/** Owner-native joined facts retain the caller's single SQL snapshot and transaction. */export const productionReadProjection={
  "chart": "JOIN finance_charts c ON c.id=s.chart_id",
  "journal": "JOIN finance_journals j ON j.id=p.journal_id"
} as const;
