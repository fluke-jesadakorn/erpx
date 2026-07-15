import 'server-only';
import { cache } from 'react';
import { cookies } from 'next/headers';
import {
  hasPermission,
  loadPermSessionFromCookieValue,
  STAGE_TO_PERM,
  matchPerm,
  parseDeptFromPerms,
  parseRoleId,
  effectOf,
  type PermSession,
} from '@folio-lib/perm/server';
import { query } from '@folio-lib/db';
import {
  tileFromRow,
  type TileDef,
  type TileGrantSummary,
} from './tile-config';
import type { BilingualText } from '@folio-lib/i18n/types';
import type { TileAccess } from './tileAccess';

const REASON_NO_SESSION = 'No actor session.';
const REASON_MET = 'Allowed by your role.';
const REASON_RESTRICTED = 'Restricted by your role.';

const SESSION_COOKIE = 'folio_session';

export interface ActorLite {
  id: number;
  role_id?: string | null;
  role_name?: string | null;
  permissions?: string[] | null;
}

export const loadActorAsSession = cache(async (): Promise<PermSession | null> => {
  try {
    const c = await cookies();
    const tok = c.get(SESSION_COOKIE)?.value ?? null;
    const out = await loadPermSessionFromCookieValue(tok);
    return out?.session ?? null;
  } catch {
    return null;
  }
});

interface TileDataBundle {
  tiles: TileDef[];
  viewPermById: Map<string, string>;
}

const loadTileData = cache(async (): Promise<TileDataBundle> => {
  const tilesRes = await query<{
    id: string;
    display_name: string;
    subtitle: string;
    icon: string;
    accent: string;
    group_name: string;
    sub_view: string | null;
    href: string;
    view_perm_id: string;
    request_target: string | null;
    sort_order: number;
  }>(
    `SELECT id, display_name, COALESCE(subtitle,'') AS subtitle,
            COALESCE(icon,'square') AS icon, COALESCE(accent,'slate') AS accent,
            group_name, COALESCE(sub_view,'') AS sub_view,
            COALESCE(href,'') AS href,
            view_perm_id,
            COALESCE(request_target,'') AS request_target,
            sort_order
       FROM perm.tiles
      ORDER BY sort_order ASC, id ASC`,
  );
  const tiles = tilesRes.rows.map((r) => tileFromRow(r as never));
  const viewPermById = new Map<string, string>();
  for (const r of tilesRes.rows) viewPermById.set(r.id, r.view_perm_id);
  return { tiles, viewPermById };
});

function gateOk(tilePerm: string | undefined, perms: string[] | null | undefined): boolean {
  if (!perms || perms.length === 0) return false;
  // Admin bypass grants everything.
  if (perms.includes('admin:system:bypass::allow')) return true;
  // No specific perm required → open.
  if (!tilePerm) return true;
  // Tile-specific perm check (also covers system:authenticated::allow when it
  // is the tile's view_perm_id).
  return matchPerm(perms, tilePerm);
}

function buildSummary(tile: TileDef): TileGrantSummary {
  return {
    tile_id: tile.id,
    display_name: tile.display_name,
    module_id: tile.module_id,
    href: tile.href,
    group_name: tile.group,
    is_available: false,
    grant_reason: '',
    grant_source_role: null,
    grant_role_level: null,
    request_target:
      tile.request_target === 'hr_manager' ||
      tile.request_target === 'cfo' ||
      tile.request_target === 'admin'
        ? tile.request_target
        : null,
  } as unknown as TileGrantSummary;
}

export function evaluateTileFromBundle(
  tile: TileDef,
  actor: ActorLite | null | undefined,
  bundle: TileDataBundle,
): TileAccess {
  const live = bundle.tiles.find((t) => t.id === tile.id) ?? tile;
  if (!actor?.id) {
    return { state: 'locked', reason: REASON_NO_SESSION, summary: buildSummary(live) };
  }
  const tilePerm = bundle.viewPermById.get(tile.id);
  const ok = gateOk(tilePerm, actor.permissions);
  return {
    state: ok ? 'open' : 'locked',
    reason: ok ? REASON_MET : REASON_RESTRICTED,
    source: 'perm',
    summary: buildSummary(live),
  };
}

