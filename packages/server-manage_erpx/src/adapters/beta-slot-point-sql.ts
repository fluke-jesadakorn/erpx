/** One parameterized beta_slots point-read family shared by the runtime active-slot
 * probe and the onboarding owner-slot projection. Only fixed projections, key
 * columns and an optional trailing predicate are admitted; callers keep their
 * own binds and statements, and no request text is interpolated. */
import {betaPointByKeySql} from './beta-point-by-key-sql';
export type BetaSlotPointProjection = '1' | 'slug,company_name,branch_name,state';
export type BetaSlotPointKey = 'holding_id' | 'owner_user_id';
export const betaSlotPointSql = (
  projection: BetaSlotPointProjection,
  key: BetaSlotPointKey,
  tail = '',
) => betaPointByKeySql('slots', projection, key, tail);
