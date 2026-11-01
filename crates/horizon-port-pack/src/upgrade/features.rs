//! Configured and placed features.
use super::{Report, kind, states, vanilla};
use serde_json::{Map, Value, json};

/// Feature types 26.3 dropped for compositions of other features; a pack
/// using them needs vanilla's composition, which the upgrade does not
/// write.
const REPLACED_FEATURES: [&str; 7] = [
    "basalt_columns",
    "basalt_pillar",
    "desert_well",
    "glowstone_blob",
    "nether_forest_vegetation",
    "twisting_vines",
    "weeping_vines",
];

/// 26.2 block state provider types and their 26.3 names.
fn provider(t: &str) -> Option<&'static str> {
    Some(match t {
        "simple_state_provider" => "simple_state_provider",
        "weighted_state_provider" => "weighted",
        "rotated_block_provider" => "rotated",
        "randomized_int_state_provider" => "randomized_int",
        "rule_based_state_provider" => "rule_based",
        "noise_provider" => "noise",
        "dual_noise_provider" => "dual_noise",
        "noise_threshold_provider" => "noise_threshold",
        _ => return None,
    })
}

/// One placement modifier: `random_offset` became `offset` with one
/// provider per axis (x and z both from `xz_spread`, drawn in that order).
fn placement(m: &mut Value) {
    if kind(m) == Some("random_offset") {
        let xz = m.get("xz_spread").cloned().unwrap_or(json!(0));
        let y = m.get("y_spread").cloned().unwrap_or(json!(0));
        *m = json!({"type": "minecraft:offset", "x": xz.clone(), "y": y, "z": xz});
    }
}

