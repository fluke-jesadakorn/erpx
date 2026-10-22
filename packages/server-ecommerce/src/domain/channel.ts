import { z } from "zod";
export const channelSchema = z.object({
  name: z.string().trim().min(2).max(160),
  channelType: z.enum(["web", "marketplace", "social"]),
  configuration: z.record(z.string(), z.unknown()),
});
export type ChannelInput = z.infer<typeof channelSchema>;
export type ChannelCommand = { key: string; id: string; operation: "ecommerce.channel.create"; input: ChannelInput };
