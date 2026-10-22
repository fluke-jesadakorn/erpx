import {z} from "zod";
const reason=z.string().trim().min(3).max(3000),date=z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
export const talentRequisitionSchema=z.object({
  jobTitle: z.string().trim().min(2).max(160),
  openings: z.number().int().positive().max(1000),
  targetStartDate: date,
  description: reason,
});
export type TalentRequisitionInput=z.infer<typeof talentRequisitionSchema>;
