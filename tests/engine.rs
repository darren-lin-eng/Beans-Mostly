use beans_mostly::api::{StorePrices, rank_stores};
use beans_mostly::normalize::{matches, normalize};
use beans_mostly::providers::Provider;
use beans_mostly::recipes::*;

fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }
fn opts<'a>(p: &'a [String], sh: &'a [String], tags: &'a [String]) -> MatchOpts<'a> {
    MatchOpts { pantry: p, shopping: sh, tags, min_coverage: 0.4, limit: 12 }
}

#[test]
fn normalize_handles_plurals_descriptors_and_aliases() {
    assert_eq!(normalize("Fresh Tomatoes"), "tomato");
    assert_eq!(normalize("extra virgin olive oil"), "oil");
    assert_eq!(normalize("Scallions"), "green onion");
    assert_eq!(normalize("tomato (about 2 cups)"), "tomato");
}

#[test]
fn matches_is_generous_for_generic_but_not_distinct_compounds() {
    assert!(matches("cherry tomato", "tomato"));
    assert!(matches("tomatoes", "tomato"));
    assert!(!matches("peanut butter", "butter"));
    assert!(!matches("coconut milk", "milk"));
    assert!(!matches("rice", "pasta"));
}

#[test]
fn match_recipes_ranks_by_coverage_and_labels_sources() {
    let (p, sh) = (s(&["pasta", "garlic"]), s(&["tomatoes", "basil", "parmesan"]));
    let out = match_recipes(&RECIPES, &opts(&p, &sh, &[]));
    assert_eq!(out[0].id, "tomato-basil-pasta");
    assert_eq!(out[0].coverage, 1.0);
    let statuses: Vec<_> = out[0].ingredients.iter().map(|i| i.status.clone()).collect();
    assert_eq!(statuses, [Status::Pantry, Status::Shopping, Status::Pantry, Status::Shopping, Status::Shopping]);
    assert!(out.windows(2).all(|w| w[0].coverage >= w[1].coverage));
}

#[test]
fn match_recipes_tag_filter_and_no_match() {
    let (p, none) = (s(&["egg", "bread"]), s(&["breakfast"]));
    assert!(match_recipes(&RECIPES, &opts(&p, &[], &none)).iter().all(|r| r.tags.contains(&"breakfast".to_string())));
    assert!(match_recipes(&RECIPES, &opts(&s(&["unobtainium"]), &[], &[])).is_empty());
}

#[test]
fn shopping_list_subtracts_pantry_scales_merges_and_skips_staples() {
    let picks = [Pick { id: "tomato-basil-pasta".into(), servings: Some(6.0) }, Pick { id: "spaghetti-aglio-e-olio".into(), servings: None }];
    let items = build_shopping_list(&RECIPES, &picks, &s(&["garlic"]), &[], false);
    let by = |n: &str| items.iter().find(|i| i.name == n);
    assert_eq!(by("pasta").unwrap().qty, 700.0); // 250g*2 scale + 200g, same unit merged
    assert_eq!(by("pasta").unwrap().for_recipes.len(), 2);
    assert!(by("garlic").is_none() && by("oil").is_none() && by("salt").is_none());
    assert_eq!(by("tomato").unwrap().qty, 8.0);
}

#[test]
fn shopping_list_skips_items_already_on_the_list() {
    let picks = [Pick { id: "tomato-basil-pasta".into(), servings: None }];
    let items = build_shopping_list(&RECIPES, &picks, &s(&["pasta"]), &s(&["tomatoes", "basil"]), false);
    assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["garlic", "parmesan"]);
}

#[test]
fn parse_generated_tolerates_prose_and_drops_malformed() {
    let r = parse_generated("Sure!\n[{\"title\":\"X\",\"ingredients\":[{\"name\":\"egg\",\"qty\":2,\"unit\":\"\"}],\"steps\":[\"cook\"]},{\"nope\":1}]\nEnjoy").unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].tags.contains(&"ai".to_string()));
    assert!(parse_generated("no json here").is_err());
}

#[test]
fn providers_declare_price_support_honestly() {
    assert!(Provider::Osm.available() && !Provider::Osm.has_products());
    assert!(Provider::Kroger.has_products());
    assert_eq!(Provider::for_store("kroger:01400943"), Some(Provider::Kroger));
    assert_eq!(Provider::for_store("osm:node/1"), Some(Provider::Osm));
    assert_eq!(Provider::for_store("sample:x"), None);
}

#[test]
fn stores_rank_by_items_found_then_total() {
    let sp = |id: &str, total: f64, found: usize| StorePrices { store_id: id.into(), rows: vec![], total, found };
    let ranked = rank_stores(vec![sp("a", 9.0, 2), sp("b", 12.0, 3), sp("c", 8.0, 3)]);
    assert_eq!(ranked.iter().map(|s| s.store_id.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
}
