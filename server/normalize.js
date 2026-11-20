// Ingredient-name normalisation shared by recipe matching and price lookup.
const ALIASES = {
  scallion: 'green onion', 'spring onion': 'green onion', cilantro: 'coriander',
  'bell pepper': 'pepper', capsicum: 'pepper', garbanzo: 'chickpea', 'garbanzo bean': 'chickpea',
  'ground beef': 'beef', mince: 'beef', 'chicken breast': 'chicken', 'chicken thigh': 'chicken',
  'olive oil': 'oil', 'vegetable oil': 'oil', 'canola oil': 'oil', 'extra virgin olive oil': 'oil',
  'all purpose flour': 'flour', 'all-purpose flour': 'flour', spaghetti: 'pasta', penne: 'pasta',
  'cheddar cheese': 'cheddar', 'parmesan cheese': 'parmesan', 'soy sauce': 'soy sauce',
};
const DESCRIPTORS = new Set(['fresh', 'large', 'small', 'medium', 'chopped', 'diced', 'minced',
  'sliced', 'ripe', 'organic', 'boneless', 'skinless', 'whole', 'frozen', 'canned', 'dried', 'cooked',
  'grated', 'shredded', 'lean', 'unsalted', 'salted', 'low-sodium', 'of', 'a', 'the']);

function singular(w) {
  if (w.length > 3 && w.endsWith('ies')) return w.slice(0, -3) + 'y';
  if (w.length > 3 && /(oes|ches|shes|sses)$/.test(w)) return w.slice(0, -2);
  if (w.length > 3 && w.endsWith('s') && !w.endsWith('ss')) return w.slice(0, -1);
  return w;
}

export function normalize(name) {
  let s = String(name || '').toLowerCase().replace(/\(.*?\)/g, ' ').replace(/[^a-z\- ]/g, ' ').replace(/\s+/g, ' ').trim();
  s = s.split(' ').map(singular).join(' ');
  if (ALIASES[s]) return ALIASES[s];
  const words = s.split(' ').filter((w) => w && !DESCRIPTORS.has(w));
  s = words.join(' ');
  return ALIASES[s] || s;
}

// Compounds whose head word must not satisfy a plain head-word need ("peanut butter" is not "butter").
const DISTINCT = new Set(['peanut butter', 'almond butter', 'coconut milk', 'almond milk', 'oat milk', 'soy milk',
  'sweet potato', 'cream cheese', 'butter bean', 'coconut oil', 'sesame oil', 'ice cream', 'sour cream',
  'green onion', 'soy sauce', 'tomato sauce', 'tomato paste', 'hot sauce', 'fish sauce']);

// Does a thing the user has (pantry / list entry) satisfy a recipe ingredient?
export function matches(have, need) {
  const a = normalize(have), b = normalize(need);
  if (!a || !b) return false;
  if (a === b) return true;
  if (DISTINCT.has(a) || DISTINCT.has(b)) return false;
  const wa = a.split(' '), wb = b.split(' ');
  // "cherry tomato" <-> "tomato": one side generic, same head word.
  return wa[wa.length - 1] === wb[wb.length - 1] && (wa.length === 1 || wb.length === 1);
}
