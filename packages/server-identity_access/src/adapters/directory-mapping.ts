import 'server-only';
import { z } from 'zod';
import type { PoolClient } from 'pg';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import { ApiError } from '@erpx/server-runtime/public/http';
import { iamDb } from '@erpx/server-identity_access/public/iam-db';
import { iamCommand } from '@erpx/server-identity_access/public/iam-transactions';
import type { IamActor } from '@erpx/server-identity_access/public/iam-context';
import { proposeIamReview } from '@erpx/server-identity_access/public/iam-service';
import { grantInput } from '@erpx/server-identity_access/public/iam-service';
import { validateIamGrant } from '@erpx/server-identity_access/public/iam-grants';
import { mappingInput } from '@erpx/server-identity_access/domain/directories';

export { mappingInput } from '@erpx/server-identity_access/domain/directories';

async function knownGroup(c: PoolClient, actor: IamActor, input: z.infer<typeof mappingInput>) {
  return (await c.query(
    'SELECT 1 FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 AND c.id=$2 AND g.external_id=$3',
    [actor.companyId, input.connectionId, input.groupId],
  )).rowCount;
}

async function previewGroups(c: PoolClient, actor: IamActor, input: z.infer<typeof mappingInput>) {
  return (await c.query(
    'SELECT p.snapshot_json FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.id=$1 AND c.company_id=$2',
    [input.connectionId, actor.companyId],
  )).rows[0] ?? null;
}

/** Propose one reviewed Directory group-to-role mapping. */
export async function proposeMapping(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof mappingInput>,
  key: string,
) {
  const database = iamDb(db);
  return iamCommand(database, actor, key, 'mapping.propose', input, 'iam.manage', async (c) => {
    if (!(await knownGroup(c, actor, input))) {
      const preview = await previewGroups(c, actor, input);
      const groups = (preview?.snapshot_json as { groups?: { externalId?: string }[] } | undefined)?.groups ?? [];
      if (!groups.some((group) => group.externalId === input.groupId))
        throw new ApiError(404, 'GROUP_NOT_FOUND', 'Test and preview the selected group first.');
    }
    const grant = grantInput.parse({ ...input, userId: 'directory-mapping-target' });
    await validateIamGrant(c, actor, grant);
    return proposeIamReview(
      c,
      actor,
      'mapping.activate',
      {
        ...input,
        expectedRoleRevision: grant.expectedRoleRevision,
        parentId: actor.assignmentId,
      },
      input.reason,
    );
  });
}
