export type ExpiredUpload = { id: string; revision: number };
export function cleanupWindow(now: number) { return { expiredUploadBefore: now - 60000, pendingDraftBefore: now - 120000, batchLimit: 20 }; }
export function expiredUploadObjectKeys(holdingId: string, upload: ExpiredUpload): string[] { return [`${holdingId}/objects/${upload.id}/r${upload.revision}`, `${holdingId}/staging/${upload.id}`]; }
