import 'server-only';
import {scheduledFeaturePredicate} from '@erpx/pms-server/public/feature-availability';
import {releaseGateRead} from './customer-authority-read-tail-r28-sources';
import {companyScopeSelect} from './customer-authority-read-tail-r33-sources';

const gateAlias='portal_release_gate';
const predicate=()=>scheduledFeaturePredicate(`${gateAlias}.company_id`,'customer_authority');
/** One canonical source so the gate Company alias cannot drift between the
 * purchase/access read and its wrapped command-assertion form. */
const gateSource=`(${companyScopeSelect()}) ${gateAlias}`;

/** Aliased release-gate read shared by portal purchase/support/file commands. */
export const portalReleaseGateSql=()=>releaseGateRead(predicate(),gateSource,' allowed');

/** Wrapped scalar form for command assertions; keeps the gate inside the assertion. */
export const portalReleaseAssertionSql=()=>`(${releaseGateRead(predicate(),gateSource,'')})`;
