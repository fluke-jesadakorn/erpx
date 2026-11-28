export * from "@erpx/organization-ui/lib/organization-draft-domain";
import {emptyDraft as emptyDraftAt,type DraftDefinition} from "@erpx/organization-ui/lib/organization-draft-domain";
/** UI and compatibility clock; service applications pass their injected time. */
export function emptyDraft():DraftDefinition{return emptyDraftAt(Date.now());}
