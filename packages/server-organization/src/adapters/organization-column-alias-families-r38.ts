import 'server-only';

/** r38 additions: placeholder-style-parameterized active-position lookup.
 * The two inline literals differ only in placeholder style ($1/$2 vs ?) and are
 * otherwise byte-identical; each call renders exactly the literal it replaces.
 * Callers keep their own binds. Text only: no r17-r37 helper, lock or spaced
 * JOIN constant is changed. The FOR UPDATE variant in people-lifecycle-facts
 * is a separate locked statement and stays byte-identical. */
export const activePositionById = (companyPlaceholder: string, idPlaceholder: string) =>
  `SELECT * FROM organization_positions WHERE company_id=${companyPlaceholder} AND id=${idPlaceholder} AND active=1`;
