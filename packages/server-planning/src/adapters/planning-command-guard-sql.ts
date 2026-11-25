import "server-only";
import {planningPackageFactRows} from "./planning-package-fact-sql";
/** Owner-local SQL fragment shared by the Planning command-source guards.
 * Every argument is a fixed server-owned table name or predicate fragment; no
 * bind value or caller SQL is interpolated. Each emitted statement is
 * byte-identical to the former inline literal it replaces. */
export const planningCommandExistsGuard = (table: string, predicate: string) =>
  `EXISTS(${planningPackageFactRows('1', table, predicate)})`;
