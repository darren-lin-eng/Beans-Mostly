// Built-in demo provider: a fake-but-plausible set of stores placed around the searched location,
// with deterministic per-store pricing. Lets the whole app work offline with zero setup.
import { readFileSync } from 'node:fs';
import { normalize, matches } from '../normalize.js';
import { haversineKm, round } from './util.js';

const catalog = JSON.parse(readFileSync(new URL('../data/sample-catalog.json', import.meta.url), 'utf8'));
const DEFAULT_CENTER = { lat: 40.7128, lng: -74.006 };

function hash(str) {
  let h = 2166136261;
  for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return (h >>> 0) / 2 ** 32; // 0..1
}

export default {
  id: 'sample',
  label: 'Sample data (demo)',
  hasProducts: true,
  available: () => true,

  async findStores({ lat, lng, radiusKm = 10 }) {
    const center = lat != null && lng != null ? { lat, lng } : DEFAULT_CENTER;
    return catalog.stores
      .map((s) => {
        const loc = { lat: center.lat + s.offset[0], lng: center.lng + s.offset[1] };
        return { id: s.id, provider: 'sample', name: s.name, chain: s.chain, address: 'Demo address', hours: s.hours,
          tags: s.tags, ...loc, distanceKm: round(haversineKm(center, loc), 1), hasProducts: true };
      })
      .filter((s) => s.distanceKm <= radiusKm)
      .sort((a, b) => a.distanceKm - b.distanceKm);
  },

  async searchProducts(storeId, query) {
    const store = catalog.stores.find((s) => s.id === storeId);
    if (!store) throw new Error(`Unknown store ${storeId}`);
    const q = normalize(query);
    if (!q) return [];
    return catalog.products
      .filter((p) => matches(p.name, q))
      .map((p) => {
        const jitter = 0.92 + hash(store.id + p.name) * 0.16; // ±8% per store+product
        return { name: p.name, category: p.category, size: p.size, price: round(p.price * store.mult * jitter), storeId };
      })
      .sort((a, b) => (normalize(b.name) === q) - (normalize(a.name) === q) || a.price - b.price); // exact name first
  },
};
