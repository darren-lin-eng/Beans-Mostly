// Kroger-family stores (Kroger, Ralphs, Fred Meyer, Harris Teeter, ...) via the official public API.
// Needs KROGER_CLIENT_ID / KROGER_CLIENT_SECRET (free at developer.kroger.com).
import { fetchJson, haversineKm, round } from './util.js';

const BASE = 'https://api.kroger.com/v1';
let token = { value: null, exp: 0 };

async function getToken() {
  if (token.value && Date.now() < token.exp) return token.value;
  const { KROGER_CLIENT_ID: id, KROGER_CLIENT_SECRET: secret } = process.env;
  const body = await fetchJson(`${BASE}/connect/oauth2/token`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded', Authorization: 'Basic ' + Buffer.from(`${id}:${secret}`).toString('base64') },
    body: 'grant_type=client_credentials&scope=product.compact',
  });
  token = { value: body.access_token, exp: Date.now() + (body.expires_in - 60) * 1000 };
  return token.value;
}
const authed = async () => ({ headers: { Authorization: `Bearer ${await getToken()}`, Accept: 'application/json' } });

export default {
  id: 'kroger',
  label: 'Kroger family (live prices)',
  hasProducts: true,
  available: () => Boolean(process.env.KROGER_CLIENT_ID && process.env.KROGER_CLIENT_SECRET),

  async findStores({ lat, lng, radiusKm = 15 }) {
    const miles = Math.max(1, Math.round(radiusKm * 0.621));
    const url = `${BASE}/locations?filter.latLong.near=${lat},${lng}&filter.radiusInMiles=${miles}&filter.limit=10`;
    const { data = [] } = await fetchJson(url, await authed());
    return data.map((l) => {
      const loc = { lat: l.geolocation?.latitude, lng: l.geolocation?.longitude };
      const a = l.address || {};
      return { id: `kroger:${l.locationId}`, provider: 'kroger', name: l.name, chain: l.chain, address: `${a.addressLine1}, ${a.city} ${a.state}`,
        hours: '', tags: [l.chain?.toLowerCase()], ...loc, distanceKm: round(haversineKm({ lat, lng }, loc), 1), hasProducts: true };
    }).sort((a, b) => a.distanceKm - b.distanceKm);
  },

  async searchProducts(storeId, query) {
    const locationId = storeId.replace(/^kroger:/, '');
    const url = `${BASE}/products?filter.term=${encodeURIComponent(query)}&filter.locationId=${locationId}&filter.limit=8`;
    const { data = [] } = await fetchJson(url, await authed());
    return data.map((p) => {
      const item = p.items?.[0] || {};
      const price = item.price?.promo || item.price?.regular;
      return { name: p.description, brand: p.brand, size: item.size, price: price ?? null, onSale: Boolean(item.price?.promo),
        category: p.categories?.[0], storeId };
    }).filter((p) => p.price != null).sort((a, b) => a.price - b.price);
  },
};
