import {z} from 'zod';
export const invitationSchema = z.object({
  email: z.email().transform((v) => v.toLowerCase()),
  branchId: z.string().min(1).max(160),
  role: z.enum(["employee", "it_admin"]).default("employee"),
});

