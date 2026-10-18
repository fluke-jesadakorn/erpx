import { providerCommercialApi } from "@/server/modules/commercial/public/provider-commercial";
export const dynamic = "force-dynamic";
async function handle(request: Request, { params }: { params: Promise<{ path?: string[] }> }) {
  return providerCommercialApi(request, (await params).path ?? []);
}
export { handle as GET, handle as POST };
