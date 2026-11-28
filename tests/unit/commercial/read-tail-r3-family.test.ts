import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';

const src = (name: string) => readFileSync(new URL(`../../../modules/commercial/server/src/adapters/${name}`, import.meta.url), 'utf8');

describe('Commercial read-tail r3 retired seat-history family', () => {
  test('the r3 TS builder module is deleted and no consumer references it', () => {
    expect(() => src('commercial-read-tail-r3-sources.ts')).toThrow();
    const seats = src('seats.ts');
    expect(seats).not.toContain('seatHistorySql');
    expect(seats).toContain('readNativeSeatHistory');
  });

  test('native lane 37 keeps the three byte-identical seat-history SQL oracles', () => {
    const rust = src('native_lane_37/domain.rs');
    for (const symbol of ['SQL_ASSIGNMENTS', 'SQL_MEMBERS', 'SQL_ROLES']) expect(rust).toContain(symbol);
    for (const table of ['billing_seat_assignment_events', 'billing_seat_member_events', 'billing_seat_role_events'])
      expect(rust).toContain(table);
    expect(rust).toContain('seatHistorySql');
  });

  test('no commercial TS adapter renders the retired seat-history SELECT', () => {
    const seats = src('seats.ts');
    for (const table of ['billing_seat_assignment_events', 'billing_seat_member_events', 'billing_seat_role_events'])
      expect(seats).not.toContain(table);
  });
});
