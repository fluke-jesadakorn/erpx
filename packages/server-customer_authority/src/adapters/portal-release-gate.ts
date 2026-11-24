import 'server-only';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/feature-availability';

const gateAlias='portal_release_gate';
const predicate=()=>scheduledFeaturePredicate(`${gateAlias}.company_id`,'customer_authority');

/** Aliased release-gate read shared by portal purchase/support/file commands. */
export const portalReleaseGateSql=()=>`SELECT ${predicate()} allowed FROM (SELECT ?::text company_id) ${gateAlias}`;

/** Wrapped scalar form for command assertions; keeps the gate inside the assertion. */
export const portalReleaseAssertionSql=()=>`(SELECT ${predicate()} FROM (SELECT ?::text company_id) ${gateAlias})`;