export async function evaluateTile(
  tile: TileDef,
  actor: ActorLite | null | undefined,
): Promise<TileAccess> {
  const bundle = await loadTileData();
  return evaluateTileFromBundle(tile, actor, bundle);
}

export interface TileFlags {
  canLedger: boolean;
  canCockpit: boolean;
  canPr: boolean;
  canSettings: boolean;
  canHr: boolean;
  canViewSubordinatePrs: boolean;
  canApprovePO: boolean;
  canSettlePO: boolean;
  canResolveAccessRequest: boolean;
  canEditRoles: boolean;
}

export function tileFlags(session: PermSession | null): TileFlags {
  return {
    canLedger: hasPermission(session, 'tile:ledger:view::allow'),
    canCockpit: hasPermission(session, 'tile:cockpit:view::allow'),
    canPr: hasPermission(session, 'tile:pr:view::allow'),
    canSettings: hasPermission(session, 'tile:settings:view::allow'),
    canHr: hasPermission(session, 'tile:hr:view::allow'),
    canViewSubordinatePrs: hasPermission(session, 'tile:my_prs:view::allow'),
    canApprovePO: hasPermission(session, 'tile:po:view::allow'),
    canSettlePO: hasPermission(session, 'tile:po:view::allow'),
    canResolveAccessRequest: hasPermission(session, 'access_request:request:resolve::allow'),
    canEditRoles: hasPermission(session, 'rbac:matrix:edit::allow'),
  };
}

const APPROVAL_STAGES = [
  'dept_verification',
  'dept_authorization',
  'accounting_verification',
  'accounting_supervision',
  'accounting_authorization',
  'cfo_authorization',
  'ceo_authorization',
  'disbursement_authorization',
] as const;

export function stageAllowMap(session: PermSession | null): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const s of APPROVAL_STAGES) {
    const perm = STAGE_TO_PERM[s];
    out[s] = perm ? hasPermission(session, perm) : false;
  }
  return out;
}

export interface AccessRow {
  id: number;
  fullname: string;
  employee_code: string;
  is_active: boolean;
  department: string | null;
  department_th: string | null;
  department_de: string | null;
  dept_id: string | null;
  effective_level: number | null;
  role_id: string | null;
  role_name: string | null;
  perm_role_ids: string[];
  perm_role_names: string[];
}

export interface AccessRole {
  id: string;
  display_name: string;
  display_name_th: string | null;
  display_name_de: string | null;
  description: string | null;
  is_system: boolean;
  level: number;
  sort_order: number;
  user_count: number;
  allow: string[];
  deny: string[];
}

