import "server-only";
import { fillPlanningReadTail, planningReadTailSources } from './planning-read-tail-contract';
/** Owner-local parameterized single-JSON-row subquery shared by the Planning attachment
 * lineage source. Every argument is a fixed server-owned identifier or predicate fragment;
 * no bind value or caller SQL is interpolated. Each emitted subquery is byte-identical to
 * the former inline literal it replaces. The exact template is owned by the native
 * read-tail contract. */
export const planningJsonbRowByKey = (table: string, alias: string, predicate: string) =>
  fillPlanningReadTail(planningReadTailSources.jsonbRowByKey, { table, alias, predicate });