/// A configured feature in 26.3's form: `config` unwrapped, block state
/// providers renamed, nested features and placements upgraded.
pub(super) fn feature(v: &Value, report: &mut Report) -> Value {
    let Value::Object(o) = v else {
        return v.clone();
    };
    let Some(t) = kind(v) else {
        return v.clone();
    };
    if REPLACED_FEATURES.contains(&t) {
        report.unknown(format!("feature {t} (written as data in 26.3)"));
    }
    let mut n = Map::new();
    n.insert("type".into(), o["type"].clone());
    if let Some(Value::Object(config)) = o.get("config") {
        for (k, x) in config {
            n.insert(k.clone(), x.clone());
        }
    }
    if t == "tree" {
        below_trunk(&mut n);
    }
    hardcoded(t, &mut n);
    let t = match t {
        "forest_rock" => "block_blob",
        "ice_spike" => "spike",
        "dripstone_cluster" => "speleothem_cluster",
        "pointed_dripstone" => "speleothem",
        other => other,
    };
    n.insert("type".into(), vanilla(t));
    let mut v = Value::Object(n);
    // Providers first: a simple state provider becomes its state in object form
    // (`{"id": ...}`), which 26.x reads as a state where a bare name would be a provider id.
    nested(&mut v, report);
    states(&mut v);
    v
    }

    /// What 1.21.11's feature classes hardcode, written as the fields 26.x reads (each from
    /// that class's `place`): lake replacement rules, huge mushroom soil, dripstone blocks,
    /// the forest rock and ice spike block checks.
    fn hardcoded(t: &str, f: &mut Map<String, Value>) {
        let not_tag = |tag: &str| json!({"type": "minecraft:not", "predicate":
            {"type": "minecraft:matching_block_tag", "tag": tag}});
        let rename = |f: &mut Map<String, Value>, from: &str, to: &str| {
            if let Some(v) = f.remove(from) {
                f.insert(to.into(), v);
            }
        };
        let dripstone = |f: &mut Map<String, Value>| {
            f.insert("base_block".into(), json!("minecraft:dripstone_block"));
            f.insert("pointed_block".into(), json!("minecraft:pointed_dripstone"));
            f.insert("replaceable_blocks".into(), json!("#minecraft:dripstone_replaceable_blocks"));
        };
        match t {
            // LakeFeature: always places; canReplaceBlock and the barrier's stone check.
            "lake" => {
                f.insert("can_place_feature".into(), json!({"type": "minecraft:true"}));
                f.insert("can_replace_with_air_or_fluid".into(), not_tag("minecraft:features_cannot_replace"));
                f.insert("can_replace_with_barrier".into(), not_tag("minecraft:lava_pool_stone_cannot_replace"));
            }
            // AbstractHugeMushroomFeature.isValidPosition: #dirt or #mushroom_grow_block below.
            "huge_red_mushroom" | "huge_brown_mushroom" => {
                f.insert("can_place_on".into(), json!({"type": "minecraft:any_of", "predicates": [
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:dirt"},
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:mushroom_grow_block"}]}));
            }
            // BlockBlobFeature: lands on #dirt or #base_stone_overworld.
            "forest_rock" => {
                f.insert("can_place_on".into(), json!({"type": "minecraft:any_of", "predicates": [
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:dirt"},
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:base_stone_overworld"}]}));
            }
            // IceSpikeFeature: stands on snow blocks, replaces air, #dirt, snow blocks and ice.
            "ice_spike" => {
                f.insert("state".into(), json!("minecraft:packed_ice"));
                f.insert("can_place_on".into(), json!({"type": "minecraft:matching_blocks", "blocks": "minecraft:snow_block"}));
                f.insert("can_replace".into(), json!({"type": "minecraft:any_of", "predicates": [
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:air"},
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:dirt"},
                    {"type": "minecraft:matching_blocks", "blocks": ["minecraft:snow_block", "minecraft:ice"]}]}));
            }
            "large_dripstone" => {
                f.insert("replaceable_blocks".into(), json!("#minecraft:dripstone_replaceable_blocks"));
            }
            "dripstone_cluster" => {
                dripstone(f);
                rename(f, "dripstone_block_layer_thickness", "speleothem_block_layer_thickness");
                rename(f, "chance_of_dripstone_column_at_max_distance_from_center", "chance_of_speleothem_at_max_distance_from_center");
                rename(f, "max_distance_from_edge_affecting_chance_of_dripstone_column", "max_distance_from_edge_affecting_chance_of_speleothem");
            }
            "pointed_dripstone" => {
                dripstone(f);
                rename(f, "chance_of_taller_dripstone", "chance_of_taller_generation");
            }
            // RootSystemFeature has no level test in 1.21.11.
            "root_system" => {
                f.entry("level_test_distance").or_insert(json!(0));
                f.entry("max_level_deviation").or_insert(json!(0));
            }
            _ => {}
        }
    }

    /// A tree's `dirt_provider` and `force_dirt` (1.21.11 and older) as 26.x's
    /// `below_trunk_provider`. `TrunkPlacer.setDirtAt` places the provider's block when
    /// `force_dirt`, or when the block below is not `#dirt` apart from grass and mycelium
    /// (which it does turn into dirt).
    fn below_trunk(tree: &mut Map<String, Value>) {
        let Some(provider) = tree.remove("dirt_provider") else {
            return;
        };
        let force = tree.remove("force_dirt").and_then(|f| f.as_bool()).unwrap_or(false);
        let below = if force {
            provider
        } else {
            json!({"type": "minecraft:rule_based_state_provider", "rules": [{
                "if_true": {"type": "minecraft:not", "predicate": {"type": "minecraft:all_of", "predicates": [
                    {"type": "minecraft:matching_block_tag", "tag": "minecraft:dirt"},
                    {"type": "minecraft:not", "predicate": {"type": "minecraft:matching_blocks",
                        "blocks": ["minecraft:grass_block", "minecraft:mycelium"]}}]}},
                "then": provider}]})
        };
        tree.insert("below_trunk_provider".into(), below);
    }

/// Inside a feature: block state providers, placed features and
/// configured features held inline.
fn nested(v: &mut Value, report: &mut Report) {
    match v {
        Value::Object(o) => {
            // RuleBasedBlockStateProvider.CODEC (1.21.11): a bare `{fallback, rules}`
            // where the field's type is fixed (DiskConfiguration.state_provider).
            if o.get("type").is_none() && o.contains_key("rules") && o.contains_key("fallback") {
                o.insert("type".into(), vanilla("rule_based"));
            }
            if let Some(t) = kind(&Value::Object(o.clone())) {
                if let Some(to) = provider(t) {
                    if t == "simple_state_provider" {
                        let mut state = o.get("state").cloned().unwrap_or_default();
                        if let Value::Object(s) = &mut state {
                            if let Some(name) = s.remove("Name") {
                                s.insert("id".into(), name);
                            }
                            if let Some(p) = s.remove("Properties") {
                                s.insert("properties".into(), p);
                            }
                        }
                        *v = state;
                        return;
                    }
                    o.insert("type".into(), vanilla(to));
                    // Noise providers carry their noise parameters inline, in the old form.
                    for key in ["noise", "slow_noise"] {
                        if let Some(n) = o.get(key).filter(|n| n.get("firstOctave").is_some()).cloned() {
                            o.insert(key.into(), super::noise::noise(&n, report));
                        }
                    }
                    }
            }
            // An inline placed feature: {feature, placement}.
            if o.contains_key("placement") && o.contains_key("feature") {
                let placed = placed(&Value::Object(o.clone()), report);
                *v = placed;
                return;
            }
            for x in o.values_mut() {
                if x.get("config").is_some() && x.get("type").is_some() {
                    *x = feature(x, report);
                } else {
                    nested(x, report);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| nested(x, report)),
        _ => {}
    }
}

/// A placed feature: its feature (an id or inline) and its placement
/// modifiers.
pub(super) fn placed(v: &Value, report: &mut Report) -> Value {
    let mut v = v.clone();
    // The inline feature first (its providers keep their states as objects), then the
    // placement's own states (`would_survive`, predicates).
    if let Some(f) = v.get("feature").cloned()
        && f.is_object()
    {
        v["feature"] = feature(&f, report);
    }
    if let Some(Value::Array(ms)) = v.get_mut("placement") {
        ms.iter_mut().for_each(states);
    }
    if let Some(Value::Array(ms)) = v.get_mut("placement") {
        for m in ms.iter_mut() {
            placement(m);
        }
    }
    v
}
