'use client';

/** Explicit Identity client API for Account shells. No server implementation,
 * sealed proof, session credential, or Company authorization crosses here. */
export {createAccountSecurityHttpPort} from '../adapters/http/account-security';
export type {AccountSecurityPort} from '../application/account-security-port';

export {createAccountDirectoryHttpPort} from "../adapters/http/account-directory";
export type {AccountDirectoryPort} from "../adapters/http/account-directory";
