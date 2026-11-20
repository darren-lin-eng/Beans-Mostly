const $ = (s, r = document) => r.querySelector(s);
const el = (tag, props = {}, ...kids) => {
  const n = Object.assign(document.createElement(tag), props);
  for (const k of kids.flat()) if (k != null) n.append(k);
  return n;
};
const api = async (path, body) => {
  const res = await fetch(path, body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : undefined);
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `Request failed (${res.status})`);
  return data;
};

// ---- state (persisted locally; nothing leaves the browser except what you search) ----
const load = (k, d) => { try { return JSON.parse(localStorage.getItem('bm:' + k)) ?? d; } catch { return d; } };
const save = (k, v) => { try { localStorage.setItem('bm:' + k, JSON.stringify(v)); } catch {} };
const state = {
  pantry: load('pantry', []), shopping: load('shopping', []),
  plan: load('plan', []),          // [{id, title, servings, baseServings, custom?}]
  stores: load('stores', []), tags: new Set(), config: null, allRecipes: new Map(),
};

// ---- tabs ----
document.querySelectorAll('nav [role=tab]').forEach((b) => b.addEventListener('click', () => {
  document.querySelectorAll('nav [role=tab]').forEach((x) => x.setAttribute('aria-selected', x === b));
  document.querySelectorAll('.tab').forEach((t) => (t.hidden = t.id !== 'tab-' + b.dataset.tab));
}));
const goTab = (name) => $(`nav [data-tab=${name}]`).click();

// ---- pantry / shopping lists ----
function renderLists() {
  for (const key of ['pantry', 'shopping']) {
    const ul = $(`#${key}-list`);
    ul.replaceChildren(...state[key].map((item, i) => el('li', {}, item,
      el('button', { title: `Remove ${item}`, 'aria-label': `Remove ${item}`, onclick: () => { state[key].splice(i, 1); save(key, state[key]); renderLists(); } }, '×'))));
  }
}
function addItems(key, text) {
  for (const raw of text.split(/[,\n]/)) {
    const v = raw.trim();
    if (v && !state[key].some((x) => x.toLowerCase() === v.toLowerCase())) state[key].push(v);
  }
  save(key, state[key]); renderLists();
}
document.querySelectorAll('form.add').forEach((f) => f.addEventListener('submit', (e) => {
  e.preventDefault();
  const input = $('input', f);
  addItems(f.dataset.list, input.value); input.value = '';
}));
$('#clear-shopping').onclick = () => { state.shopping = []; save('shopping', []); renderLists(); $('#price-results').replaceChildren(); };

// ---- price comparison ----
$('#compare-prices').onclick = async () => {
  const out = $('#price-results');
  if (!state.shopping.length) return out.replaceChildren(el('p', { className: 'muted' }, 'Add something to your shopping list first.'));
  out.replaceChildren(el('p', { className: 'muted' }, 'Checking prices…'));
  try {
    const ids = state.stores.filter((s) => s.hasProducts).map((s) => s.id);
    if (!ids.length) return out.replaceChildren(el('p', { className: 'muted' }, 'No price data yet: search for stores using a source that has prices (e.g. Kroger) on the Stores tab. OpenStreetMap only has locations.'));
    const { stores } = await api('/api/prices', { items: state.shopping, storeIds: ids });
    const names = Object.fromEntries(state.stores.map((s) => [s.id, s.name]));
    out.replaceChildren(...stores.map((s, i) => el('details', { open: i === 0 },
      el('summary', {}, `${names[s.storeId] || s.storeId} — $${s.total.toFixed(2)} `, el('span', { className: i === 0 ? 'best' : 'muted' }, `(${s.found}/${state.shopping.length} found${i === 0 ? ', best' : ''})`)),
      el('table', {}, ...s.rows.map((r) => el('tr', {}, el('td', {}, r.item), el('td', { className: r.product ? '' : 'muted' }, r.product ? `${r.product.name} · ${r.product.size || ''}` : 'not found'), el('td', {}, r.product ? '$' + r.product.price.toFixed(2) : '')))))));
  } catch (e) { out.replaceChildren(el('p', { className: 'error' }, e.message)); }
};

