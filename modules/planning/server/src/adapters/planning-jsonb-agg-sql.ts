import "server-only";
import { fillPlanningReadTail, planningReadTailSources } from './planning-read-tail-contract';
/** Owner-local parameterized ordered JSONB aggregate subquery shared by the Planning
 * attachment lineage source. Every argument is a fixed server-owned projection,
 * ordering, FROM or predicate fragment; no bind value or caller SQL is
 * interpolated. The FROM fragment carries its own leading whitespace so each
 * emitted subquery is byte-identical to the former inline literal it replaces.
 * The exact template is owned by the native read-tail contract. */
export const planningJsonbAggOrdered = (projection: string, order: string, from: string, predicate: string) =>
  fillPlanningReadTail(planningReadTailSources.jsonbAggOrdered, { projection, order, from, predicate });
