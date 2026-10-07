import {requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {requestPublicOrigin} from '@/server/modules/manage_erpx/public/public-origin';
import {ApiError, errorResponse} from '@/server/modules/runtime/public/http';
import {readBoundedUploadBody} from '@/server/modules/files/public/attachments/upload';
import {uploadMemberProfileMedia, removeMemberProfileMedia} from '@/server/modules/organization/public/profile-media-command';
const response = (data: unknown) => Response.json({data}, {headers: {'Cache-Control': 'private, no-store'}});
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
