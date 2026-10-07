import { z } from 'zod';
import { flowKinds } from '@/lib/flow';
export const routingQuerySchema = z.object({ kind: z.enum(flowKinds).default('expense'), after: z.string().max(180).default(''), limit: z.coerce.number().int().min(1).max(50).default(25) });
