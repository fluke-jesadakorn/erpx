export type AccessEntry = {
  id: string;
  kind: "person" | "identity" | "group";
  label: string;
  email: string | null;
  userId: string | null;
  personId: string | null;
  connectionId: string | null;
  externalId: string | null;
  source: string;
  status: string;
  account: string;
  sync: string;
  accessStatus?: string;
  nodes: string[];
};
