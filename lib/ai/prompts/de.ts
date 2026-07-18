import type { SqlSummaryTemplates } from './en';

export const de: SqlSummaryTemplates = {
  empty: 'Keine passenden Datensätze gefunden.',
  count: ({ noun, formatted }) =>
    `Es${noun ? ' ' + noun : ''}gibt insgesamt ${formatted} Einträge.`,
  average: ({ formatted }) =>
    `Der Durchschnitt liegt bei ${formatted}.`,
  sum: ({ formatted }) =>
    `Die Summe beträgt ${formatted}.`,
  result: ({ bullets }) => `Ergebnis: ${bullets}`,
  fallback: ({ rows }) => `${rows} Datensätze gefunden.`,
};