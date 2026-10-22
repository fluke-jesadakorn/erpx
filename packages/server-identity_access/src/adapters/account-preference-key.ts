import "server-only";
import { createHash } from "node:crypto";
export const accountPreferenceKey = (userId: string) => createHash("sha256").update("account-profile:").update(userId).digest("hex");
