import 'server-only';
import {ORG_NODES_SPACED_N_FROM} from './organization-nodes-spaced-projections';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function learningTalentSnapshotSources(){return {
  "learning_adapters_executive_metric_projections_organization_nodes_n_from": ORG_NODES_SPACED_N_FROM,
  "talent_adapters_executive_metric_projections_organization_nodes_n_from": ORG_NODES_SPACED_N_FROM
} as const;}
