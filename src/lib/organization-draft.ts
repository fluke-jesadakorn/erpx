export * from "./organization-draft-domain";
import {emptyDraft as emptyDraftAt,type DraftDefinition} from "./organization-draft-domain";
/** UI and compatibility clock; service applications pass their injected time. */
export function emptyDraft():DraftDefinition{return emptyDraftAt(Date.now());}
