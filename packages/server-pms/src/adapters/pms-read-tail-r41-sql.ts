/** Byte-identical PMS read-tail r41 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, the r17-r40
 * fragments and the native read registry stay untouched. */
import { PMS_HOME_ACTIVE_ROOM_COUNT_SQL, PMS_HOME_STAY_STATE_COUNT_SQL } from './pms-read-tail-r38-sql';
import { pmsHomeGuestAccountSumSql } from './pms-read-tail-r39-sql';

type GuestIdJoin = 'email' | 'reservationEmail' | 'operations';
const GUEST_ID_JOIN: Record<GuestIdJoin, { projection: string; join: string; on: string }> = {
  email: { projection: "g.data_json::jsonb->>'email' email", join: ' JOIN pms_records g', on: '' },
  reservationEmail: { projection: 'r.id,r.property_id', join: ' JOIN pms_records g', on: " AND g.property_id=r.property_id AND g.kind='guest'" },
  operations: { projection: 'r.id', join: '\n   LEFT JOIN pms_records g', on: " AND g.property_id=r.property_id AND g.kind='guest'" },
};
/** Rank 1: guest-id self-join head. The JOIN/LEFT JOIN token and the extra ON
 * predicates are closed variants here; WHERE/ORDER/LIMIT and binds stay at sites. */
export const pmsGuestIdJoinSql = (kind: GuestIdJoin): string => {
  const { projection, join, on } = GUEST_ID_JOIN[kind];
  return `SELECT ${projection} FROM pms_records r${join} ON g.id=r.data_json::jsonb->>'guestId'${on}`;
};

type PropertyScope = 'hotelMetrics' | 'hotelMoney' | 'liabilities';
const PROPERTY_SCOPE: Record<PropertyScope, { cte: string; projection: string; from: string; suffix: string }> = {
  hotelMetrics: {
    cte: '',
    projection: `p.id,p.name,p.branch_id,p.business_date,p.state,p.config_json::jsonb->>'currency' currency,
    (${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}) room_count,
    (${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}
      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,
    (${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}
      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'
        AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=p.business_date AND b.data_json::jsonb->>'roomId'=r.id)) capacity,
    (${PMS_HOME_STAY_STATE_COUNT_SQL} IN ('in_house','checked_out')
      AND s.data_json::jsonb->>'arrival'<=p.business_date AND s.data_json::jsonb->>'departure'>p.business_date) sold,
    (${PMS_HOME_STAY_STATE_COUNT_SQL}='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,
    (${PMS_HOME_STAY_STATE_COUNT_SQL}='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures
    `,
    from: 'FROM pms_properties p ',
    suffix: '',
  },
  hotelMoney: {
    cte: `WITH RECURSIVE entry_roots AS (
      SELECT r.id,r.property_id,r.id root_id FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'
      UNION ALL SELECT r.id,r.property_id,e.root_id FROM pms_records r JOIN entry_roots e ON r.property_id=e.property_id AND r.data_json::jsonb->>'originalId'=e.id
        WHERE r.kind='entry' AND r.id<>e.id
    ) `,
    projection: `p.id,
      COALESCE((SELECT sum((r.data_json::jsonb->>'net')::bigint) FROM pms_records r JOIN entry_roots e ON e.id=r.id AND e.property_id=r.property_id
        WHERE r.property_id=p.id AND r.data_json::jsonb->>'date'=p.business_date AND r.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) room_revenue,
      COALESCE((${pmsHomeGuestAccountSumSql('receivable')}),0) receivable,
      COALESCE((${pmsHomeGuestAccountSumSql('advance')}),0) advance,
      COALESCE((${pmsHomeGuestAccountSumSql('deposit')}),0) deposit
    `,
    from: 'FROM pms_properties p ',
    suffix: '',
  },
  liabilities: {
    cte: '',
    projection: `p.id,
    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.refund_liability' THEN (e.data_json::jsonb->>'total')::bigint
      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'<>'payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) refund_payable,
    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint
      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) unallocated
    `,
    from: `FROM pms_properties p LEFT JOIN pms_records e ON e.property_id=p.id AND e.kind='entry'
    LEFT JOIN pms_records source ON source.property_id=p.id AND source.id=e.data_json::jsonb->>'originalId' AND source.kind='entry'
    `,
    suffix: ' GROUP BY p.id',
  },
};
const PROPERTY_SCOPE_WHERE = 'WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)';
/** Rank 2: property-scope tail family. The byte-exact projection variants,
 * optional CTE FROM/suffix and the shared WHERE tail are closed here; the
 * finance transfer tail (t.company_id) is deliberately not a member. */
export const pmsPropertyScopeSql = (kind: PropertyScope): string => {
  const { cte, projection, from, suffix } = PROPERTY_SCOPE[kind];
  return `${cte}SELECT ${projection}${from}${PROPERTY_SCOPE_WHERE}${suffix}`;
};

type RolloutKind = 'room' | 'rate';
/** Rank 3: correlated readiness witness; the kind is a closed union, not caller SQL. */
export const pmsRolloutReadyExistsSql = (kind: RolloutKind): string =>
  `EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='${kind}')`;
