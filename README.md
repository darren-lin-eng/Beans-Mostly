# Beans Mostly

Search local grocery stores and turn what you have (or plan to buy) into recipes — and turn recipes back into a shopping list.

```bash
npm start        # http://localhost:3000  (Node 20+, zero dependencies)
npm test
```

## What it does

| Direction | How |
|---|---|
| **Lists → recipes** | Add items to your *pantry* and *shopping list*; **Recipes → Find recipes** ranks recipes by how much of them you already cover (pantry vs. "on list" vs. missing). Staples like salt/oil are assumed. |
| **Recipes → list** | Add recipes to the meal plan, set servings, and **Add missing ingredients** — quantities are scaled, merged, and reduced by your pantry and existing list. |
| **Stores** | Find stores near a ZIP/address/GPS, search a store's products, and **compare the price of your whole shopping list** across stores. |
| **AI ideas** | Optional: Claude invents new recipes from your lists and preferences. |

Lists and the meal plan live in your browser's `localStorage`.

## Data sources (`server/providers/`)

| Provider | Stores | Prices | Setup |
|---|---|---|---|
| `sample` | fake, placed around you | yes (demo) | none — default |
| `osm` | real, via OpenStreetMap | no | none |
| `kroger` | real Kroger-family stores | yes, live | `KROGER_CLIENT_ID` / `KROGER_CLIENT_SECRET` |

Copy `.env.example` to `.env` to configure. Set `ANTHROPIC_API_KEY` to enable recipe generation.

To add a chain/aggregator (Instacart partner API, Walmart, Open Food Facts…), implement `{ id, label, hasProducts, available(), findStores(), searchProducts() }` and register it in `server/providers/index.js`.

## Layout

- `server/normalize.js` — ingredient-name matching ("tomatoes" ≈ "cherry tomato", but peanut butter ≠ butter)
- `server/recipes.js` — matching, shopping-list builder, Claude generation
- `server/data/` — recipes and the sample catalog
- `public/` — vanilla-JS single-page UI
