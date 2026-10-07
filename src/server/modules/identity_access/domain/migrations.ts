import { z } from 'zod';
import { grantInput } from './iam-service';
export const migrationInput = z.object({
    connectionId: z.string(),
    users: z
        .array(z.object({
        localUserId: z.string(),
        identityId: z.string(),
        personId: z.string(),
        grants: z.array(grantInput.omit({ userId: true })).min(1),
    }))
        .min(1)
        .max(200),
    reason: z.string().trim().min(3).max(1000),
});
export type MigrationInput = z.infer<typeof migrationInput>;
export type MigrationMember = MigrationInput['users'][number];
export type MigrationBatch = {
    id: string;
    version: number;
    status: string;
    connection_id: string;
    users_json: unknown;
};
export type MigrationPerson = {
    id: string;
    status: string;
};
export type MigrationIdentity = {
    id: string;
    name: string;
    external_id: string;
};
