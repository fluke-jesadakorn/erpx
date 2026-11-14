import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeMarketingCampaignReadInput,
  NativeMarketingCampaignReport,
} from '../../public/native-marketing-campaign';

export function createNextNativeMarketingCampaignReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMarketingCampaignReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeMarketingCampaignReport>,
) {
  return async (input: NativeMarketingCampaignReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
