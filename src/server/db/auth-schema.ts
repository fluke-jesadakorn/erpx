import {
  pgTable,
  text,
  bigint,
  customType,
} from "drizzle-orm/pg-core";
// Existing credential rows use epoch milliseconds; session migration 0003 uses ISO text.
// Map both explicitly, preserving legacy accounts and avoiding destructive table rebuilds.
const epoch = customType<{ data: Date; driverData: string | number }>({
  dataType: () => "bigint", toDriver: value => value.getTime(), fromDriver: value => new Date(Number(value)),
});
const booleanInteger = customType<{ data: boolean; driverData: string | number }>({
  dataType: () => "bigint", toDriver: value => value ? 1 : 0, fromDriver: value => Number(value) === 1,
});
const iso = customType<{ data: Date; driverData: string }>({
  dataType: () => "text",
  toDriver: (value) => value.toISOString(),
  fromDriver: (value) => new Date(value),
});
export const user = pgTable("user", {
  id: text("id").primaryKey(),
  name: text("name").notNull(),
  email: text("email").notNull().unique(),
  emailVerified: booleanInteger("emailVerified")
    .notNull()
    .default(false),
  image: text("image"),
  createdAt: epoch("createdAt").notNull(),
  updatedAt: epoch("updatedAt").notNull(),
});
export const session = pgTable("session", {
  id: text("id").primaryKey(),
  expiresAt: iso("expiresAt").notNull(),
  token: text("token").notNull().unique(),
  createdAt: iso("createdAt").notNull(),
  updatedAt: iso("updatedAt").notNull(),
  ipAddress: text("ipAddress"),
  userAgent: text("userAgent"),
  userId: text("userId")
    .notNull()
    .references(() => user.id, { onDelete: "cascade" }),
});
export const account = pgTable("account", {
  id: text("id").primaryKey(),
  accountId: text("accountId").notNull(),
  providerId: text("providerId").notNull(),
  userId: text("userId")
    .notNull()
    .references(() => user.id, { onDelete: "cascade" }),
  accessToken: text("accessToken"),
  refreshToken: text("refreshToken"),
  idToken: text("idToken"),
  accessTokenExpiresAt: epoch("accessTokenExpiresAt"),
  refreshTokenExpiresAt: epoch("refreshTokenExpiresAt"),
  scope: text("scope"),
  password: text("password"),
  createdAt: epoch("createdAt").notNull(),
  updatedAt: epoch("updatedAt").notNull(),
  issuer: text("issuer").notNull().default("local:credential"),
});
export const verification = pgTable("verification", {
  id: text("id").primaryKey(),
  identifier: text("identifier").notNull(),
  value: text("value").notNull(),
  expiresAt: epoch("expiresAt").notNull(),
  createdAt: epoch("createdAt"),
  updatedAt: epoch("updatedAt"),
});
export const rateLimit = pgTable("rateLimit", {
  id: text("id").primaryKey(),
  key: text("key").notNull().unique(),
  count: bigint("count", { mode: "number" }).notNull(),
  lastRequest: bigint("lastRequest", { mode: "number" }).notNull(),
});
