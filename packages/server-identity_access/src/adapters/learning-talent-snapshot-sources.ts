import 'server-only';
import { assignmentsProjection } from './assignment-projection-source';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function learningTalentSnapshotSources(){return {
  "learning_adapters_learning_assignments_x_from": `FROM (${assignmentsProjection}) x`,
  "talent_adapters_talent_assignments_x_from": `FROM (${assignmentsProjection}) x`,
  "talent_adapters_talent_assignments_from": `FROM (${assignmentsProjection}) assignments`
} as const;}
