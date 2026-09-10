CREATE TABLE messaging_teams (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 owner_id TEXT NOT NULL, version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL
);
CREATE TABLE messaging_team_members (
 team_id TEXT NOT NULL REFERENCES messaging_teams(id), user_id TEXT NOT NULL,
 PRIMARY KEY(team_id,user_id)
);
CREATE TABLE messaging_rooms (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('direct','group','channel')),
 name TEXT NOT NULL, owner_id TEXT NOT NULL,
 team_id TEXT REFERENCES messaging_teams(id), private BOOLEAN NOT NULL DEFAULT false,
 direct_key TEXT, version BIGINT NOT NULL DEFAULT 1, last_seq BIGINT NOT NULL DEFAULT 0,
 created_at BIGINT NOT NULL,
 UNIQUE(company_id,direct_key),
 CHECK((kind='channel')=(team_id IS NOT NULL)),
 CHECK((kind='direct')=(direct_key IS NOT NULL))
);
CREATE TABLE messaging_room_members (
 room_id TEXT NOT NULL REFERENCES messaging_rooms(id), user_id TEXT NOT NULL,
 PRIMARY KEY(room_id,user_id)
);
CREATE TABLE messaging_messages (
 id TEXT PRIMARY KEY, room_id TEXT NOT NULL REFERENCES messaging_rooms(id),
 seq BIGINT NOT NULL, sender_id TEXT NOT NULL, sender_name TEXT NOT NULL,
 body TEXT NOT NULL CHECK(char_length(body)<=5000), created_at BIGINT NOT NULL,
 UNIQUE(room_id,seq)
);
CREATE TABLE messaging_reads (
 room_id TEXT NOT NULL REFERENCES messaging_rooms(id), user_id TEXT NOT NULL,
 seq BIGINT NOT NULL DEFAULT 0, PRIMARY KEY(room_id,user_id)
);
CREATE TABLE messaging_files (
 id TEXT PRIMARY KEY, room_id TEXT NOT NULL REFERENCES messaging_rooms(id),
 message_id TEXT REFERENCES messaging_messages(id), created_by TEXT NOT NULL,
 original_name TEXT NOT NULL, media_type TEXT NOT NULL, bytes BIGINT NOT NULL CHECK(bytes BETWEEN 1 AND 10485760),
 object_key TEXT NOT NULL UNIQUE, checksum TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE INDEX messaging_rooms_company ON messaging_rooms(company_id,team_id);
CREATE INDEX messaging_teams_company ON messaging_teams(company_id);
CREATE INDEX messaging_files_message ON messaging_files(message_id);
CREATE INDEX messaging_room_members_user ON messaging_room_members(user_id,room_id);
CREATE INDEX messaging_team_members_user ON messaging_team_members(user_id,team_id);
