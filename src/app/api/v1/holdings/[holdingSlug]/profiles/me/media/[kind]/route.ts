import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import { activeHoldingMember } from "@/server/member-profiles";
import { assertion, executeCommand, replayCommand } from "@/server/commands";
import { readBoundedUploadBody } from "@/server/attachments/upload";
import { detectedAttachmentMediaType, sanitizedAttachmentPreview } from "@/server/attachments/preview";
import { scanAttachmentBytes } from "@/server/attachments/scanner";
import { attachmentsV2Enabled } from "@/server/attachments/feature";
import { sha256 } from "@/server/crypto";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/me/media/[kind]">) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, kind } = await context.params;
    if (kind !== "avatar" && kind !== "cover") throw new ApiError(404,"PROFILE_MEDIA_NOT_FOUND","Unknown profile image.");
    const actor = await requireWorkContext(holdingSlug);
    const write = await activeHoldingMember(actor,actor.userId);
    const { files } = await holdingResources(actor.holdingId);
    const mediaType = request.headers.get("content-type")?.split(";",1)[0]?.trim().toLowerCase() ?? "";
    if (!["image/jpeg","image/png","image/webp"].includes(mediaType)) throw new ApiError(422,"PROFILE_IMAGE_TYPE","Choose a JPEG, PNG or WebP image.");
    const max = kind === "avatar" ? 2 * 1024 * 1024 : 5 * 1024 * 1024;
    const size = Number(request.headers.get("content-length"));
    if (!Number.isSafeInteger(size) || size < 1 || size > max) throw new ApiError(422,"PROFILE_IMAGE_SIZE",`${kind === "avatar" ? "Avatar" : "Cover"} must be ${kind === "avatar" ? "2" : "5"} MB or smaller.`);
    const expectedVersion = Number(request.headers.get("x-profile-version"));
    if (!Number.isSafeInteger(expectedVersion) || expectedVersion < 0) throw new ApiError(422,"PROFILE_VERSION_REQUIRED","Refresh your profile before uploading.");
    const commandId = request.headers.get("Idempotency-Key") ?? "";
    if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(commandId)) throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");
    const buffer = await readBoundedUploadBody(request,size);
    const bytes = new Uint8Array(buffer);
    if (detectedAttachmentMediaType(bytes,mediaType) !== mediaType) throw new ApiError(422,"PROFILE_IMAGE_TYPE","Image contents do not match its type.");
    const digest = await sha256(buffer);
    const input = { kind, expectedVersion, digest };
    const prior = await replayCommand<{version:number}>(write,actor,commandId,"member.profile.media",actor.userId,input);
    if (prior) return Response.json({ data: prior }, { headers: { "Cache-Control": "private, no-store" } });
    let scanned;
    try { scanned = await scanAttachmentBytes(bytes, attachmentsV2Enabled() ? process.env : { ...process.env, ERPX_ATTACHMENTS_V2: "true" }); }
    catch { throw new ApiError(503,"PROFILE_SCANNER_UNAVAILABLE","Image scanning is temporarily unavailable."); }
    if (!scanned.clean) throw new ApiError(422,"PROFILE_IMAGE_REJECTED","This image did not pass security scanning.");
    const preview = await sanitizedAttachmentPreview(bytes,mediaType);
    if (!preview) throw new ApiError(422,"PROFILE_IMAGE_TYPE","Unable to process this image.");
    const key = `${actor.holdingId}/profiles/${actor.userId}/${kind}/${crypto.randomUUID()}.webp`;
    const stored = await files.put(key,preview.bytes,{
      onlyIf:{etagDoesNotMatch:"*"},
      httpMetadata:{contentType:preview.mediaType,cacheControl:"private, no-store"},
      customMetadata:{sanitized:"true",sourceSha256:digest},
    });
    if (!stored) throw new ApiError(503,"PROFILE_IMAGE_STORAGE","Unable to store the image.");
    const nextVersion = expectedVersion + 1, now = Date.now();
    try {
      const result = await executeCommand(write,actor,commandId,"member.profile.media",input,actor.userId,nextVersion,{version:nextVersion},[
        assertion(write,commandId,"profile-version",expectedVersion===0
          ? "NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)"
          : "EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?)",
          expectedVersion===0?[actor.userId]:[actor.userId,expectedVersion]),
        expectedVersion===0
          ? write.prepare(`INSERT INTO member_profiles(user_id,${kind}_key,${kind}_media_type,version,updated_at) VALUES(?,?,?,?,?)`)
            .bind(actor.userId,key,preview.mediaType,nextVersion,now)
          : write.prepare(`UPDATE member_profiles SET ${kind}_key=?,${kind}_media_type=?,version=?,updated_at=? WHERE user_id=? AND version=?`)
            .bind(key,preview.mediaType,nextVersion,now,actor.userId,expectedVersion),
      ]);
      return Response.json({ data: result }, { headers: { "Cache-Control": "private, no-store" } });
    } catch (error) { await files.delete(key).catch(() => undefined); throw error; }
  } catch (error) { return errorResponse(error); }
}

export async function DELETE(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/me/media/[kind]">) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, kind } = await context.params;
    if (kind !== "avatar" && kind !== "cover") throw new ApiError(404,"PROFILE_MEDIA_NOT_FOUND","Unknown profile image.");
    const actor = await requireWorkContext(holdingSlug);
    const write = await activeHoldingMember(actor,actor.userId);
    const version = Number(request.headers.get("x-profile-version"));
    if (!Number.isSafeInteger(version) || version < 1) throw new ApiError(422,"PROFILE_VERSION_REQUIRED","Refresh your profile before removing an image.");
    const commandId = request.headers.get("Idempotency-Key") ?? "";
    if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(commandId)) throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");
    const input = { kind, version };
    const prior = await replayCommand<{version:number}>(write,actor,commandId,"member.profile.media.remove",actor.userId,input);
    if (prior) return Response.json({ data: prior }, { headers: { "Cache-Control": "private, no-store" } });
    const row = await write.prepare(`SELECT ${kind}_key AS key FROM member_profiles WHERE user_id=? AND version=?`)
      .bind(actor.userId,version).first<{key:string|null}>();
    if (!row?.key) throw new ApiError(404,"PROFILE_MEDIA_NOT_FOUND","Profile image unavailable.");
    const nextVersion = version + 1;
    const result = await executeCommand(write,actor,commandId,"member.profile.media.remove",input,actor.userId,nextVersion,{version:nextVersion},[
      assertion(write,commandId,"profile-version",`EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=? AND ${kind}_key=?)`,[actor.userId,version,row.key]),
      write.prepare(`UPDATE member_profiles SET ${kind}_key=NULL,${kind}_media_type=NULL,version=?,updated_at=? WHERE user_id=? AND version=?`)
        .bind(nextVersion,Date.now(),actor.userId,version),
    ]);
    const { files } = await holdingResources(actor.holdingId);
    await files.delete(row.key).catch(() => undefined);
    return Response.json({ data: result }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
