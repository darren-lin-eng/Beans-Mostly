import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalize, matches } from '../server/normalize.js';
import { matchRecipes, buildShoppingList, parseGenerated } from '../server/recipes.js';
import sample from '../server/providers/sample.js';
import { cheapestPerItem } from '../server/index.js';

test('normalize handles plurals, descriptors and aliases', () => {
  assert.equal(normalize('Fresh Tomatoes'), 'tomato');
  assert.equal(normalize('extra virgin olive oil'), 'oil');
  assert.equal(normalize('Scallions'), 'green onion');
});

test('matches is generous for generic/specific but not for distinct compounds', () => {
  assert.ok(matches('cherry tomato', 'tomato'));
  assert.ok(matches('tomatoes', 'tomato'));
  assert.ok(!matches('peanut butter', 'butter'));
  assert.ok(!matches('coconut milk', 'milk'));
  assert.ok(!matches('rice', 'pasta'));
});

test('matchRecipes ranks by coverage and labels the source of each ingredient', () => {
  const out = matchRecipes({ pantry: ['pasta', 'garlic'], shopping: ['tomatoes', 'basil', 'parmesan'] });
  assert.equal(out[0].id, 'tomato-basil-pasta');
  assert.equal(out[0].coverage, 1);
  assert.deepEqual(out[0].ingredients.map((i) => i.status), ['pantry', 'shopping', 'pantry', 'shopping', 'shopping']);
  assert.ok(out.every((r, i) => i === 0 || out[i - 1].coverage >= r.coverage));
});

test('matchRecipes tag filter and empty input', () => {
  assert.ok(matchRecipes({ pantry: ['egg', 'bread'], tags: ['breakfast'] }).every((r) => r.tags.includes('breakfast')));
  assert.deepEqual(matchRecipes({ pantry: ['unobtainium'] }), []);
});

test('buildShoppingList subtracts pantry, scales servings, merges duplicates, skips staples', () => {
  const items = buildShoppingList({ picks: [{ id: 'tomato-basil-pasta', servings: 6 }, { id: 'spaghetti-aglio-e-olio' }], pantry: ['garlic'] });
  const by = Object.fromEntries(items.map((i) => [i.name, i]));
  assert.equal(by.pasta.qty, 500 + 200);          // 250g*2 scale + 200g, same unit merged
  assert.deepEqual(by.pasta.forRecipes.length, 2);
  assert.ok(!by.garlic && !by.oil && !by.salt);
  assert.equal(by.tomato.qty, 8);
});

test('buildShoppingList skips items already on the shopping list', () => {
  const items = buildShoppingList({ picks: [{ id: 'tomato-basil-pasta' }], shopping: ['tomatoes', 'basil'], pantry: ['pasta'] });
  assert.deepEqual(items.map((i) => i.name).sort(), ['garlic', 'parmesan']);
});

test('parseGenerated tolerates prose around JSON and drops malformed entries', () => {
  const r = parseGenerated('Sure!\n[{"title":"X","ingredients":[{"name":"egg","qty":2,"unit":""}],"steps":["cook"]},{"nope":1}]\nEnjoy');
  assert.equal(r.length, 1);
  assert.ok(r[0].tags.includes('ai'));
  assert.throws(() => parseGenerated('no json here'));
});

test('sample provider: exact match first, no coconut-milk-for-milk', async () => {
  const [hit, ...rest] = await sample.searchProducts('sample:valuemart', 'milk');
  assert.equal(hit.name, 'milk');
  assert.ok(rest.every((p) => p.name !== 'coconut milk'));
  const stores = await sample.findStores({ lat: 40, lng: -74 });
  assert.ok(stores.length >= 3 && stores[0].distanceKm <= stores.at(-1).distanceKm);
});

test('cheapestPerItem totals per store and ranks them', async () => {
  const res = await cheapestPerItem(['milk', 'pasta', 'unobtainium'], ['sample:valuemart', 'sample:greenleaf']);
  assert.equal(res.length, 2);
  assert.equal(res[0].found, 2);
  assert.ok(res[0].total <= res[1].total);       // ValueMart is the discount store
  assert.equal(res[0].rows[2].product, null);
});
