/* Only generic Inbox prompts are shown on the lock screen. */
self.addEventListener("push", event => {
  let data = {};
  try { data = event.data ? event.data.json() : {}; } catch { /* Generic prompt still works. */ }
  const tag = typeof data.tag === "string" && /^[0-9a-f-]{36}$/.test(data.tag) ? data.tag : "erpx-inbox";
  const path = typeof data.path === "string" && (
    /^\/holdings\/[a-zA-Z0-9%_-]+(?:\/[^?#]*)?\/inbox\?tab=updates$/.test(data.path) ||
    /^\/[a-zA-Z0-9_-]+\/[a-zA-Z0-9_-]+\/(?:holding|branches\/[a-zA-Z0-9_-]+(?:\/departments\/[a-zA-Z0-9_-]+)?|departments\/[a-zA-Z0-9_-]+)\/notifications$/.test(data.path)
  )
    ? data.path : "/";
  event.waitUntil(self.registration.showNotification("ERPX", {
    body: "You have a work update.", tag, renotify: false,
    data: { path }, icon: "/favicon.ico",
  }));
});
self.addEventListener("notificationclick", event => {
  event.notification.close();
  const path = event.notification.data?.path || "/";
  const url = new URL(path, self.location.origin);
  if (url.origin !== self.location.origin) return;
  event.waitUntil(clients.matchAll({ type: "window", includeUncontrolled: true }).then(async tabs => {
    const tab = tabs.find(item => item.url === url.href);
    if (tab) return tab.focus();
    return clients.openWindow(url.href);
  }));
});
