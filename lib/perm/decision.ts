export interface ResourceRef {
  type?: string;
  id?: string | number | null;
  submitterId?: number | null;
  submitterDeptId?: string | null;
  submitterLevel?: number | null;
  uploaderId?: number | null;
  requesterId?: number | null;
  ownerId?: number | null;
  deptId?: string | null;
  totalAmount?: number | null;
  currentStage?: string | null;
  [extra: string]: unknown;
}

export type Decision =
  | {
      allow: true;
      reason: string;
      matchedPerm?: string;
      matchedPolicy?: string;
      scope?: string;
    }
  | {
      allow: false;
      reason: string;
      matchedPerm?: string;
      matchedPolicy?: string;
      scope?: string;
    };

export type AuthAction =
  | { kind: 'perm'; perm: string }
  | { kind: 'policy'; policy: string }
  | { kind: 'stage'; stage: string }
  | { kind: 'stage_with_policy'; stage: string; policy: string };
