import { readFileSync } from 'node:fs';
import { normalize, matches } from './normalize.js';

export const RECIPES = JSON.parse(readFileSync(new URL('./data/recipes.json', import.meta.url), 'utf8'));

const isStaple = (i) => Boolean(i.staple);
const covered = (ing, have) => have.some((h) => matches(h, ing.name));

/**
 * Direction 1: "what can I cook with what I have / am buying?"
 * pantry + shopping are lists of ingredient names. Recipes are ranked by how much of
 * their non-staple ingredients are covered. Staples (salt, oil…) never count against a recipe.
 */
export function matchRecipes({ pantry = [], shopping = [], recipes = RECIPES, tags = [], minCoverage = 0.4, limit = 12 }) {
  const wanted = tags.map((t) => t.toLowerCase());
  return recipes
    .filter((r) => wanted.every((t) => r.tags.includes(t)))
    .map((r) => {
      const core = r.ingredients.filter((i) => !isStaple(i));
      const rows = core.map((i) => ({
        name: i.name,
        status: covered(i, pantry) ? 'pantry' : covered(i, shopping) ? 'shopping' : 'missing',
      }));
      const have = rows.filter((x) => x.status !== 'missing').length;
      return {
        id: r.id, title: r.title, minutes: r.minutes, servings: r.servings, tags: r.tags,
        coverage: core.length ? have / core.length : 1,
        ingredients: rows,
        missing: rows.filter((x) => x.status === 'missing').map((x) => x.name),
        usesPantry: rows.filter((x) => x.status === 'pantry').length,
      };
    })
    .filter((r) => r.coverage >= minCoverage && (pantry.length + shopping.length === 0 || r.ingredients.some((i) => i.status !== 'missing')))
    .sort((a, b) => b.coverage - a.coverage || a.missing.length - b.missing.length || a.minutes - b.minutes)
    .slice(0, limit);
}

/**
 * Direction 2: "I picked these recipes — what do I need to buy?"
 * picks: [{id, servings?}]. Scales quantities, merges duplicates, subtracts the pantry and anything already on the list.
 */
export function buildShoppingList({ picks = [], pantry = [], shopping = [], recipes = RECIPES, includeStaples = false }) {
  const merged = new Map();
  for (const pick of picks) {
    const r = recipes.find((x) => x.id === pick.id);
    if (!r) continue;
    const scale = pick.servings ? pick.servings / r.servings : 1;
    for (const ing of r.ingredients) {
      if (isStaple(ing) && !includeStaples) continue;
      if (covered(ing, pantry) || covered(ing, shopping)) continue; // already owned or already on the list
      const key = `${normalize(ing.name)}|${ing.unit}`;
      const row = merged.get(key) || { name: ing.name, qty: 0, unit: ing.unit, forRecipes: [] };
      row.qty += (ing.qty || 0) * scale;
      if (!row.forRecipes.includes(r.title)) row.forRecipes.push(r.title);
      merged.set(key, row);
    }
  }
  return [...merged.values()]
    .map((x) => ({ ...x, qty: Math.round(x.qty * 100) / 100 }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** Optional: ask Claude for new recipes. Throws a tagged error when no key is configured. */
export async function generateRecipes({ pantry = [], shopping = [], preferences = '', count = 3 }) {
  const key = process.env.ANTHROPIC_API_KEY;
  if (!key) throw Object.assign(new Error('ANTHROPIC_API_KEY is not set'), { code: 'NO_AI' });
  const model = process.env.ANTHROPIC_MODEL || 'claude-sonnet-5-5';
  const prompt = `Create ${count} practical home recipes using mostly these ingredients.
Pantry (already owned): ${pantry.join(', ') || 'none'}
Shopping list (will buy): ${shopping.join(', ') || 'none'}
Preferences/restrictions: ${preferences || 'none'}
Prefer recipes that need few extra ingredients; ordinary staples (salt, pepper, oil, water) are assumed.
Reply with ONLY a JSON array. Each element: {"title":string,"minutes":number,"servings":number,"tags":string[],
"ingredients":[{"name":string (singular, generic e.g. "tomato"),"qty":number,"unit":string,"staple":boolean (optional)}],"steps":string[]}`;
  const res = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: { 'x-api-key': key, 'anthropic-version': '2023-06-01', 'content-type': 'application/json' },
    body: JSON.stringify({ model, max_tokens: 4000, messages: [{ role: 'user', content: prompt }] }),
    signal: AbortSignal.timeout(60000),
  });
  if (!res.ok) throw new Error(`Anthropic API ${res.status}: ${(await res.text()).slice(0, 200)}`);
  const text = (await res.json()).content?.map((b) => b.text || '').join('') || '';
  return parseGenerated(text);
}

export function parseGenerated(text) {
  const start = text.indexOf('['), end = text.lastIndexOf(']');
  if (start < 0 || end < start) throw new Error('Model did not return a JSON array');
  const arr = JSON.parse(text.slice(start, end + 1));
  return arr
    .filter((r) => r && typeof r.title === 'string' && Array.isArray(r.ingredients) && Array.isArray(r.steps))
    .map((r, i) => ({
      id: `ai-${Date.now().toString(36)}-${i}`, title: r.title, minutes: Number(r.minutes) || 30, servings: Number(r.servings) || 2,
      tags: [...(r.tags || []), 'ai'], steps: r.steps.map(String),
      ingredients: r.ingredients.filter((g) => g && g.name).map((g) => ({ name: String(g.name), qty: Number(g.qty) || 0, unit: String(g.unit || ''), ...(g.staple ? { staple: true } : {}) })),
    }));
}
