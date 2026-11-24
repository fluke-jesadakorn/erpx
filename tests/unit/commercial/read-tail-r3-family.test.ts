import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {seatHistorySql} from '../../../packages/server-commercial/src/adapters/commercial-read-tail-r3-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-commercial/src/adapters/${name}`, import.meta.url), 'utf8');
const ORIGINALS = {
  assignment: "SELECT event_id,assignment_id,captured_at,present,state::text state_json FROM billing_seat_assignment_events WHERE captured_at<? ORDER BY assignment_id,captured_at,event_id",
  member: "SELECT event_id,company_id,user_id,captured_at,present,state::text state_json FROM billing_seat_member_events WHERE captured_at<? ORDER BY company_id,user_id,captured_at,event_id",
  role: "SELECT event_id,company_id,role_id,captured_at,present,state::text state_json FROM billing_seat_role_events WHERE captured_at<? ORDER BY company_id,role_id,captured_at,event_id",
};

describe('Commercial read-tail r3 parameterized family', () => {
  test('builder reproduces each original SELECT byte-identically', () => {
    expect(seatHistorySql("assignment_id","assignment","assignment_id")).toBe(ORIGINALS.assignment);
    expect(seatHistorySql("company_id,user_id","member","company_id,user_id")).toBe(ORIGINALS.member);
    expect(seatHistorySql("company_id,role_id","role","company_id,role_id")).toBe(ORIGINALS.role);
  });

  test('builder SQL is pure read and free of writes/locks', () => {
    for (const sql of [ORIGINALS.assignment, ORIGINALS.member, ORIGINALS.role]) {
      expect(sql.toUpperCase()).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('every consumer keeps its own prepare/bind and no inline literal remains', () => {
    const seats = src('seats.ts');
    expect(seats).toContain('seatHistorySql');
    for (const sql of Object.values(ORIGINALS)) expect(seats, sql).not.toContain(sql);
    expect(seats.split('seatHistorySql(')).toHaveLength(4); // 1 import + 3 call sites
  });

  test('shared module declares the literal exactly once', () => {
    expect(src('commercial-read-tail-r3-sources.ts').split('SELECT ')).toHaveLength(2); // one literal in the builder template
  });
});
