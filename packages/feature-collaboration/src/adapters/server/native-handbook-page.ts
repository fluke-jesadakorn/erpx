import 'server-only';
import {headers} from 'next/headers';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {HandbookListRow, HandbookRevision} from '@erpx/server-collaboration/public/handbook';
import type {HandbookPurchaseControls} from '@erpx/server-collaboration/public/handbook-purchases';
import {collaborationPresentationQueries} from '@erpx/server-collaboration/public/presentation-queries';
import {createNativeHandbookClient} from './native-handbook';

export type HandbookAccess = {
  owner: boolean;
  editor: boolean;
  purchase?: HandbookPurchaseControls;
};

export type HandbookListRead = {
  access: HandbookAccess;
  articles: HandbookListRow[];
  previousCursor: string | null;
  nextCursor: string | null;
  hasMore: boolean;
};

export type HandbookArticleRead = {
  access: HandbookAccess;
  article: {
    id: string;
    version: number | null;
    draft_revision?: number;
    published_revision: number | null;
    archived: number;
  };
  revision: HandbookRevision;
  history: Omit<HandbookRevision, 'body'>[];
};

export type HandbookEditorOptionsRead = {
  version: number;
  editors: {user_id: string; granted_at: number}[];
};

export type HandbookReads = {
  handbookAccess(actor: WorkContext): Promise<HandbookAccess>;
  handbookList(
    actor: WorkContext,
    query?: string,
    editing?: boolean,
    cursor?: string,
  ): Promise<HandbookListRead>;
  handbookArticle(
    actor: WorkContext,
    id: string,
    editing?: boolean,
    revision?: number,
  ): Promise<HandbookArticleRead>;
  handbookEditorOptions(actor: WorkContext): Promise<HandbookEditorOptionsRead>;
};

type NativeClient = ReturnType<typeof createNativeHandbookClient>;

/** Selector only; an absent configuration keeps the retained TypeScript read. */
export function nativeHandbookAvailable(): boolean {
  return process.env.ERPX_NATIVE_HANDBOOK_CONFIG !== undefined;
}

const invalid = (): never => {
  throw new ApiError(
    503,
    'NATIVE_HANDBOOK_RESPONSE_INVALID',
    'The native handbook reader returned an invalid response.',
  );
};

const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string';
const nonEmpty = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0;
const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const nullableText = (value: unknown): value is string | null =>
  value === null || text(value);
const nullableInteger = (value: unknown): value is number | null =>
  value === null || integer(value);

function access(value: unknown): HandbookAccess {
  if (
    !object(value) ||
    typeof value.owner !== 'boolean' ||
    typeof value.editor !== 'boolean'
  )
    return invalid();
  if (value.purchase === undefined)
    return {owner: value.owner, editor: value.editor};
  const purchase = value.purchase;
  if (
    !object(purchase) ||
    typeof purchase.newWork !== 'boolean' ||
    typeof purchase.settleExisting !== 'boolean' ||
    typeof purchase.history !== 'boolean' ||
    !text(purchase.revision)
  )
    return invalid();
  return {
    owner: value.owner,
    editor: value.editor,
    purchase: {
      newWork: purchase.newWork,
      settleExisting: purchase.settleExisting,
      history: purchase.history,
      revision: purchase.revision,
    },
  };
}

function listRow(value: unknown): HandbookListRow {
  if (
    !object(value) ||
    !nonEmpty(value.id) ||
    !text(value.page_key) ||
    !integer(value.version) ||
    !integer(value.archived) ||
    !integer(value.revision) ||
    !text(value.title) ||
    !text(value.category) ||
    !text(value.excerpt) ||
    !nullableInteger(value.published_at)
  )
    return invalid();
  return {
    id: value.id,
    page_key: value.page_key,
    version: value.version,
    archived: value.archived,
    revision: value.revision,
    title: value.title,
    category: value.category,
    excerpt: value.excerpt,
    published_at: value.published_at,
  };
}

function list(value: unknown): HandbookListRead {
  if (
    !object(value) ||
    !Array.isArray(value.articles) ||
    !nullableText(value.previousCursor) ||
    !nullableText(value.nextCursor) ||
    typeof value.hasMore !== 'boolean'
  )
    return invalid();
  return {
    access: access(value.access),
    articles: value.articles.map(listRow),
    previousCursor: value.previousCursor,
    nextCursor: value.nextCursor,
    hasMore: value.hasMore,
  };
}

