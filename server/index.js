import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize as pathNormalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { providers, providerForStore } from './providers/index.js';
import { geocode } from './providers/osm.js';
import { RECIPES, matchRecipes, buildShoppingList, generateRecipes } from './recipes.js';
import { round } from './providers/util.js';

// Minimal .env loader (no dependency).
try {
  for (const line of (await readFile(new URL('../.env', import.meta.url), 'utf8')).split('\n')) {
    const m = line.match(/^\s*([A-Z_]+)\s*=\s*(.*?)\s*$/);
    if (m && m[2] && !(m[1] in process.env)) process.env[m[1]] = m[2];
  }
} catch {}

const PUBLIC = fileURLToPath(new URL('../public', import.meta.url));
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.svg': 'image/svg+xml', '.json': 'application/json' };

class HttpError extends Error { constructor(status, msg) { super(msg); this.status = status; } }
const list = (v) => (Array.isArray(v) ? v.map(String).map((s) => s.trim()).filter(Boolean).slice(0, 200) : []);

async function body(req) {
  let raw = '';
  for await (const c of req) { raw += c; if (raw.length > 1e6) throw new HttpError(413, 'Body too large'); }
  try { return raw ? JSON.parse(raw) : {}; } catch { throw new HttpError(400, 'Invalid JSON'); }
}

async function resolveLocation(q) {
  if (q.get('lat') && q.get('lng')) return { lat: +q.get('lat'), lng: +q.get('lng') };
  const text = q.get('q');
  if (text) return geocode(text);
  return null;
}

export async function cheapestPerItem(items, storeIds) {
  const stores = [];
  for (const storeId of storeIds) {
    const p = providerForStore(storeId);
    if (!p?.hasProducts) continue;
    const rows = await Promise.all(items.map(async (name) => {
      try { const hits = await p.searchProducts(storeId, name); return { item: name, product: hits[0] || null }; }
      catch (e) { return { item: name, product: null, error: e.message }; }
    }));
    stores.push({
      storeId, rows,
      total: round(rows.reduce((s, r) => s + (r.product?.price || 0), 0)),
      found: rows.filter((r) => r.product).length,
    });
  }
  // Best store = most items found, then lowest total.
  return stores.sort((a, b) => b.found - a.found || a.total - b.total);
}

const routes = {
  'GET /api/config': async () => ({
    providers: Object.values(providers).map((p) => ({ id: p.id, label: p.label, available: p.available(), hasProducts: p.hasProducts })),
    ai: Boolean(process.env.ANTHROPIC_API_KEY),
    tags: [...new Set(RECIPES.flatMap((r) => r.tags))].sort(),
  }),

  'GET /api/stores': async (req, q) => {
    const p = providers[q.get('provider') || 'sample'];
    if (!p || !p.available()) throw new HttpError(400, 'Provider unavailable');
    const loc = (await resolveLocation(q)) || (p.id === 'sample' ? {} : null);
    if (!loc) throw new HttpError(400, 'Provide lat/lng or a q= place/zip');
    const radiusKm = Math.min(+q.get('radiusKm') || 8, 50);
    const stores = await p.findStores({ ...loc, radiusKm });
    return { location: loc, stores };
  },

  'GET /api/products': async (req, q) => {
    const storeId = q.get('store'), term = q.get('q');
    const p = providerForStore(storeId);
    if (!p || !term) throw new HttpError(400, 'store and q are required');
    return { products: await p.searchProducts(storeId, term) };
  },

  'POST /api/prices': async (req) => {
    const b = await body(req);
    return { stores: await cheapestPerItem(list(b.items).slice(0, 40), list(b.storeIds).slice(0, 8)) };
  },

  'POST /api/recipes/match': async (req) => {
    const b = await body(req);
    return { recipes: matchRecipes({ pantry: list(b.pantry), shopping: list(b.shopping), tags: list(b.tags), minCoverage: b.minCoverage ?? 0.4 }) };
  },

  'GET /api/recipes': async (req, q) => {
    const id = q.get('id');
    if (id) { const r = RECIPES.find((x) => x.id === id); if (!r) throw new HttpError(404, 'No such recipe'); return r; }
    return { recipes: RECIPES };
  },

  'POST /api/recipes/shopping-list': async (req) => {
    const b = await body(req);
    const custom = Array.isArray(b.custom) ? b.custom : [];
    const picks = (Array.isArray(b.picks) ? b.picks : []).slice(0, 30);
    return { items: buildShoppingList({ picks, pantry: list(b.pantry), shopping: list(b.shopping), recipes: [...RECIPES, ...custom], includeStaples: Boolean(b.includeStaples) }) };
  },

  'POST /api/recipes/generate': async (req) => {
    const b = await body(req);
    const pantry = list(b.pantry), shopping = list(b.shopping);
    try {
      const generated = await generateRecipes({ pantry, shopping, preferences: String(b.preferences || '').slice(0, 300), count: Math.min(+b.count || 3, 5) });
      const annotated = matchRecipes({ pantry, shopping, recipes: generated, minCoverage: 0, limit: 10 });
      // Return full recipe (steps etc.) merged with coverage info.
      return { recipes: annotated.map((m) => ({ ...generated.find((g) => g.id === m.id), ...m, steps: generated.find((g) => g.id === m.id).steps, fullIngredients: generated.find((g) => g.id === m.id).ingredients })) };
    } catch (e) {
      if (e.code === 'NO_AI') throw new HttpError(501, 'AI recipes are disabled: set ANTHROPIC_API_KEY on the server.');
      throw new HttpError(502, `Recipe generation failed: ${e.message}`);
    }
  },
};

async function serveStatic(pathname, res) {
  const rel = pathNormalize(pathname === '/' ? '/index.html' : pathname).replace(/^(\.\.[/\\])+/, '');
  const file = join(PUBLIC, rel);
  if (!file.startsWith(PUBLIC)) throw new HttpError(403, 'Forbidden');
  try {
    const data = await readFile(file);
    res.writeHead(200, { 'Content-Type': MIME[extname(file)] || 'application/octet-stream' });
    res.end(data);
  } catch { throw new HttpError(404, 'Not found'); }
}

export const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  try {
    const handler = routes[`${req.method} ${url.pathname}`];
    if (handler) {
      const out = await handler(req, url.searchParams);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      return res.end(JSON.stringify(out));
    }
    if (req.method !== 'GET' || url.pathname.startsWith('/api/')) throw new HttpError(404, 'Not found');
    await serveStatic(decodeURIComponent(url.pathname), res);
  } catch (e) {
    const status = e.status || 500;
    if (status === 500) console.error(e);
    res.writeHead(status, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: e.message }));
  }
});

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const port = process.env.PORT || 3000;
  server.listen(port, () => console.log(`Beans Mostly running at http://localhost:${port}`));
}
