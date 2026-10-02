import { expect, type Page } from "@playwright/test";
import type { OrganizationAccessData } from "../../src/lib/organization-access";
import { changedPeople, emptyDraft, organizationDraftDiff, personPlacement, projectOrganization, type DraftDefinition, type OrganizationDraft } from "../../src/lib/organization-draft";

/** The browser exercises durable draft responses without editing a user's private draft. */
export async function mockPeopleDraft(page: Page, organization: OrganizationAccessData, definition?: DraftDefinition) {
  let draft: OrganizationDraft | null = definition ? { id: "browser-private-draft", version: 1, state: "draft", definition, updated_at: Date.now() } : null;
  let nextFailure: { code: string; message: string; status: number } | null = null;
  const writes: { action: string; body: Record<string, unknown> }[] = [];
  await page.route(/\/api\/v1\/.*\/people\/organization\/drafts(?:\/[^?]*)?(?:\?.*)?$/, async route => {
    const request = route.request();
    const suffix = new URL(request.url()).pathname.split("/people/organization/drafts")[1];
    const action = suffix.split("/").filter(Boolean).at(-1) ?? "open";
    const respond = (data: unknown, status = 200) => route.fulfill({ status, contentType: "application/json", body: JSON.stringify({ data }) });
    if (request.method() === "GET") return respond({ draft, requests: [], recent: [] });
    const body = request.postDataJSON() as Record<string, unknown>;
    writes.push({ action, body });
    if (!suffix) {
      draft ??= { id: "browser-private-draft", version: 1, state: "draft", definition: emptyDraft(), updated_at: Date.now() };
      return respond({ draft, requests: [], recent: [] });
    }
    if (nextFailure && action === "save") {
      const failure = nextFailure; nextFailure = null;
      return route.fulfill({ status: failure.status, contentType: "application/json", body: JSON.stringify({ error: failure }) });
    }
    if (action === "save") {
      expect(body.expectedVersion).toBe(draft!.version);
      draft = { ...draft!, version: draft!.version + 1, definition: body.definition as DraftDefinition, updated_at: Date.now() };
      return respond({ draft });
    }
    if (action === "preview") {
      const projected = projectOrganization(organization, draft!.definition);
      const diff = organizationDraftDiff(organization, draft!.definition);
      const affected = changedPeople(organization, projected);
      const blockers = draft!.definition.items.some(item => item.parked) ? ["Choose a destination for this item."] : [];
      return respond({ ready: !blockers.length, token: "browser-review-token", version: draft!.version, diff, blockers, staleSources: [], legacy: [],
        affectedPeople: affected.map(person => ({ id: person.engagement_id, name: person.legal_name, before: personPlacement(organization.people.find(before => before.engagement_id === person.engagement_id)!), after: personPlacement(person), direct: draft!.definition.items.some(item => item.id === person.engagement_id) })),
        leaveImpact: affected.map(person => ({ engagementId: person.engagement_id, note: "Existing leave remains." })), policyImpact: { added: 0, removed: 0, retained: 0 } });
    }
    if (action === "discard") { draft = null; return respond({}); }
    // A test must explicitly opt into any future submission behavior.
    return route.fulfill({ status: 409, contentType: "application/json", body: JSON.stringify({ error: { message: `Unexpected draft action: ${action}` } }) });
  });
  return {
    writes,
    get draft() { return draft; },
    failNextSave(message: string, code = "SAVE_FAILED", status = 503) { nextFailure = { message, code, status }; },
    replaceSaved(definition: DraftDefinition) { draft = { ...draft!, definition, version: draft!.version + 1, updated_at: Date.now() }; },
  };
}
