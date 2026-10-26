// Generated from the Rust Account HTTP contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-account-contracts.ts
export type AccountRevision = number;
export type EmailSignInInput = { email: string, password: string, revision?: AccountRevision, callbackURL?: string, rememberMe?: boolean, };
export type EmailSignUpInput = { name: string, email: string, password: string, callbackURL?: string, invitationToken?: string, };
export type EmailVerificationInput = { token: string, callbackURL?: string, revision?: AccountRevision, };
export type PasswordChangeInput = { currentPassword: string, newPassword: string, };
export type PasswordResetRequestInput = { email: string, redirectTo?: string, };
export type PasswordResetInput = { newPassword: string, token: string, };
export type EmailVerificationRequestInput = { email: string, callbackURL?: string, };
export type DeviceSelectionInput = { handle: string, revision: AccountRevision, };
export type AccountUserDto = { id: string, name: string, email: string, emailVerified: boolean, image: string | null, };
export type AccountSessionDto = { id: string, userId: string, expiresAt: string, createdAt: string, updatedAt: string, };
export type AccountSessionResponse = { session: AccountSessionDto, user: AccountUserDto, };
export type DeviceAccountDto = { handle: string, name: string, email: string, image: string | null, current: boolean, };
export type DeviceAccountsDto = { initialized: boolean, revision: AccountRevision, maximum: number, accounts: Array<DeviceAccountDto>, };
export type AccountAuthError = { code: string, message: string, };
export type AccountAuthErrorResponse = { error: AccountAuthError, };
export type EmailSignInResponse = { user: AccountUserDto, };
export type AccountOperationResponse = { status: boolean, };
