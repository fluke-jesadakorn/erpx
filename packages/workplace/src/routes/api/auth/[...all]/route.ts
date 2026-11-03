import {nativeAccountHttp} from '@erpx/server-identity_access/public/native-account-http';
export const dynamic='force-dynamic';
// Rust owns endpoint admission, sessions, cookies, redirects and all writes.
export const GET=nativeAccountHttp;
export const POST=nativeAccountHttp;
