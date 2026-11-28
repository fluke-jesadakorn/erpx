import type {AccountMfaEnrollment,AccountMfaVerified,AccountMfaVerifyInput,AccountSecuritySeed} from '@erpx/platform-contracts/account-auth';

/** Transport returns display data, never a reusable authorization proof. */
export interface AccountSecurityPort {
  enroll():Promise<AccountMfaEnrollment>;
  verify(input:AccountMfaVerifyInput):Promise<AccountMfaVerified>;
  read():Promise<AccountSecuritySeed>;
}
