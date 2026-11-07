import {nativeAccountHttp} from '@erpx/server-identity_access/public/native-account-http';
export const dynamic='force-dynamic';

/** Native Account security owns MFA freshness, session-family revocation,
 * request replay and readback. A missing or failed native runtime is explicit;
 * this route never falls back to the retired TypeScript security command. */
export {nativeAccountHttp as GET,nativeAccountHttp as POST};
