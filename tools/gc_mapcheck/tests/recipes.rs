//! Recipes must make maps the checker passes. This sweeps a few seeds of
//! each Field Battle recipe, at both sizes, and fifty of each Conquest
//! recipe at each size, through the checker. When a recipe is tuned, the
//! build also looks at a few by hand, with `gc_mapcheck gen RECIPE SIZE
//! SEED OUT.ron`, `gc_mapcheck check` and `gc_mapcheck png`.

use gc_map::SizeClass;
use gc_map::conquest::{self, CONQUEST_SIZES, ConquestRecipe};
use gc_map::recipe::{Recipe, describe};
use gc_mapcheck::{BUDGET, check, generate};

#[test]
fn every_recipe_makes_maps_the_checker_passes() {
    let mut failures = Vec::new();
    for recipe in Recipe::ALL {
        for tiles in [256, 320] {
            for seed in 1..=12 {
                let report = check(&describe(recipe, tiles, seed));
                if let Some(first) = report.errors.first() {
                    failures.push(format!("{} {tiles} seed {seed}: {first}", recipe.name()));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} recipe maps failed the checker:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// How many seeds of each Conquest recipe and size the pass-rate test
/// checks, and the share of them that must pass before any reroll.
const CONQUEST_SEEDS: u64 = 50;
const CONQUEST_PASS_PERCENT: u64 = 80;

/// Most seeds of every Conquest recipe make a map the checker passes, so
/// a change that breaks a recipe's fairness fails the build. The sizes
/// run side by side.
#[test]
fn conquest_recipes_pass_the_checker_for_most_seeds() {
    let jobs: Vec<(ConquestRecipe, SizeClass)> = ConquestRecipe::ALL
        .into_iter()
        .flat_map(|r| CONQUEST_SIZES.map(|s| (r, s)))
        .collect();
    let results: Vec<(String, u64, Option<String>)> = std::thread::scope(|scope| {
        let running: Vec<_> = jobs
            .iter()
            .map(|&(recipe, size)| {
                scope.spawn(move || {
                    let mut passed = 0;
                    let mut first = None;
                    for seed in 1..=CONQUEST_SEEDS {
                        let report = check(&conquest::describe(recipe, size, seed).unwrap());
                        match report.errors.first() {
                            None => passed += 1,
                            Some(e) => {
                                first.get_or_insert_with(|| format!("seed {seed}: {e}"));
                            }
                        }
                    }
                    (format!("{} {size:?}", recipe.name()), passed, first)
                })
            })
            .collect();
        running.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let low: Vec<String> = results
        .iter()
        .filter(|(_, passed, _)| passed * 100 < CONQUEST_SEEDS * CONQUEST_PASS_PERCENT)
        .map(|(what, passed, first)| format!("{what}: {passed}/{CONQUEST_SEEDS} ({first:?})"))
        .collect();
    assert!(
        low.is_empty(),
        "recipes below {CONQUEST_PASS_PERCENT}%:\n{}",
        low.join("\n")
    );
}

/// The reroll loop hands back a map the checker passes, from the first
/// seed that makes one, and refuses a size the recipes don't make.
#[test]
fn generate_rerolls_to_a_map_the_checker_passes() {
    for recipe in ConquestRecipe::ALL {
        let (seed, def) = generate(recipe, SizeClass::Duel, 7, BUDGET).unwrap();
        assert!(seed >= 7);
        assert!(check(&def).ok(), "{} seed {seed}", recipe.name());
    }
    let err = generate(ConquestRecipe::Fen, SizeClass::Grand, 1, BUDGET).unwrap_err();
    assert!(err[0].contains("Grand"), "{err:?}");
}
