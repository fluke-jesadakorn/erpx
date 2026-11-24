import "server-only";
/** Owner-local parameterized projection/FROM head shared by the Planning command-source
 * executive projections. Every argument is a fixed server-owned projection or FROM
 * fragment; no bind value or caller SQL is interpolated. `indent` carries the fixed
 * whitespace before FROM so each emitted statement is byte-identical to the former
 * inline literal it replaces. */
export const planningSelectFrom = (columns: string, indent: string, from: string) =>
  `SELECT ${columns}\n${indent}FROM ${from}`;
