/** Compatibility entry point; legacy implementation belongs to identity_access. */
export {beginMfa,encodeBase32,openSecret,requireFreshMfa,sealSecret,totp,verifyMfa} from "@/server/modules/identity_access/adapters/iam-mfa";
