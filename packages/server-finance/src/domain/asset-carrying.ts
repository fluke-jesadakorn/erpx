export type AssetCarryBook = {
  chartId: string;
  costAccount: string;
  accumulatedAccount: string;
  ruleId: string | null;
};
export type AssetCarryBasis = {
  ordinal: number;
  throughDate: string;
  books: Array<{ chartId: string; accumulated: string; units: number }>;
};

export type AssetCarryingRow=Record<string,string|number|null>;
