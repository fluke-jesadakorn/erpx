import { betaEmailBody, verifyBetaEmailReceipt, type BetaEmail, type BetaEmailReceipt } from '../domain/email';
import type { Clock, IdentifierSource } from '../../shared';

export interface BetaEmailPorts {
  configuration: { sandbox: boolean; production: boolean; repositoryAvailable: boolean; providerConfigured: boolean };
  clock: Clock;
  identifiers: IdentifierSource;
  repository: {
    asyncReserve(input: {
      id: string;
      deliveryKey: string;
      to: string;
      subject: string;
      body: string;
      sensitive: boolean;
      providerId: string;
      now: number;
    }): Promise<void>;
    receipt(key: string): Promise<BetaEmailReceipt | null>;
  };
  provider: { send(input: { to: string; subject: string; body: string; key?: string }): Promise<string | null> };
}

export function createBetaEmailApplication(ports: BetaEmailPorts) {
  return {
    async send(email: BetaEmail) {
      const body = betaEmailBody(email.subject, email.url);
      if (ports.configuration.sandbox) {
        if (ports.configuration.production) throw new Error('EMAIL_SANDBOX_FORBIDDEN_IN_PRODUCTION');
        if (!ports.configuration.repositoryAvailable) throw new Error('EMAIL_SANDBOX_NOT_CONFIGURED');
        const deliveryKey = email.key ?? `sandbox-email-${ports.identifiers.next()}`;
        const id = `sandbox-email-${ports.identifiers.next()}`;
        const providerId = `sandbox-${deliveryKey}`;
        await ports.repository.asyncReserve({
          id,
          deliveryKey,
          to: email.to,
          subject: email.subject,
          body,
          sensitive: email.sensitive === true,
          providerId,
          now: ports.clock.now(),
        });
        return verifyBetaEmailReceipt(await ports.repository.receipt(deliveryKey), email, body, providerId);
      }
      if (!ports.configuration.providerConfigured) throw new Error('EMAIL_NOT_CONFIGURED');
      return ports.provider.send({ to: email.to, subject: email.subject, body, key: email.key });
    },
  };
}