export const loadUsersAndRoles = cache(async (): Promise<{
  users: AccessRow[] | null;
  roles: AccessRole[] | null;
  permCount: number | null;
}> => {
  const [usersRes, rolesRes, permsRes, grantsRes] = await Promise.all([
    query<{
      id: number; fullname: string; employee_code: string; is_active: boolean;
      role_id: string | null; dept_perm: string | null;
      role_ids: string[] | null; role_names: string[] | null;
    }>(
      `SELECT u.id, u.fullname, u.employee_code, u.is_active,
              (SELECT ur.role_id FROM perm.user_roles ur
                WHERE ur.user_id = u.id
                ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                               WHEN ur.role_id LIKE '%::2' THEN 1
                               WHEN ur.role_id LIKE '%::3' THEN 2
                               WHEN ur.role_id LIKE '%::4' THEN 3
                               WHEN ur.role_id LIKE '%::5' THEN 4
                               ELSE 5 END), ur.granted_at ASC
                LIMIT 1) AS role_id,
              (SELECT up.permission_id FROM perm.user_permissions up
                WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                  AND up.revoked_at IS NULL
                  AND (up.ends_at IS NULL OR up.ends_at > now())
                ORDER BY up.permission_id LIMIT 1) AS dept_perm,
              COALESCE((SELECT array_agg(ur.role_id ORDER BY ur.role_id)
                          FROM perm.user_roles ur WHERE ur.user_id = u.id),
                        ARRAY[]::text[]) AS role_ids,
              COALESCE((SELECT array_agg(r.display_name ORDER BY r.display_name)
                          FROM perm.user_roles ur JOIN perm.roles r ON r.id = ur.role_id
                         WHERE ur.user_id = u.id),
                        ARRAY[]::text[]) AS role_names
         FROM users u
         ORDER BY u.id`,
    ),
    query<{
      id: string; display_name: string; description: string | null;
      is_system: boolean; sort_order: number; display_name_th: string | null;
      display_name_de: string | null;
    }>(
      `SELECT id, display_name, description, is_system, sort_order,
              display_name_th, display_name_de
         FROM perm.roles
        ORDER BY is_system DESC, sort_order, id`,
    ),
    query<{ count: string }>(`SELECT COUNT(*)::text AS count FROM perm.permissions`),
    query<{ role_id: string; permission_id: string }>(
      `SELECT role_id, permission_id FROM perm.role_permissions`,
    ),
    query<{ role_id: string; count: number }>(
      `SELECT role_id, COUNT(*)::int AS count FROM perm.user_roles GROUP BY role_id`,
    ),
  ]);

  const grantsByRole = new Map<string, { allow: string[]; deny: string[] }>();
  for (const g of grantsRes.rows) {
    const entry = grantsByRole.get(g.role_id) ?? { allow: [], deny: [] };
    if (effectOf(g.permission_id) === 'deny') entry.deny.push(g.permission_id);
    else entry.allow.push(g.permission_id);
    grantsByRole.set(g.role_id, entry);
  }
  const userCountByRole = new Map<string, number>();
  for (const r of (await query<{ role_id: string; count: number }>(
    `SELECT role_id, COUNT(*)::int AS count FROM perm.user_roles GROUP BY role_id`,
  )).rows) userCountByRole.set(r.role_id, r.count);

  const users: AccessRow[] = usersRes.rows.map((u) => {
    const parsed = parseRoleId(u.role_id ?? 'officer::5');
    const deptId = u.dept_perm
      ? u.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
      : null;
    return {
      id: u.id, fullname: u.fullname, employee_code: u.employee_code, is_active: u.is_active,
      department: deptId, department_th: deptId, department_de: deptId,
      dept_id: deptId, dept_group_id: deptId as any,
      effective_level: parsed?.level ?? 5,
      role_id: u.role_id, role_name: parsed?.name ?? 'officer',
      perm_role_ids: u.role_ids ?? [], perm_role_names: u.role_names ?? [],
    } as AccessRow;
  });

  const roles: AccessRole[] = rolesRes.rows.map((r) => {
    const parsed = parseRoleId(r.id);
    const grants = grantsByRole.get(r.id) ?? { allow: [], deny: [] };
    return {
      id: r.id, display_name: r.display_name, description: r.description,
      is_system: r.is_system, sort_order: r.sort_order,
      display_name_th: r.display_name_th, display_name_de: r.display_name_de,
      level: parsed?.level ?? 0,
      user_count: userCountByRole.get(r.id) ?? 0,
      allow: grants.allow.sort(), deny: grants.deny.sort(),
    };
  });

  return {
    users,
    roles,
    permCount: permsRes.rows?.[0] ? Number(permsRes.rows[0].count) : null,
  };
});

export interface TileAccessBundle {
  tiles: TileDef[];
  accessByTile: Record<string, TileAccess>;
  tileFlags: TileFlags;
  stageAllow: Record<string, boolean>;
  deptNames: Map<string, string>;
  deptNamesTh: Map<string, string>;
  deptNamesDe: Map<string, string>;
}

export const loadTileAccessBundle = cache(async (
  session: PermSession | null,
): Promise<TileAccessBundle> => {
  const bundle = await loadTileData();
  const perms = session?.permissions ?? [];

  const accessByTile: Record<string, TileAccess> = {};
  for (const t of bundle.tiles) {
    const tilePerm = bundle.viewPermById.get(t.id);
    const ok = gateOk(tilePerm, perms);
    accessByTile[t.id] = {
      state: ok ? 'open' : 'locked',
      reason: ok ? REASON_MET : REASON_RESTRICTED,
      source: 'perm',
      summary: buildSummary(t),
    };
  }
  return {
    tiles: bundle.tiles,
    accessByTile,
    tileFlags: tileFlags(session),
    stageAllow: stageAllowMap(session),
    deptNames: new Map(), deptNamesTh: new Map(), deptNamesDe: new Map(),
  };
});

export { parseDeptFromPerms };
