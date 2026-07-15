export const PETTY_THRESHOLD_THB = 5_000;
export const CFO_THRESHOLD_THB   = 200_000;

export type Artifact = 'pr' | 'po' | 'gl' | 'paySlip' | 'none';

export function artifactForPip(pipKey: string, amountTHB: number | null): Artifact {
  const amt = amountTHB ?? 0;
  if (amt < PETTY_THRESHOLD_THB) return pipKey === 'final_authorization' ? 'gl' : 'none';
  switch (pipKey) {
    case 'submission':               return 'pr';
    case 'accounting_authorization': return 'po';
    case 'final_authorization':      return 'gl';
    default:                         return 'none';
  }
}
