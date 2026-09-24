import { requireManageErpxContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { activeHoldingBindings, holdingResources, runtimeEnv } from "@/server/runtime";
import { summarizePlatformOperations } from "@/server/platform-operations";

export async function GET() {
  try {
    const { platform: actor } = await requireManageErpxContext();
    const env = await runtimeEnv();
    const bindings = (await activeHoldingBindings()).filter((binding) => binding.id !== "h003");
    const summary = await summarizePlatformOperations(
      actor,
      env.CONTROL_DB,
      await Promise.all(
        bindings.map(async (binding) => ({
          id: binding.id,
          slug: binding.slug,
          displayName: binding.displayName,
          db: (await holdingResources(binding.id)).write,
        })),
      ),
    );
    return Response.json({ data: summary }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
