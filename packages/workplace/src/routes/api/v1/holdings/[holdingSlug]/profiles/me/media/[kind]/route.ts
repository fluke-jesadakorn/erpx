import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError, errorResponse} from '@erpx/server-runtime/public/http';
import {readBoundedUploadBody} from '@erpx/server-files/public/attachments/upload';
import {uploadMemberProfileMedia, removeMemberProfileMedia} from '@erpx/server-organization/public/profile-media-command';
const response = (data: unknown) => Response.json(canonicalDestinationLinks({data}), {headers: {'Cache-Control': 'private, no-store'}});
export async function POST(request: Request, context: RouteContext<'/api/v1/holdings/[holdingSlug]/profiles/me/media/[kind]'>) {
  try {
    const origin = request.headers.get('origin');
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    const {holdingSlug, kind} = await context.params;
    if (kind !== 'avatar' && kind !== 'cover') throw new ApiError(404, 'PROFILE_MEDIA_NOT_FOUND', 'Unknown profile image.');
    const actor = await requireWorkContext(holdingSlug);
    return response(await uploadMemberProfileMedia(actor, {kind, contentType: request.headers.get('content-type'), contentLength: request.headers.get('content-length'), version: request.headers.get('x-profile-version'), commandId: request.headers.get('Idempotency-Key')}, size => readBoundedUploadBody(request, size)));
  } catch (error) { return errorResponse(error); }
}
export async function DELETE(request: Request, context: RouteContext<'/api/v1/holdings/[holdingSlug]/profiles/me/media/[kind]'>) {
  try {
    const origin = request.headers.get('origin');
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    const {holdingSlug, kind} = await context.params;
    if (kind !== 'avatar' && kind !== 'cover') throw new ApiError(404, 'PROFILE_MEDIA_NOT_FOUND', 'Unknown profile image.');
    const actor = await requireWorkContext(holdingSlug);
    return response(await removeMemberProfileMedia(actor, {kind, version: request.headers.get('x-profile-version'), commandId: request.headers.get('Idempotency-Key')}));
  } catch (error) { return errorResponse(error); }
}
