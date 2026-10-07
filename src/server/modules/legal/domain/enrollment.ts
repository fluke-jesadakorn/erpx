import { z } from "zod";
export const legalEnrollmentAudience = "erpx-legal-enrollment";
export const legalEnrollmentClaims = z.object({ slug: z.string(), shareId: z.string() });
