import { productSsoRequest } from "@erpx/identity-access-server/public/product-sso";
export const dynamic = "force-dynamic";
export async function GET(request: Request, { params }: { params: Promise<{ action: string }> }) {
  return productSsoRequest(request, (await params).action);
}
