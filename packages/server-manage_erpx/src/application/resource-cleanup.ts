import type { Clock } from "@erpx/server-shared/index";
import { cleanupWindow, expiredUploadObjectKeys, type ExpiredUpload } from "@erpx/server-manage_erpx/domain/resource-cleanup";

export interface ResourceCleanupRepository {
  expiredUploads(before: number, limit: number): Promise<ExpiredUpload[]>;
  uploadHasAttachment(uploadId: string): Promise<boolean>;
  expireUpload(uploadId: string, before: number, now: number): Promise<void>;
  expireAiReservations(pendingDraftBefore: number, now: number): Promise<void>;
}
export function createResourceCleanupApplication(ports: { repository: ResourceCleanupRepository; clock: Clock; objects: { delete(keys: string[]): Promise<void> } }) {
  return {
    async cleanup(holdingId: string): Promise<void> {
      const now = ports.clock.now(), window = cleanupWindow(now);
      for (const upload of await ports.repository.expiredUploads(window.expiredUploadBefore, window.batchLimit)) {
        if (await ports.repository.uploadHasAttachment(upload.id)) continue;
        await ports.objects.delete(expiredUploadObjectKeys(holdingId, upload));
        await ports.repository.expireUpload(upload.id, window.expiredUploadBefore, now);
      }
      await ports.repository.expireAiReservations(window.pendingDraftBefore, now);
    },
  };
}
