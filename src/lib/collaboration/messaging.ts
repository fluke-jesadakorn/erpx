export type MessagingPerson = { id: string; name: string; email: string };
export type MessagingTeam = {
  organization_managed?: boolean;
  id: string;
  name: string;
  owner_id: string;
  version: number;
};
export type MessagingRoom = {
  organization_node_id?: string | null;
  id: string;
  name: string;
  kind: "direct" | "group" | "channel";
  team_id: string | null;
  private: boolean;
  owner_id: string;
  version: number;
  last_seq: number;
  unread: number;
  preview: string | null;
  updated_at: number;
};
export type MessagingFile = {
  id: string;
  original_name: string;
  media_type: string;
  bytes: number;
};
export type MessagingMessage = {
  id: string;
  seq: number;
  sender_id: string;
  sender_name: string;
  body: string;
  created_at: number;
  files: MessagingFile[];
};
export type MessagingOverview = {
  teams: MessagingTeam[];
  rooms: MessagingRoom[];
  purchase?:import('./messaging-purchases').MessagingPurchaseControls;
};
export type MessagingHistory = {
  room: MessagingRoom;
  messages: MessagingMessage[];
  hasMore: boolean;
};
