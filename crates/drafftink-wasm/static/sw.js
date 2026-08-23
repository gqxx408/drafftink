// Service Worker for Drafftink WASM client.
//
// Caches the WASM binary and static assets for offline use.
// Strategy: cache-first for static assets, network-first for API requests.

const CACHE_NAME = 'drafftink-v1';
const STATIC_ASSETS = [
    '/',
    '/index.html',
    '/drafftink_wasm.js',
    '/drafftink_wasm_bg.wasm',
    '/sw.js',
];

// ── Install: pre-cache static assets ──────────────────────────────────────
self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => cache.addAll(STATIC_ASSETS))
            .then(() => self.skipWaiting())
            .catch((err) => console.warn('SW cache addAll failed:', err))
    );
});

// ── Activate: clean up old caches ─────────────────────────────────────────
self.addEventListener('activate', (event) => {
    event.waitUntil(
        caches.keys()
            .then((cacheNames) => {
                return Promise.all(
                    cacheNames
                        .filter((name) => name !== CACHE_NAME)
                        .map((name) => caches.delete(name))
                );
            })
            .then(() => self.clients.claim())
    );
});

// ── Fetch: cache-first for static, network-first for API ──────────────────
self.addEventListener('fetch', (event) => {
    const request = event.request;

    // Skip non-GET requests for caching (let them pass through)
    if (request.method !== 'GET') {
        return;
    }

    const url = new URL(request.url);

    // Network-first for API requests
    if (url.pathname.startsWith('/api/')) {
        event.respondWith(
            fetch(request)
                .then((response) => {
                    // Cache successful API responses
                    if (response.ok) {
                        const responseClone = response.clone();
                        caches.open(CACHE_NAME).then((cache) => {
                            cache.put(request, responseClone);
                        });
                    }
                    return response;
                })
                .catch(() => {
                    // Fall back to cache when offline
                    return caches.match(request);
                })
        );
        return;
    }

    // Cache-first for static assets
    event.respondWith(
        caches.match(request)
            .then((cachedResponse) => {
                if (cachedResponse) {
                    // Return cached response, and update cache in background
                    fetch(request)
                        .then((response) => {
                            if (response.ok) {
                                caches.open(CACHE_NAME).then((cache) => {
                                    cache.put(request, response);
                                });
                            }
                        })
                        .catch(() => {});
                    return cachedResponse;
                }

                // Not in cache — fetch from network
                return fetch(request)
                    .then((response) => {
                        if (response.ok && response.type === 'basic') {
                            const responseClone = response.clone();
                            caches.open(CACHE_NAME).then((cache) => {
                                cache.put(request, responseClone);
                            });
                        }
                        return response;
                    })
                    .catch(() => {
                        // Offline and not cached — return a fallback
                        if (request.destination === 'document') {
                            return caches.match('/index.html');
                        }
                    });
            })
    );
});