// ---- stores ----
async function searchStores(provider, locText, silent) {
  const status = $('#store-status');
  const qs = new URLSearchParams({ provider });
  if ($('#chain').value.trim()) qs.set('chain', $('#chain').value.trim());
  if (locText?.lat != null) { qs.set('lat', locText.lat); qs.set('lng', locText.lng); } else if (locText) qs.set('q', locText);
  if (!silent) status.textContent = 'Searching…';
  const { stores } = await api('/api/stores?' + qs);
  state.stores = stores; save('stores', stores);
  if (!silent) status.textContent = stores.length ? `${stores.length} stores found.` : 'No stores found in that area.';
  renderStores();
}
function renderStores() {
  $('#store-results').replaceChildren(...state.stores.map((s) => {
    const results = el('div');
    const input = el('input', { placeholder: 'Search this store (e.g. milk)', 'aria-label': `Search ${s.name}` });
    const form = el('form', { className: 'row', onsubmit: async (e) => {
      e.preventDefault();
      if (!input.value.trim()) return;
      results.replaceChildren(el('p', { className: 'muted' }, 'Searching…'));
      try {
        const { products } = await api(`/api/products?store=${encodeURIComponent(s.id)}&q=${encodeURIComponent(input.value)}`);
        results.replaceChildren(...(products.length ? products.slice(0, 6).map((p) => el('div', { className: 'prod' },
          el('span', {}, `${p.name} `, el('span', { className: 'muted' }, p.size || '')), el('span', {}, `$${p.price.toFixed(2)} `,
            el('button', { className: 'ghost', title: 'Add to shopping list', onclick: () => addItems('shopping', p.name) }, '＋')))) : [el('p', { className: 'muted' }, 'No matches.')]));
      } catch (err) { results.replaceChildren(el('p', { className: 'error' }, err.message)); }
    } }, input, el('button', { className: 'secondary' }, 'Search'));
    return el('div', { className: 'card' },
      el('h2', {}, s.name), el('p', { className: 'muted' }, `${s.distanceKm} km · ${s.address}${s.hours ? ' · ' + s.hours : ''}`),
      el('div', {}, ...(s.tags || []).filter(Boolean).map((t) => el('span', { className: 'tag' }, t))),
      s.hasProducts ? [form, results] : el('p', { className: 'muted' }, 'This data source has no product or price info.'));
  }));
}
$('#search-stores').onclick = async () => {
  try {
    const provider = $('#provider').value, loc = $('#loc').value.trim();
    if (!loc) return ($('#store-status').textContent = 'Enter a ZIP, city or address (or use your location).');
    await searchStores(provider, loc || null);
  } catch (e) { $('#store-status').textContent = e.message; }
};
$('#geo').onclick = () => {
  if (!navigator.geolocation) return ($('#store-status').textContent = 'Geolocation is not available in this browser.');
  navigator.geolocation.getCurrentPosition(
    (p) => searchStores($('#provider').value, { lat: p.coords.latitude, lng: p.coords.longitude }).catch((e) => ($('#store-status').textContent = e.message)),
    () => ($('#store-status').textContent = 'Location permission denied — type a ZIP or address instead.'));
};

// ---- recipes ----
const custom = new Map(); // AI recipes keyed by id (kept for this session)
function recipeCard(r) {
  const pct = Math.round(r.coverage * 100);
  const inPlan = state.plan.some((p) => p.id === r.id);
  const full = state.allRecipes.get(r.id) || custom.get(r.id);
  return el('div', { className: 'card' },
    el('h2', {}, r.title), el('div', {}, el('span', { className: 'tag' }, `${r.minutes} min`), el('span', { className: 'tag' }, `serves ${r.servings}`), ...r.tags.map((t) => el('span', { className: 'tag' }, t))),
    el('div', { className: 'meter', role: 'img', 'aria-label': `${pct}% of ingredients covered` }, el('i', { style: `width:${pct}%` })),
    el('p', { className: 'muted' }, r.missing.length ? `${pct}% covered · need ${r.missing.join(', ')}` : 'You have everything!'),
    el('ul', { className: 'ing' }, ...r.ingredients.map((i) => el('li', { className: i.status }, i.name, i.status === 'pantry' ? ' · pantry' : i.status === 'shopping' ? ' · on list' : ''))),
    full?.steps ? el('details', {}, el('summary', {}, 'Method'), el('ol', {}, ...full.steps.map((s) => el('li', {}, s)))) : null,
    el('button', { className: inPlan ? 'secondary' : '', disabled: inPlan, onclick: () => addToPlan(r) }, inPlan ? 'In plan ✓' : 'Add to meal plan'));
}
function showRecipes(list) {
  $('#recipe-results').replaceChildren(...list.map(recipeCard));
}
$('#find-recipes').onclick = async () => {
  const status = $('#recipe-status');
  if (!state.pantry.length && !state.shopping.length) { status.textContent = 'Add some ingredients to your pantry or shopping list first.'; return; }
  status.textContent = 'Matching…';
  try {
    const { recipes } = await api('/api/recipes/match', { pantry: state.pantry, shopping: state.shopping, tags: [...state.tags] });
    status.textContent = recipes.length ? `${recipes.length} recipes, best match first.` : 'No recipes match well yet — try adding more ingredients or clearing filters.';
    showRecipes(recipes);
  } catch (e) { status.textContent = e.message; }
};
$('#gen-recipes').onclick = async () => {
  const status = $('#recipe-status');
  status.textContent = 'Asking Claude…';
  try {
    const { recipes } = await api('/api/recipes/generate', { pantry: state.pantry, shopping: state.shopping, preferences: $('#prefs').value });
    recipes.forEach((r) => custom.set(r.id, { id: r.id, title: r.title, servings: r.servings, steps: r.steps, ingredients: r.fullIngredients }));
    status.textContent = `${recipes.length} AI-generated recipes.`;
    showRecipes(recipes);
  } catch (e) { status.textContent = e.message; }
};

