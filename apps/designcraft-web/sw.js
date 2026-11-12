// DesignCraft Editorial PWA Service Worker
const CACHE_NAME = 'designcraft-pwa-v1';

self.addEventListener('install', (event) => {
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) => {
      return Promise.all(
        keys.map((key) => {
          if (key !== CACHE_NAME) {
            return caches.delete(key);
          }
        })
      );
    }).then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Bypass API endpoints, leases, documents, cloud sync and Google OAuth
  if (
    url.pathname.startsWith('/api/') ||
    url.pathname.startsWith('/documents/') ||
    url.pathname.startsWith('/templates/') ||
    url.hostname.includes('googleapis.com') ||
    url.hostname.includes('google.com') ||
    event.request.method !== 'GET'
  ) {
    return;
  }

  // Network-first strategy for app shell & wasm with cache fallback
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        if (!response || response.status !== 200 || response.type !== 'basic') {
          return response;
        }
        const responseToCache = response.clone();
        caches.open(CACHE_NAME).then((cache) => {
          cache.put(event.request, responseToCache);
        });
        return response;
      })
      .catch(() => caches.match(event.request))
  );
});

