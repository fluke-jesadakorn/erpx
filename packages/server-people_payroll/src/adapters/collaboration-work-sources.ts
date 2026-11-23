import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { Row } from '@erpx/server-collaboration/public/collaboration-work';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { randomUUID } from 'node:crypto';
import { nativeCollaborationReadStatement } from '@erpx/server-collaboration/adapters/native-card-history-statement';

/** Native-owned eligible-people read for the Collaboration workspace. The
 * SQL text is issued by the owner against the authorized Company selection;
 * the caller executes it inside its own serializable unit. */
export function collaborationWorkSources(db: SqlDatabase) {
    return { async eligiblePeople(actor: WorkContext) {
            const read = await nativeCollaborationReadStatement(db, actor, 'collaboration_eligible_people', 'people', randomUUID(), Date.now());
            return { results: read.results as Row[] };
        } };
}
