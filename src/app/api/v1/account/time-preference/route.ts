import {sessionFromRequest} from '@/server/modules/identity_access/public/auth';
import {accountTimePreference, changeAccountTimePreference} from '@/server/modules/identity_access/public/account-time-preference';
import {ApiError, errorResponse} from '@/server/modules/runtime/public/http';
import {requestPublicOrigin} from '@/server/modules/manage_erpx/public/public-origin';
async function userId(request: Request) {
  const session = await sessionFromRequest(request);
  if (!session) throw new ApiError(401, 'SIGN_IN_REQUIRED', 'Sign in to edit your time settings.');
  return session.user.id;
}
const response = (data: unknown) => Response.json({data}, {headers: {'Cache-Control': 'private, no-store'}});
export async function GET(request: Request) {
  try { return response(await accountTimePreference(await userId(request))); } catch (error) { return errorResponse(error); }
}
export async function PATCH(request: Request) {
  try {
    const origin = request.headers.get('origin');
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    const id = await userId(request);
    return response(await changeAccountTimePreference(id, await request.json()));
  } catch (error) { return errorResponse(error); }
}
