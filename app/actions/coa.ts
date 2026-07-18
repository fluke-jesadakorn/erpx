'use server';

import { applyCoaSuggestionCore, type ApplyCoaSuggestionArgs, type ApplyCoaResult } from './coaCore';

export type { ApplyCoaSuggestionArgs, ApplyCoaResult } from './coaCore';

export async function applyCoaSuggestionAction(
  args: ApplyCoaSuggestionArgs,
): Promise<ApplyCoaResult> {
  return applyCoaSuggestionCore(args);
}

export { applySoCoaAction, suggestSoCoa, canActAtSalesRecording } from '@folio-lib/sales/coa';