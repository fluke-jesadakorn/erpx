import {z} from "zod";
const reason=z.string().trim().min(3).max(3000);
export const learningCourseSchema=z.object({
  title: z.string().trim().min(3).max(160),
  description: reason,
  provider: z.string().trim().min(2).max(160),
  durationMinutes: z.number().int().positive(),
  requiredFor: z.string().trim().min(2).max(120).nullable().optional(),
});
export type LearningCourseInput=z.infer<typeof learningCourseSchema>;
