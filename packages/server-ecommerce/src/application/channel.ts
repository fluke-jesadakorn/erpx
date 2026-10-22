import { channelSchema, type ChannelCommand } from "@erpx/server-ecommerce/domain/channel";
import type { Clock } from "@erpx/server-shared/application/ports";

export type ChannelRepository = {
  replay(command: ChannelCommand): Promise<Record<string, unknown> | null>;
  create(command: ChannelCommand, createdAt: number): Promise<Record<string, unknown>>;
};
export function createChannelApplication(ports: { authorize(): void; repository(): Promise<ChannelRepository>; clock: Clock }) {
  return async (raw: unknown, key: string) => {
    ports.authorize();
    const command: ChannelCommand = { input: channelSchema.parse(raw), key, id: `ecommerce-channel-${key}`, operation: "ecommerce.channel.create" };
    const repository = await ports.repository(), prior = await repository.replay(command);
    if (prior) return prior;
    return repository.create(command, ports.clock.now());
  };
}
