import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const push = src('web-push.ts');
const planner = src('notification_statement.rs');

const READS = {
  subscriptionTable: ['web_push_subscription_table', "SELECT to_regclass('push_subscriptions') AS name"],
  fanoutSubscriptions: ['web_push_fanout_subscriptions', 'SELECT s.*,p.* FROM push_subscriptions s JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id WHERE p.enabled=true ORDER BY s.scan_at,s.id LIMIT 25'],
  newDeliveries: ['web_push_new_deliveries', 'SELECT d.id,d.delivered_at,e.category FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE d.company_id=? AND d.user_id=? AND NOT erpx_field_push_source(e.source_type,e.source_id,e.company_id) AND (d.delivered_at,d.id)>(?,?) ORDER BY d.delivered_at,d.id LIMIT 50'],
  currentDelivery: ['web_push_current_delivery', 'SELECT j.*,s.user_id,s.session_id,s.origin,s.inbox_path,s.sealed_subscription,p.enabled,p.work_enabled,p.mention_enabled,p.system_enabled,p.calendar_enabled,p.notice_enabled,p.quiet_start_minute,p.quiet_end_minute,d.read_at,d.archived_at,e.category FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id JOIN inbox_deliveries d ON d.id=j.inbox_delivery_id JOIN inbox_events e ON e.id=d.event_id WHERE j.id=? AND j.lease_token=?'],
} as const;

describe('Web-push read oracles on the native planner', () => {
  it('planner owns all four retired reads byte for byte', () => {
    for (const [kind, sql] of Object.values(READS)) {
      expect(planner).toContain(`"${kind}" => "${sql}"`);
      expect(sql.match(/SELECT /g) ?? []).toHaveLength(1);
    }
  });

  it('adapter sends only kind + ordered values, with no SQL tokens left', () => {
    expect(push.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(push).not.toContain("SELECT to_regclass('push_subscriptions')");
    expect(push).not.toContain('SELECT s.*,p.* FROM push_subscriptions');
    expect(push).toContain("ownerRead(db, 'system', 'web_push_subscription_table', [])");
    expect(push).toContain("ownerRead(db, 'system', 'web_push_fanout_subscriptions', [])");
    expect(push).toContain("ownerRead(db, company_id, 'web_push_new_deliveries', [company_id, user_id, cursor_at, cursor_id])");
    expect(push).toContain("ownerRead(db, 'system', 'web_push_current_delivery', [id, lease_token])");
    expect(push).toContain('nativeSystemNotificationStatement(db, company,');
    expect(push).toContain('values_json: JSON.stringify(values)');
  });

  it('keeps the lane-16 native push reads and all owner writes untouched', () => {
    expect(push).toContain('pushEndpointOwner, pushPreferences, pushSessionSubscriptionCount');
    expect(push).toContain("write(companyId, 'push_preferences_save'");
    expect(push).toContain("scoped('push_subscription_revoke'");
  });
});

describe('retirement:ts-statements', () => {
  it('drops 306 -> 302 with the web-push reads retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(302);
    expect(total).toBeLessThan(306);
  });
});
