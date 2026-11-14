import type {
  AccountAuthError, AccountOperationResponse, AccountSessionResponse, DeviceAccountsDto, DeviceSelectionInput, DeviceInitializeInput, AccountInvitationAcceptInput, AccountInvitationAcceptResponse, AccountInvitationInspectResponse,
  EmailSignInInput, EmailSignInResponse, EmailSignUpInput, EmailVerificationInput, EmailVerificationRequestInput, PasswordChangeInput, PasswordResetInput, PasswordResetRequestInput,
} from "@erpx/contracts/account-auth";

export type AccountAuthResult<T> = { data: T | null; error: AccountAuthError | null };

/** Browser intent only. Authentication and current device authority remain at the HTTP boundary. */
export interface AccountAuthPort {
  signIn: { email(input: EmailSignInInput): Promise<AccountAuthResult<EmailSignInResponse>> };
  signUp: { email(input: EmailSignUpInput): Promise<AccountAuthResult<AccountOperationResponse>> };
  signOut(): Promise<AccountAuthResult<AccountOperationResponse>>;
  getSession(): Promise<AccountAuthResult<AccountSessionResponse>>;
  requestPasswordReset(input: PasswordResetRequestInput): Promise<AccountAuthResult<AccountOperationResponse>>;
  resetPassword(input: PasswordResetInput): Promise<AccountAuthResult<AccountOperationResponse>>;
  sendVerificationEmail(input: EmailVerificationRequestInput): Promise<AccountAuthResult<AccountOperationResponse>>;
  verifyEmail(input: EmailVerificationInput): Promise<AccountAuthResult<EmailSignInResponse>>;
  changePassword(input: PasswordChangeInput): Promise<AccountAuthResult<AccountOperationResponse>>;
  invitations: {
    inspect(token: string): Promise<AccountAuthResult<AccountInvitationInspectResponse>>;
    accept(input: AccountInvitationAcceptInput): Promise<AccountAuthResult<AccountInvitationAcceptResponse>>;
  };
  multiSession: {
    initialize(input: DeviceInitializeInput): Promise<AccountAuthResult<DeviceAccountsDto>>;
    listDeviceSessions(): Promise<AccountAuthResult<DeviceAccountsDto>>;
    setActive(input: DeviceSelectionInput): Promise<AccountAuthResult<DeviceAccountsDto>>;
  };
}
