import { randomBytes } from "node:crypto";
import { createBeginMfa, createVerifyMfa, type MfaActor } from "@erpx/server-identity_access/application/mfa";
import { encodeBase32 } from "@erpx/server-identity_access/domain/mfa";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { iamThrottle } from "@erpx/server-identity_access/public/iam-throttle";
import { openSecret, sealSecret, totp } from "@erpx/server-identity_access/public/iam-mfa";

/**
 * Fixture/test oracle for authenticator enrollment and verification.
 *
 * The runtime owner port for these writes is native
 * (`/api/v1/account/iam/mfa/enroll|verify` -> `account_mfa_postgres`). Fixtures
 * and integration tests seed a local disposable database without an HTTP
 * listener, so they drive the same retained application logic against an
 * explicit Control database instead of keeping TypeScript SQL writers.
 */
export function iamMfaEnrollment(database: SqlDatabase) {
  const error = (status: number, code: string, message: string) => new ApiError(status, code, message);
  return {
    begin(actor: MfaActor) {
      return createBeginMfa({
        throttle: current => iamThrottle("mfa-enroll", current.userId, 3),
        enabled: async current =>
          Boolean(
            (
              await database
                .prepare("SELECT enabled FROM iam_mfa WHERE user_id=?")
                .bind(current.userId)
                .first<{ enabled: boolean }>()
            )?.enabled,
          ),
        sessionCreated: async current =>
          (
            await database
              .prepare('SELECT "createdAt" FROM session WHERE id=? AND "userId"=?')
              .bind(current.sessionId, current.userId)
              .first<{ createdAt: string }>()
          )?.createdAt ?? null,
        now: Date.now,
        error,
        secret: () => encodeBase32(randomBytes(20)),
        seal: sealSecret,
        enroll: async (current, sealed) => {
          await database
            .prepare(
              "INSERT INTO iam_mfa(user_id,secret) VALUES(?,?) ON CONFLICT(user_id) DO UPDATE SET secret=excluded.secret WHERE iam_mfa.enabled=false",
            )
            .bind(current.userId, sealed)
            .run();
        },
      })(actor);
    },
    verify(actor: MfaActor, code: string) {
      return createVerifyMfa({
        throttle: current => iamThrottle("mfa", current.userId),
        enrollment: async current =>
          await database
            .prepare("SELECT secret,last_counter FROM iam_mfa WHERE user_id=?")
            .bind(current.userId)
            .first<{ secret: string; last_counter: number }>(),
        now: Date.now,
        error,
        open: openSecret,
        matches: (secret, counter, candidate) => totp(secret, counter) === candidate,
        activate: async (current, counter) =>
          Boolean(
            await database
              .prepare("UPDATE iam_mfa SET enabled=true,last_counter=? WHERE user_id=? AND last_counter<? RETURNING user_id")
              .bind(counter, current.userId, counter)
              .first(),
          ),
        verified: async (current, now) => {
          await database
            .prepare(
              "INSERT INTO iam_mfa_sessions VALUES(?,?,?) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at",
            )
            .bind(current.sessionId, current.userId, now)
            .run();
        },
      })(actor, code);
    },
  };
}
