// Real nearby grocery stores from OpenStreetMap (Nominatim geocoding + Overpass). No API key needed.
// OSM has locations but no product/price data, so stores from here are for discovery only.
import { fetchJson, haversineKm, round } from './util.js';

const UA = { 'User-Agent': 'beans-mostly/0.1 (grocery recipe app)' };
const OVERPASS = process.env.OVERPASS_URL || 'https://overpass-api.de/api/interpreter';

export async function geocode(text) {
  const url = `https://nominatim.openstreetmap.org/search?format=json&limit=1&q=${encodeURIComponent(text)}`;
  const [hit] = await fetchJson(url, { headers: UA });
  if (!hit) throw new Error(`Could not find a location for "${text}"`);
  return { lat: +hit.lat, lng: +hit.lon, label: hit.display_name };
}

export default {
  id: 'osm',
  label: 'OpenStreetMap (real stores, no prices)',
  hasProducts: false,
  available: () => true,

  async findStores({ lat, lng, radiusKm = 5 }) {
    const r = Math.round(radiusKm * 1000);
    const q = `[out:json][timeout:15];nwr["shop"~"^(supermarket|grocery|greengrocer|convenience|butcher|bakery|deli)$"](around:${r},${lat},${lng});out center 60;`;
    const data = await fetchJson(OVERPASS, { method: 'POST', body: 'data=' + encodeURIComponent(q),
      headers: { ...UA, 'Content-Type': 'application/x-www-form-urlencoded' } }, 20000);
    const here = { lat, lng };
    return (data.elements || [])
      .filter((e) => e.tags?.name)
      .map((e) => {
        const loc = { lat: e.lat ?? e.center?.lat, lng: e.lon ?? e.center?.lon };
        const t = e.tags;
        const address = [t['addr:housenumber'], t['addr:street'], t['addr:city']].filter(Boolean).join(' ');
        return { id: `osm:${e.type}/${e.id}`, provider: 'osm', name: t.name, chain: t.brand || '', address: address || 'Address not listed',
          hours: t.opening_hours || '', tags: [t.shop], ...loc, distanceKm: round(haversineKm(here, loc), 1), hasProducts: false };
      })
      .filter((s) => s.lat != null)
      .sort((a, b) => a.distanceKm - b.distanceKm);
  },

  async searchProducts() { return []; },
};
