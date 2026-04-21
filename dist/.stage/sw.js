self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

// Network-first fetch. This keeps behavior simple and avoids stale app state.
self.addEventListener("fetch", () => {});
