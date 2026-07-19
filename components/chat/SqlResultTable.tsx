'use client';

import { HtmlReportView } from './HtmlReportView';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

interface Row {
  [k: string]: unknown;
}

export function SqlResultTable({
  question,
  sql,
  columns,
  rows,
  rowCount,
  explanation,
}: {
  question?: string;
  sql: string;
  columns: string[];
  rows: Row[];
  rowCount: number;
  explanation?: string;
}) {
  const lang = useSecondaryLocale();
  const rawTitle = lang === 'th' ? 'ผลลัพธ์ Raw SQL' : lang === 'de' ? 'Raw-SQL-Ergebnis' : 'Raw SQL result';
  const title = question && !/^(SELECT|WITH|```sql)/i.test(question.trim()) ? question : rawTitle;
  return (
    <HtmlReportView
      title={title}
      columns={columns}
      rows={rows}
      rowCount={rowCount}
      sql={sql}
      explanation={explanation}
      lang={lang}
    />
  );
}
