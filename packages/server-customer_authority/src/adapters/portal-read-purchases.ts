import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commercePackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
/** The alias and purpose are code-owned. Readers keep original verified
 * identity, approved binding, expiry, actual order and private file policy. */
export async function customerPortalPurchaseScope(_database:SqlDatabase,alias:'b'|'binding',purpose:CapabilityPurpose='history'){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return 'TRUE';
 const release=scheduledFeaturePredicate(`${alias}.company_id`,'customer_authority');return mode==='shadow'?release:`(EXISTS(WITH portal_purchase_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM portal_purchase_clock WHERE ${purchasedCapabilityPredicate(`${alias}.company_id`,'sales.portal',purpose,'portal_purchase_clock.now_ms')}) AND ${release})`;
}
