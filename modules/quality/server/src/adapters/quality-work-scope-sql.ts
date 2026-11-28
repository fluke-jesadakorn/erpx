import {scopeSql} from "./quality-scope-sql";
/** Derived company/branch/department tuple shared by the Quality current-scope readers. */
export const qualityWorkScopeTuple = (columns: string, alias: string, separator = " ") => `${scopeSql(columns)}${separator}${alias}`;
