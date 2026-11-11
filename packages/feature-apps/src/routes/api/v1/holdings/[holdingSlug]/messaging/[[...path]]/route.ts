import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import {channelSchema,createMessagingChannel,createMessagingRoom,createMessagingTeam,manageMessaging,manageSchema,messageSchema,messagingFile,messagingHistory,messagingMembers,messagingOverview,messagingPeople,readMessaging,roomSchema,sendMessagingMessage,teamSchema} from '@erpx/server-collaboration/public/messaging';
import { uploadMessagingFile,discardMessagingFile } from "@erpx/server-collaboration/public/messaging-files";
import { nativeCreateChannel, nativeCreateRoom, nativeCreateTeam, nativeMarkRead, nativeMessagingAvailable, nativeMessagingMembers, nativeMessagingOverview, nativeSendMessage } from "@erpx/server-collaboration/public/native-messaging-page";
import { attachmentContent } from "@erpx/server-files/public/attachments/service";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const cursor = z.coerce
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER);
async function handle(request: Request, context: Context) {
  try {
    if (request.method !== "GET") {
      const origin = request.headers.get("origin");
      if (origin && origin !== requestPublicOrigin(request))
        throw new ApiError(
          403,
          "ORIGIN_FORBIDDEN",
          "Use the application origin.",
        );
    }
    const { holdingSlug, path: p = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug);
    const { write: db, files } = await holdingResources(actor.holdingId),
      url = new URL(request.url);
    let data: unknown;
    if (request.method === "GET") {
      if (!p.length)
        data = nativeMessagingAvailable()
          ? await nativeMessagingOverview(request, holdingSlug)
          : await messagingOverview(db, actor);
      else if (p[0] === "people" && p.length === 1)
        data = await messagingPeople(
          db,
          actor,
          (url.searchParams.get("q") ?? "").slice(0, 160),
          url.searchParams.get("team") ?? undefined,
        );
      else if (p[0] === "rooms" && p[2] === "messages" && p.length === 3) {
        const before = url.searchParams.get("before"),
          after = url.searchParams.get("after");
        if (before !== null && after !== null)
          throw new ApiError(
            422,
            "CURSOR_INVALID",
            "Choose one message cursor.",
          );
        data = await messagingHistory(
          db,
          actor,
          p[1],
          before === null ? undefined : cursor.parse(before),
          after === null ? undefined : cursor.parse(after),
        );
      } else if (
        ["rooms", "teams"].includes(p[0]) &&
        p[2] === "members" &&
        p.length === 3
      ) {
        if (nativeMessagingAvailable()) {
          const reply = (await nativeMessagingMembers(
            request,
            holdingSlug,
            p[1],
            p[0] === "teams",
          )) as { members?: unknown };
          data = reply.members ?? [];
        } else {
          data = await messagingMembers(db, actor, p[1], p[0] === "teams");
        }
      } else if (p[0] === "files" && p.length === 2) {
        const file = await messagingFile(db, actor, p[1]);
        if (!file.attachment_link_id)
          throw new ApiError(
            404,
            "FILE_NOT_FOUND",
            "This message file has not been migrated to the current attachment lifecycle.",
          );
        const variant =
          url.searchParams.get("variant") === "preview"
            ? "preview"
            : "original";
        const content = await attachmentContent(
          db,
          files,
          actor,
          file.attachment_link_id,
          variant,
          {compatibilityAdapter:true},
        );
        return new Response(content.object.body, {
          headers: {
            "Content-Type": content.mediaType,
            "Cache-Control": "private, no-store",
            "X-Content-Type-Options": "nosniff",
            "Content-Security-Policy": "default-src 'none'; sandbox",
            "Content-Disposition": `${variant === "preview" ? "inline" : "attachment"}; filename*=UTF-8''${encodeURIComponent(content.filename)}`,
          },
        });
      } else
        throw new ApiError(404, "NOT_FOUND", "Messaging endpoint not found.");
    } else if (
      request.method === "PUT" &&
      p[0] === "rooms" &&
      p[2] === "files" &&
      p.length === 4
    ) {
      data = await uploadMessagingFile(
        db,
        files,
        actor,
        p[1],
        {
          id: p[3],
          name: decodeURIComponent(request.headers.get("x-file-name") ?? ""),
          type: request.headers.get("content-type") as "image/jpeg",
        },
        request,
      );
    } else if (request.method === "POST") {
      const input = await request.json(),
        key = request.headers.get("idempotency-key") ?? "";
      if(p[0]==='files'&&p[2]==='discard'&&p.length===3)data=await discardMessagingFile(db,actor,p[1],z.object({reason:z.string().trim().min(3).max(1000)}).parse(input),key);
      else if (p[0] === "rooms" && p.length === 1) {
        const room = roomSchema.parse(input);
        data = nativeMessagingAvailable()
          ? await nativeCreateRoom(request, holdingSlug, room, key)
          : await createMessagingRoom(db, actor, room, key);
      } else if (p[0] === "teams" && p.length === 1) {
        const team = teamSchema.parse(input);
        data = nativeMessagingAvailable()
          ? await nativeCreateTeam(request, holdingSlug, team, key)
          : await createMessagingTeam(db, actor, team, key);
      } else if (p[0] === "teams" && p[2] === "channels" && p.length === 3) {
        const channel = channelSchema.parse(input);
        data = nativeMessagingAvailable()
          ? await nativeCreateChannel(request, holdingSlug, p[1], channel, key)
          : await createMessagingChannel(db, actor, p[1], channel, key);
      }
      else if (["rooms", "teams"].includes(p[0]) && p.length === 2)
        data = await manageMessaging(
          db,
          actor,
          p[1],
          p[0] === "teams",
          manageSchema.parse(input),
          key,
        );
      else if (p[0] === "rooms" && p[2] === "messages" && p.length === 3) {
        const message = messageSchema.parse(input);
        data =
          nativeMessagingAvailable() && message.files.length === 0
            ? await nativeSendMessage(
                request,
                holdingSlug,
                p[1],
                { body: message.body },
                key,
              )
            : await sendMessagingMessage(db, actor, p[1], message, key);
      } else if (p[0] === "rooms" && p[2] === "read" && p.length === 3) {
        const seq = z.object({ seq: cursor }).parse(input).seq;
        data = nativeMessagingAvailable()
          ? await nativeMarkRead(request, holdingSlug, p[1], seq, key)
          : await readMessaging(db, actor, p[1], seq);
      }
      else
        throw new ApiError(404, "NOT_FOUND", "Messaging endpoint not found.");
    } else throw new ApiError(405, "METHOD_NOT_ALLOWED", "Method not allowed.");
    return Response.json(
      canonicalDestinationLinks({ data }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
export { handle as GET, handle as POST, handle as PUT };
