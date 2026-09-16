import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
export function betaDestinationAllowed(destination: string, api: boolean) {
  if (api && /^people\/organization(?:\/.*)?$/.test(destination)) return true;
  if (!api && destination === "people/organization") return true;
  if (api && /^messaging(?:\/.*)?$/.test(destination)) return true;
  return api
    ? /^(beta(?:\/.*)?|flows\/(?:expense|request|staff_task)\/[^/]+|requests(?:\/.*)?|service-teams(?:\/.*)?|handbook(?:\/.*)?|work\/(?:summary|card-preferences)|ai-providers(?:\/.*)?|expenses(?:\/.*)?|people\/directory(?:\/[^/]+(?:\/(?:status|grant|revoke))?)?|posts\/[^/]+(?:\/(?:task|task-source|ai-drafts))?|staff-tasks(?:\/.*)?|approvals\/[^/]+\/reroute|staff\/(?:people|settings|levels(?:\/[^/]+)?|engagements\/[^/]+\/level|departments\/[^/]+\/head|notifications(?:\/[^/]+\/read)?)|uploads(?:\/[^/]+\/content)?|attachments\/[^/]+\/(?:content|derivatives)|assignments\/active|companies\/active)$/.test(
        destination,
      )
    : /^(home|feed|work|requests(?:\/[^/]+)?|handbook(?:\/[^/]+)?|people|settings|inbox|expenses(?:\/[^/]+)?|posts\/[^/]+|staff-tasks(?:\/[^/]+)?)$/.test(
        destination,
      );
}
export async function betaAccess(
  db: SqlDatabase,
  companyId: string,
  requestHeaders: Headers,
) {
  const beta = !!(await db
    .prepare("SELECT 1 FROM beta_policy WHERE company_id=?")
    .bind(companyId)
    .first());
  const destination = requestHeaders.get("x-erpx-destination");
  if (
    beta &&
    destination &&
    !betaDestinationAllowed(
      destination,
      requestHeaders.get("x-erpx-api") === "true",
    )
  )
    throw new ApiError(
      403,
      "BETA_CAPABILITY_UNAVAILABLE",
      "This capability is not part of the public beta.",
    );
  return beta;
}
