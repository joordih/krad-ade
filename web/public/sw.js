self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("push", (event) => {
  const notice = event.data ? event.data.json() : { title: "Krad", body: "", tag: "krad", url: "/" };
  event.waitUntil(
    self.registration.showNotification(notice.title, {
      body: notice.body,
      tag: notice.tag,
      icon: "/icon-192.png",
      badge: "/icon-192.png",
      data: { url: notice.url },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = new URL(event.notification.data?.url ?? "/", self.location.origin).href;
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((windows) => {
      const open = windows[0];
      if (!open) return self.clients.openWindow(url);
      open.postMessage({ type: "open", url });
      return open.focus();
    }),
  );
});
