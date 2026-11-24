/** Derived company/branch/department tuple shared by the Quality current-scope readers. */
export const qualityWorkScopeTuple = (columns: string, alias: string, separator = " ") => `(SELECT ${columns})${separator}${alias}`;
