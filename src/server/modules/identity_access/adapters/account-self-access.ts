import 'server-only';
import {cache} from 'react';
import {currentSession} from './authorization';
import {ownIamAccess} from './iam-my-access';

/** Account inspection is bound to the authenticated account, never a selected
 * Company role, a caller-supplied user, or a purchased People capability. */
export const currentAccountSelfAccess=cache(async()=>{
 const {session}=await currentSession();
 return ownIamAccess(session.user.id);
});