function revision(value: unknown, body: boolean): HandbookRevision | Omit<HandbookRevision, 'body'> {
  if (
    !object(value) ||
    !integer(value.revision) ||
    !text(value.title) ||
    !text(value.category) ||
    (body && !text(value.body)) ||
    !text(value.created_by) ||
    !integer(value.created_at) ||
    !nullableText(value.published_by) ||
    !nullableInteger(value.published_at)
  )
    return invalid();
  const row = {
    revision: value.revision,
    title: value.title,
    category: value.category,
    created_by: value.created_by,
    created_at: value.created_at,
    published_by: value.published_by,
    published_at: value.published_at,
  };
  return body ? {...row, body: value.body as string} : row;
}

function article(value: unknown): HandbookArticleRead {
  if (
    !object(value) ||
    !object(value.article) ||
    !nonEmpty(value.article.id) ||
    !integer(value.article.version) ||
    (value.article.draft_revision !== undefined &&
      !integer(value.article.draft_revision)) ||
    !nullableInteger(value.article.published_revision) ||
    !integer(value.article.archived) ||
    !Array.isArray(value.history)
  )
    return invalid();
  return {
    access: access(value.access),
    article: {
      id: value.article.id,
      version: value.article.version,
      ...(value.article.draft_revision !== undefined
        ? {draft_revision: value.article.draft_revision}
        : {}),
      published_revision: value.article.published_revision,
      archived: value.article.archived,
    },
    revision: revision(value.revision, true) as HandbookRevision,
    history: value.history.map((row) => revision(row, false) as Omit<HandbookRevision, 'body'>),
  };
}

function editorOptions(value: unknown): HandbookEditorOptionsRead {
  if (!object(value) || !integer(value.version) || !Array.isArray(value.editors))
    return invalid();
  return {
    version: value.version,
    editors: value.editors.map((row) => {
      if (!object(row) || !nonEmpty(row.user_id) || !integer(row.granted_at))
        return invalid();
      return {user_id: row.user_id, granted_at: row.granted_at};
    }),
  };
}

/**
 * Binds the strict native transport to the incoming request context. Responses
 * are structurally validated before they reach a page; malformed data never
 * degrades into a retained read.
 */
export function createNativeHandbookReads(
  client: NativeClient,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  actorHoldingSlug: string,
): HandbookReads {
  const current = (actor: WorkContext) => {
    if (actor.holdingSlug !== holdingSlug || actorHoldingSlug !== holdingSlug)
      throw new ApiError(
        409,
        'NATIVE_HANDBOOK_CONTEXT_CHANGED',
        'Your Holding changed. Reload the handbook in its original account.',
      );
  };
  return {
    async handbookAccess(actor) {
      current(actor);
      // The list read is the existing owner-authorized carrier for `access`;
      // do not open a second retained authority path just to render the page.
      return list(await client.list(incoming, holdingSlug, {limit: 1})).access;
    },
    async handbookList(actor, query = '', editing = false, cursor) {
      current(actor);
      return list(
        await client.list(incoming, holdingSlug, {
          q: query,
          edit: editing,
          cursor,
        }),
      );
    },
    async handbookArticle(actor, id, editing = false, revision) {
      current(actor);
      if (!nonEmpty(id) || id.length > 200)
        throw new ApiError(
          422,
          'NATIVE_HANDBOOK_REQUEST_INVALID',
          'Choose a valid handbook article.',
        );
      return article(
        await client.article(incoming, holdingSlug, id, {
          edit: editing,
          revision,
        }),
      );
    },
    async handbookEditorOptions(actor) {
      current(actor);
      return editorOptions(await client.editors(incoming, holdingSlug));
    },
  };
}

/** Native handbook reads, or undefined when the retained path is configured. */
export async function nativeHandbookReads(
  actor: WorkContext,
  holdingSlug: string,
): Promise<HandbookReads | undefined> {
  const raw = process.env.ERPX_NATIVE_HANDBOOK_CONFIG;
  if (raw === undefined) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new ApiError(
      409,
      'NATIVE_HANDBOOK_CONTEXT_CHANGED',
      'Your Holding changed. Reload the handbook in its original account.',
    );
  return createNativeHandbookReads(
    createNativeHandbookClient(raw),
    await headers(),
    holdingSlug,
    actor.holdingSlug,
  );
}

/**
 * The handbook page reader: native when configured, retained when unset. A
 * configured native failure is allowed to escape unchanged.
 */
export async function handbookReads(
  actor: WorkContext,
  holdingSlug: string,
): Promise<HandbookReads> {
  return (
    (await nativeHandbookReads(actor, holdingSlug)) ??
    collaborationPresentationQueries(actor.holdingId)
  );
}
