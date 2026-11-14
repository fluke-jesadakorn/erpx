import "server-only";
import { nativeAccountHttp } from "@erpx/server-identity_access/public/native-account-http";
// Native Account invitation inspection and acceptance. The owner route enforces
// the Account origin, the single-use invitation proof and the reviewed or
// directly authorized activation; an unconfigured runtime fails closed with
// 503 NATIVE_ACCOUNT_CONFIGURATION_REQUIRED.
export const dynamic = "force-dynamic";
export function GET(request: Request) {
  return nativeAccountHttp(request);
}
export function POST(request: Request) {
  return nativeAccountHttp(request);
}
