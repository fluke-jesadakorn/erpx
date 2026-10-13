/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { encodeBase32, totp, sealSecret, openSecret, requireFreshMfa, beginMfa, verifyMfa } from "@/server/modules/identity_access/adapters/iam-mfa";
