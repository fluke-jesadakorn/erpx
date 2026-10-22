import { describe, expect, it, vi } from "vitest";
import { createAccountRecoveryApplication, type AccountRecoveryPorts } from "@erpx/server-identity_access/application/account-recovery";

function fixture() {
  const job = { id: "recovery-job", user_id: "user-1", url_ciphertext: "encrypted", attempts: 1, expires_at: 100001 };
  const ports: AccountRecoveryPorts = {
    clock: { now: () => 100000 }, identifiers: { next: () => "lease-or-event" }, productOrigins: { pms: "https://pms.example.test", pos: "https://pos.example.test" },
    cryptography: { recoveryId: vi.fn(() => "hashed-token"), sealLink: vi.fn(() => "encrypted"), openLink: vi.fn(() => "https://erpx.example.test/reset?token=secret") },
    send: vi.fn(async () => "provider-receipt"),
    repository: { queue: vi.fn(async () => {}), recoverInterrupted: vi.fn(async () => {}), claim: vi.fn(async () => job), account: vi.fn(async () => ({ email: "member@example.test" })), cancel: vi.fn(async () => {}), sent: vi.fn(async () => {}), failed: vi.fn(async () => {}), complete: vi.fn(async () => {}) },
  };
  return { job, ports, application: createAccountRecoveryApplication(ports) };
}

describe("Identity Account Recovery application", () => {
  it("queues encrypted links and a one-hour expiry through one atomic repository operation", async () => {
    const f = fixture();
    await f.application.queue("user-1", "https://erpx.example.test/reset?token=secret", "secret");
    expect(f.ports.cryptography.recoveryId).toHaveBeenCalledWith("secret");
    expect(f.ports.cryptography.sealLink).toHaveBeenCalledWith("https://erpx.example.test/reset?token=secret");
    expect(f.ports.repository.queue).toHaveBeenCalledExactlyOnceWith({ id: "hashed-token", userId: "user-1", verificationIdentifier: "reset-password:secret", urlCiphertext: "encrypted", now: 100000, expiresAt: 3700000, eventId: "lease-or-event" });
    expect(f.ports.send).not.toHaveBeenCalled();
  });

  it("cancels at the exact expiry before decrypting or sending a password reset link", async () => {
    const f = fixture(); f.job.expires_at = 100000;
    expect(await f.application.deliver()).toBe(true);
    expect(f.ports.repository.cancel).toHaveBeenCalledWith(f.job, "lease-or-event", "RECOVERY_EXPIRED", 100000, "lease-or-event");
    expect(f.ports.cryptography.openLink).not.toHaveBeenCalled(); expect(f.ports.send).not.toHaveBeenCalled();
  });

  it("cancels deleted accounts and distinguishes a queue with no claim", async () => {
    const f = fixture(); f.ports.repository.account = vi.fn(async () => null);
    await f.application.deliver();
    expect(f.ports.repository.cancel).toHaveBeenCalledWith(f.job, "lease-or-event", "ACCOUNT_UNAVAILABLE", 100000, "lease-or-event");
    expect(f.ports.send).not.toHaveBeenCalled();
    f.ports.repository.claim = vi.fn(async () => null);
    expect(await f.application.deliver()).toBe(false);
    expect(f.ports.repository.account).toHaveBeenCalledTimes(1);
  });

  it("keeps the durable job id as provider idempotency key and records its claimed lease", async () => {
    const f = fixture();
    f.ports.cryptography.openLink = vi.fn(() => "https://pms.example.test/reset?token=secret");
    await f.application.deliver();
    expect(f.ports.send).toHaveBeenCalledWith("member@example.test", "Reset your PMS Front password", "https://pms.example.test/reset?token=secret", "recovery-job");
    expect(f.ports.repository.sent).toHaveBeenCalledWith(f.job, "lease-or-event", "provider-receipt", 100000, "lease-or-event");
  });

  it("retains a null receipt when a deterministic provider has no provider identifier", async () => {
    const f = fixture(); f.ports.send = vi.fn(async () => {});
    await f.application.deliver();
    expect(f.ports.repository.sent).toHaveBeenCalledWith(f.job, "lease-or-event", null, 100000, "lease-or-event");
  });

  it("requeues transient failures and marks the sixth attempt terminal without leaking provider errors", async () => {
    const f = fixture(); f.ports.send = vi.fn(async () => { throw new Error("private provider credential"); });
    await f.application.deliver();
    expect(f.ports.repository.failed).toHaveBeenCalledWith(f.job, "lease-or-event", { state: "queued", terminal: false, code: "EMAIL_DELIVERY_FAILED", nextAttemptAt: 160000 }, 100000, "lease-or-event");
    f.job.attempts = 6; f.ports.send = vi.fn(async () => { throw new Error("EMAIL_NOT_CONFIGURED"); });
    await f.application.deliver();
    expect(f.ports.repository.failed).toHaveBeenLastCalledWith(f.job, "lease-or-event", { state: "failed", terminal: true, code: "EMAIL_NOT_CONFIGURED", nextAttemptAt: 2020000 }, 100000, "lease-or-event");
  });

  it("revokes recovery state and sessions through one completion repository operation", async () => {
    const f = fixture(); await f.application.complete("user-1");
    expect(f.ports.repository.complete).toHaveBeenCalledExactlyOnceWith("user-1", 100000, "lease-or-event");
    expect(f.ports.send).not.toHaveBeenCalled(); expect(f.ports.cryptography.openLink).not.toHaveBeenCalled();
  });
});
