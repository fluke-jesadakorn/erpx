export type PortalScope = {
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  itemId: string;
  itemVersion: number;
  shareId: string;
};

export type PortalAttachment = {
  setId: string;
  linkId: string;
  sourceId: string;
  objectKey: string;
  checksum: string;
  mediaType: string;
  bytes: number;
  expectedTargetVersion: number;
};

