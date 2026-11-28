import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {createHandbookService} from '@erpx/collaboration-server/application/handbook';
import {nativeHandbookReads} from '@erpx/collaboration-ui/lib/native-handbook-page';

export {handbookSchema,handbookActionSchema,handbookEditorSchema} from "@erpx/collaboration-server/domain/handbook";

/**
 * The native handbook HTTP service is the sole read authority. The retained
 * TypeScript repository/page SQL that bound the owner service here is removed;
 * these compatibility entry points delegate to that service so retained
 * presentation callers keep their exact payload without a second SQL path.
 */
type Reads = ReturnType<typeof createHandbookService>;
type Bound<K extends keyof Reads> = (
  db: SqlDatabase,
  ...args: Parameters<Reads[K]>
) => ReturnType<Reads[K]>;

export const handbookAccess: Bound<'handbookAccess'> = async (_db, actor) =>
  (await nativeHandbookReads(actor, actor.holdingSlug)).handbookAccess(actor);

export const handbookEditorOptions: Bound<'handbookEditorOptions'> = async (
  _db,
  actor,
) => (await nativeHandbookReads(actor, actor.holdingSlug)).handbookEditorOptions(actor);

export const handbookList: Bound<'handbookList'> = async (
  _db,
  actor,
  query,
  editing,
  cursor,
) => {
  const read = await (
    await nativeHandbookReads(actor, actor.holdingSlug)
  ).handbookList(actor, query, editing, cursor);
  // The retired service exposed both `items` and `articles`; the native wire
  // names only `articles`. Keep the legacy alias for retained callers.
  return {...read, items: read.articles};
};

export const handbookArticle: Bound<'handbookArticle'> = async (
  _db,
  actor,
  id,
  editing,
  revision,
) =>
  (await nativeHandbookReads(actor, actor.holdingSlug)).handbookArticle(
    actor,
    id,
    editing,
    revision,
  );
