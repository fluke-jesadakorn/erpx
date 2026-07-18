export interface CountParams {
  noun: string;
  formatted: string;
}

export interface NumberParams {
  formatted: string;
}

export interface ResultParams {
  bullets: string;
}

export interface FallbackParams {
  rows: number;
}

export interface SqlSummaryTemplates {
  empty: string;
  count: (p: CountParams) => string;
  average: (p: NumberParams) => string;
  sum: (p: NumberParams) => string;
  result: (p: ResultParams) => string;
  fallback: (p: FallbackParams) => string;
}

export const en: SqlSummaryTemplates = {
  empty: 'No matching records found.',
  count: ({ noun, formatted }) =>
    `There are${noun ? ' ' + noun : ''} ${formatted} in total.`,
  average: ({ formatted }) =>
    `The average is ${formatted}.`,
  sum: ({ formatted }) =>
    `The total is ${formatted}.`,
  result: ({ bullets }) => `Result: ${bullets}`,
  fallback: ({ rows }) => `${rows} records returned.`,
};