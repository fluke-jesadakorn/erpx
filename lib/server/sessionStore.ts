import 'server-only';
import { query } from '../db';
import type { SessionPayload } from './sessionToken';

export async function validateActiveSession(payload: SessionPayload): Promise<SessionPayload | null> {
  try {
    const result = await query<{ user_id: number }>(
      `SELECT s.user_id
         FROM auth.sessions s
         JOIN users u ON u.id = s.user_id AND u.is_active IS TRUE
        WHERE s.id = $1
          AND s.user_id = $2
          AND s.revoked_at IS NULL
          AND s.expires_at > now()
        LIMIT 1`,
      [payload.id, payload.sub],
    );
    return result.rows.length > 0 ? payload : null;
  } catch {
    return null;
  }
}
