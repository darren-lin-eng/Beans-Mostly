# Beans Mostly

Search local grocery stores and turn what you have (or plan to buy) into recipes — and turn recipes back into a shopping list.

```bash
cargo run --release   # http://localhost:3000   (Rust 1.85+)
cargo test
```

Written in Rust (axum + tokio). Ships as a single ~6 MB binary that serves the API and the `public/` UI.

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
| `osm` | real, via OpenStreetMap (includes Aldi, Sam's Club, etc.) | no | none — default |
| `kroger` | real Kroger-family stores | yes, live | `KROGER_CLIENT_ID` / `KROGER_CLIENT_SECRET` |

Copy `.env.example` to `.env` to configure. Set `ANTHROPIC_API_KEY` to enable recipe generation.

To add a chain/aggregator (Instacart partner API, Walmart, Open Food Facts…), add a module in `src/providers/` exposing `find_stores` / `search_products` and wire it into the `Provider` enum in `src/providers/mod.rs`.

## Layout

- `src/normalize.rs` — ingredient-name matching ("tomatoes" ≈ "cherry tomato", but peanut butter ≠ butter)
- `src/recipes.rs` — matching, shopping-list builder, Claude generation
- `src/providers/` — `osm`, `kroger`
- `src/api.rs` — HTTP routes; `src/main.rs` — startup and `.env` loading
- `data/` — recipes (compiled into the binary)
- `public/` — vanilla-JS single-page UI
- `tests/` — engine tests and HTTP end-to-end tests
