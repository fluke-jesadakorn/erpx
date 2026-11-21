import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people/payroll Thai statutory profile statements. */
export function payrollStatutory(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    insertProfile: (f: {
      id: string; companyId: string; engagementId: string; taxYear: number; effectiveFrom: string;
      effectiveTo: string; annualDeductionsMinor: number; scheduledPaysInYear: number;
      socialSecurityEligible: boolean; exemptionEvidence: string | null; openingThroughDate: string;
      openingTaxableMinor: number; openingIrregularMinor: number; openingWithheldMinor: number;
      priorIncomeComplete: boolean; evidenceRef: string; userId: string; now: number; now2: number;
    }) => call(f.companyId, 'payroll_statutory_profile_insert',
      [f.id, f.companyId, f.engagementId, f.taxYear, f.effectiveFrom, f.effectiveTo, f.annualDeductionsMinor,
       f.scheduledPaysInYear, f.socialSecurityEligible, f.exemptionEvidence, f.openingThroughDate,
       f.openingTaxableMinor, f.openingIrregularMinor, f.openingWithheldMinor, f.priorIncomeComplete,
       f.evidenceRef, f.userId, f.now, f.now2]),
    decideProfile: (f: { status: string; status2: string; userId: string; now: number; id: string; companyId: string }) =>
      call(f.companyId, 'payroll_statutory_profile_decide', [f.status, f.status2, f.userId, f.now, f.id]),
  };
}
