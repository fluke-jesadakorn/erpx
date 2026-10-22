import { z } from "zod";
export const metadata = z.object({
    id: z.string().uuid(),
    name: z.string().trim().min(1).max(255),
    type: z.enum([
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/heic",
        "image/heif",
        "application/pdf",
        "text/csv",
    ]),
});
