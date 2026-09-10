import { requestPublicOrigin } from "@/server/public-origin";
import { createAuth } from "@/server/auth";

export const dynamic = "force-dynamic";

export async function GET(request: Request) {
  return (await createAuth(requestPublicOrigin(request))).handler(request);
}

export async function POST(request: Request) {
  return (await createAuth(requestPublicOrigin(request))).handler(request);
}