// ---- meal plan → shopping list (the "vice versa" direction) ----
function renderPlan() {
  save('plan', state.plan);
  $('#plan-card').hidden = !state.plan.length;
  $('#plan-list').replaceChildren(...state.plan.map((p, i) => el('li', {}, el('span', {}, p.title),
    el('label', {}, 'Servings ', el('input', { type: 'number', min: 1, max: 24, value: p.servings, onchange: (e) => { p.servings = Math.max(1, +e.target.value || p.baseServings); save('plan', state.plan); } })),
    el('button', { className: 'ghost', 'aria-label': `Remove ${p.title}`, onclick: () => { state.plan.splice(i, 1); renderPlan(); } }, '×'))));
}
function addToPlan(r) {
  if (!state.plan.some((p) => p.id === r.id)) {
    const c = custom.get(r.id);
    state.plan.push({ id: r.id, title: r.title, servings: r.servings, baseServings: r.servings, ...(c ? { custom: c } : {}) });
  }
  renderPlan();
  document.querySelectorAll('#recipe-results .card').forEach((c) => { if ($('h2', c).textContent === r.title) { const b = $('button:last-child', c); b.textContent = 'In plan ✓'; b.disabled = true; b.className = 'secondary'; } });
}
$('#build-list').onclick = async () => {
  const status = $('#plan-status');
  try {
    const { items } = await api('/api/recipes/shopping-list', {
      picks: state.plan.map((p) => ({ id: p.id, servings: p.servings })), pantry: state.pantry, shopping: state.shopping,
      custom: state.plan.filter((p) => p.custom).map((p) => ({ ...p.custom, servings: p.baseServings })),
    });
    addItems('shopping', items.map((i) => i.name).join(','));
    status.textContent = items.length ? `Added ${items.length} items to your shopping list: ${items.map((i) => `${i.name}${i.qty ? ` (${i.qty}${i.unit ? ' ' + i.unit : ''})` : ''}`).join(', ')}.` : 'Your pantry and shopping list already cover everything.';
  } catch (e) { status.textContent = e.message; }
};

// ---- boot ----
(async function init() {
  renderLists(); renderPlan(); renderStores();
  try {
    state.config = await api('/api/config');
    $('#provider').replaceChildren(...state.config.providers.filter((p) => p.available).map((p) => el('option', { value: p.id }, p.label)));
    $('#gen-recipes').disabled = !state.config.ai;
    $('#tag-filters').replaceChildren(...state.config.tags.map((t) => el('button', { className: 'tagbtn', 'aria-pressed': 'false', onclick: (e) => {
      state.tags.has(t) ? state.tags.delete(t) : state.tags.add(t); e.target.setAttribute('aria-pressed', state.tags.has(t));
    } }, t)));
    const { recipes } = await api('/api/recipes');
    recipes.forEach((r) => state.allRecipes.set(r.id, r));
  } catch (e) { $('#store-status').textContent = 'Could not reach the server: ' + e.message; }
})();
