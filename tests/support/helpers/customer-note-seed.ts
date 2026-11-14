import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Native note commands own their receipts/audit. Tests that only need the
 * immutable owner rows seed the exact `customer_contact_notes` and
 * `customer_contact_note_history` facts those commands persist. */
export async function seedCustomerNote(
  database: SqlDatabase,
  facts: {
    companyId: string;
    customerId: string;
    orderId?: string | null;
    channel: 'phone' | 'email' | 'meeting' | 'other';
    occurredAt: number;
    body: string;
    createdBy: string;
    now?: number;
  },
) {
  const id = `contact-note-${crypto.randomUUID()}`;
  const now = facts.now ?? Date.now();
  await database.batch([
    database
      .prepare(
        'INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,\'active\',1,?,?,?)',
      )
      .bind(
        id,
        facts.companyId,
        facts.customerId,
        facts.orderId ?? null,
        facts.channel,
        facts.occurredAt,
        facts.body,
        facts.createdBy,
        now,
        now,
      ),
    database
      .prepare(
        "INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) VALUES(?,?,1,'create',?,?,?,?)",
      )
      .bind(
        `contact-note-history-${crypto.randomUUID()}`,
        id,
        facts.body,
        facts.orderId
          ? 'Initial contact record'
          : 'Initial standalone customer contact record',
        facts.createdBy,
        now,
      ),
  ]);
  return {id, version: 1, status: 'active' as const};
}

export async function seedCustomerNoteChange(
  database: SqlDatabase,
  facts: {
    noteId: string;
    action: 'correct' | 'archive';
    expectedVersion: number;
    body?: string;
    reason: string;
    actorId: string;
    now?: number;
  },
) {
  const current = await database
    .prepare('SELECT body FROM customer_contact_notes WHERE id=?')
    .bind(facts.noteId)
    .first<{body: string}>();
  if (!current) throw new Error('CUSTOMER_NOTE_FIXTURE_REQUIRED');
  const version = facts.expectedVersion + 1;
  const content = facts.action === 'correct' ? (facts.body ?? current.body) : current.body;
  const status = facts.action === 'archive' ? 'archived' : 'active';
  const now = facts.now ?? Date.now();
  await database.batch([
    database
      .prepare(
        'UPDATE customer_contact_notes SET body=?,status=?,version=version+1,updated_at=? WHERE id=?',
      )
      .bind(content, status, now, facts.noteId),
    database
      .prepare(
        'INSERT INTO customer_contact_note_history(id,note_id,version,action,body,reason,actor_id,created_at) VALUES(?,?,?,?,?,?,?,?)',
      )
      .bind(
        `contact-note-history-${crypto.randomUUID()}`,
        facts.noteId,
        version,
        facts.action,
        content,
        facts.reason,
        facts.actorId,
        now,
      ),
  ]);
  return {id: facts.noteId, version, status};
}
