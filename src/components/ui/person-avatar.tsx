import styles from "./person-avatar.module.css";

export function PersonAvatar({
  name,
  identity = name,
  small = false,
}: {
  name: string;
  identity?: string;
  small?: boolean;
}) {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const initials =
    parts.length > 1
      ? `${Array.from(parts[0])[0]}${Array.from(parts.at(-1)!)[0]}`
      : Array.from(parts[0] ?? "?")
          .slice(0, 2)
          .join("");
  const tone =
    Array.from(identity).reduce((sum, char) => (sum * 31 + char.codePointAt(0)!) >>> 0, 0) % 3;
  return (
    <span
      aria-hidden="true"
      className={`${styles.avatar} ${small ? styles.small : ""}`}
      data-tone={tone}
    >
      {initials.toLocaleUpperCase("en")}
    </span>
  );
}
