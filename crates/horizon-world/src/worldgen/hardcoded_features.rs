//! 1.21.11 features whose rules are code, not data (26.1 writes them as data):
//! `BasaltColumnsFeature`, `BasaltPillarFeature`, `GlowstoneFeature`,
//! `NetherForestVegetationFeature`, `TwistingVinesFeature`, `WeepingVinesFeature` and
//! `DesertWellFeature`, with vanilla's draws in order.
use super::block_feature::FeatureBlockBindings;
use super::feature::FeatureWorld;
use super::placement::{PlacementContext, PlacementQueryError};
use super::state_provider::{BlockStateProvider, StateProviderError};
use super::value_provider::IntProvider;
use super::{DecodeError, Object, number_text, required};
use crate::chunk::BlockState;
use crate::registry::BlockStateId;
use horizon_core::random::{InvalidBound, RandomSource};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Debug)]
pub enum HardcodedFeatureError<E> {
    Random(InvalidBound),
    World(E),
    Query(PlacementQueryError),
    Provider(StateProviderError<E>),
}
impl<E: std::fmt::Display> std::fmt::Display for HardcodedFeatureError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Random(e) => e.fmt(f),
            Self::World(e) => e.fmt(f),
            Self::Query(e) => e.fmt(f),
            Self::Provider(e) => e.fmt(f),
        }
    }
}
impl<E> From<InvalidBound> for HardcodedFeatureError<E> {
    fn from(e: InvalidBound) -> Self {
        Self::Random(e)
    }
}
impl<E> From<PlacementQueryError> for HardcodedFeatureError<E> {
    fn from(e: PlacementQueryError) -> Self {
        Self::Query(e)
    }
}
type Res<T, E> = Result<T, HardcodedFeatureError<E>>;
type Set = Arc<BTreeSet<BlockStateId>>;

/// The blocks these features name, every state of a block where vanilla tests `is(Block)`.
#[derive(Clone, Debug)]
pub struct HardcodedBlocks {
    air: [BlockStateId; 3],
    lava: Set,
    basalt: BlockStateId,
    basalt_any: Set,
    /// BasaltColumnsFeature.CANNOT_PLACE_ON.
    cannot_place_on: BTreeSet<BlockStateId>,
    netherrack: Set,
    blackstone: Set,
    glowstone: BlockStateId,
    glowstone_any: Set,
    wart: BlockStateId,
    wart_any: Set,
    warped_nylium: Set,
    warped_wart: Set,
    /// `#nylium`.
    nylium: Set,
    /// Ages 17..=25.
    twisting: [BlockStateId; 9],
    twisting_plant: BlockStateId,
    weeping: [BlockStateId; 9],
    weeping_plant: BlockStateId,
    sand: BlockStateId,
    sand_any: Set,
    sandstone: BlockStateId,
    sandstone_slab: BlockStateId,
    water: BlockStateId,
    suspicious_sand: BlockStateId,
}
impl HardcodedBlocks {
    pub fn new(b: &FeatureBlockBindings<'_>) -> Result<Self, DecodeError> {
        let registry = b.registry();
        let missing = |name: &str| DecodeError::Registry(format!("missing block {name}"));
        let any = |name: &str| b.blocks.get(name).cloned().ok_or_else(|| missing(name));
        let default = |name: &str| registry.default_state(name).ok_or_else(|| missing(name));
        let ages = |name: &str| -> Result<[BlockStateId; 9], DecodeError> {
            let mut out = [BlockStateId(0); 9];
            for (i, state) in out.iter_mut().enumerate() {
                *state = registry
                    .resolve(&BlockState {
                        name: name.into(),
                        properties: [("age".into(), (17 + i).to_string())].into_iter().collect(),
                    })
                    .map_err(|e| DecodeError::Registry(e.to_string()))?;
            }
            Ok(out)
        };
        let mut cannot_place_on = BTreeSet::new();
        for name in [
            "lava", "bedrock", "magma_block", "soul_sand", "nether_bricks", "nether_brick_fence",
            "nether_brick_stairs", "nether_wart", "chest", "spawner",
        ] {
            cannot_place_on.extend(any(&format!("minecraft:{name}"))?.iter());
        }
        Ok(Self {
            air: b.air,
            lava: any("minecraft:lava")?,
            basalt: default("minecraft:basalt")?,
            basalt_any: any("minecraft:basalt")?,
            cannot_place_on,
            netherrack: any("minecraft:netherrack")?,
            blackstone: any("minecraft:blackstone")?,
            glowstone: default("minecraft:glowstone")?,
            glowstone_any: any("minecraft:glowstone")?,
            wart: default("minecraft:nether_wart_block")?,
            wart_any: any("minecraft:nether_wart_block")?,
            warped_nylium: any("minecraft:warped_nylium")?,
            warped_wart: any("minecraft:warped_wart_block")?,
            nylium: b.tags.get("minecraft:nylium").cloned().ok_or_else(|| missing("tag #minecraft:nylium"))?,
            twisting: ages("minecraft:twisting_vines")?,
            twisting_plant: default("minecraft:twisting_vines_plant")?,
            weeping: ages("minecraft:weeping_vines")?,
            weeping_plant: default("minecraft:weeping_vines_plant")?,
            sand: default("minecraft:sand")?,
            sand_any: any("minecraft:sand")?,
            sandstone: default("minecraft:sandstone")?,
            sandstone_slab: default("minecraft:sandstone_slab")?,
            water: default("minecraft:water")?,
            suspicious_sand: default("minecraft:suspicious_sand")?,
        })
    }
    fn air(&self, state: BlockStateId) -> bool {
        self.air.contains(&state)
    }
}

#[derive(Clone, Debug)]
pub enum HardcodedFeature {
    BasaltColumns { height: IntProvider, reach: IntProvider },
    BasaltPillar,
    Glowstone,
    NetherForestVegetation { provider: BlockStateProvider, width: i32, height: i32 },
    TwistingVines { width: i32, height: i32, max_height: i32 },
    WeepingVines,
    DesertWell,
}

impl HardcodedFeature {
    /// The 1.21.11 config with its `config` wrapper removed by the upgrader.
    pub(crate) fn decode(
        kind: &str,
        object: &Object,
        bindings: &FeatureBlockBindings<'_>,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<Self, DecodeError> {
        let positive = |name: &'static str| -> Result<i32, DecodeError> {
            number_text(required(object, name)?)?
                .parse::<i32>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or(DecodeError::Field(name))
        };
        Ok(match kind {
            "minecraft:basalt_columns" => {
                let mut int = |name: &'static str, low: i32, high: i32| {
                    let value = IntProvider::decode(required(object, name)?, depth + 1, remaining)?;
                    let [min, max] = value.bounds();
                    if min < low || max > high {
                        return Err(DecodeError::Field(name));
                    }
                    Ok(value)
                };
                let reach = int("reach", 0, 3)?;
                Self::BasaltColumns { height: int("height", 1, 10)?, reach }
            }
            "minecraft:basalt_pillar" => Self::BasaltPillar,
            "minecraft:glowstone_blob" => Self::Glowstone,
            "minecraft:nether_forest_vegetation" => {
                let (providers, catalog, _) = bindings
                    .simple_blocks
                    .as_ref()
                    .ok_or(DecodeError::Field("nether forest vegetation provider bindings"))?;
                let provider = catalog.provider_with_budget(
                    required(object, "state_provider")?.get(),
                    providers,
                    depth + 1,
                    remaining,
                )?;
                Self::NetherForestVegetation {
                    provider,
                    width: positive("spread_width")?,
                    height: positive("spread_height")?,
                }
            }
            "minecraft:twisting_vines" => Self::TwistingVines {
                width: positive("spread_width")?,
                height: positive("spread_height")?,
                max_height: positive("max_height")?,
            },
            "minecraft:weeping_vines" => Self::WeepingVines,
            "minecraft:desert_well" => Self::DesertWell,
            _ => return Err(DecodeError::Field("hardcoded feature type")),
        })
    }

    pub fn place<W: FeatureWorld + PlacementContext>(
        &self,
        b: &HardcodedBlocks,
        world: &mut W,
        random: &mut impl RandomSource,
        origin: [i32; 3],
    ) -> Res<bool, W::Error> {
        match self {
            Self::BasaltColumns { height, reach } => basalt_columns(b, world, random, origin, height, reach),
            Self::BasaltPillar => basalt_pillar(b, world, random, origin),
            Self::Glowstone => glowstone(b, world, random, origin),
            Self::NetherForestVegetation { provider, width, height } => {
                nether_forest_vegetation(b, world, random, origin, provider, *width, *height)
            }
            Self::TwistingVines { width, height, max_height } => {
                twisting_vines(b, world, random, origin, *width, *height, *max_height)
            }
            Self::WeepingVines => weeping_vines(b, world, random, origin),
            Self::DesertWell => desert_well(b, world, random, origin),
        }
    }
}

fn get<W: FeatureWorld>(world: &mut W, p: [i32; 3]) -> Res<BlockStateId, W::Error> {
    world.block(p).map_err(HardcodedFeatureError::World)
}
fn set<W: FeatureWorld>(world: &mut W, p: [i32; 3], state: BlockStateId, flags: i32) -> Res<bool, W::Error> {
    world.set_block(p, state, flags).map_err(HardcodedFeatureError::World)
}
fn empty<W: FeatureWorld>(b: &HardcodedBlocks, world: &mut W, p: [i32; 3]) -> Res<bool, W::Error> {
    Ok(b.air(get(world, p)?))
}
fn off(p: [i32; 3], dx: i32, dy: i32, dz: i32) -> [i32; 3] {
    [p[0] + dx, p[1] + dy, p[2] + dz]
}
fn manhattan(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}
/// Direction.values(): down, up, north, south, west, east.
const DIRECTIONS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
/// Mth.nextInt(random, min, max).
fn mth_next_int(random: &mut impl RandomSource, min: i32, max: i32) -> Result<i32, InvalidBound> {
    if min >= max { Ok(min) } else { Ok(random.next_int_bound(max - min + 1)? + min) }
}

fn basalt_columns<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
    height: &IntProvider,
    reach: &IntProvider,
) -> Res<bool, W::Error> {
    // chunkGenerator().getSeaLevel().
    let sea = world.generation().sea_level;
    if !can_place_at(b, world, sea, origin)? {
        return Ok(false);
    }
    let h = height.sample(random)?;
    let clustered = random.next_float() < 0.9;
    let r = h.min(if clustered { 5 } else { 8 });
    let count = if clustered { 50 } else { 15 };
    let mut placed = false;
    // BlockPos.randomBetweenClosed: x, y (a span of one) and z for each position.
    for _ in 0..count {
        let x = origin[0] - r + random.next_int_bound(2 * r + 1)?;
        let y = origin[1] + random.next_int_bound(1)?;
        let z = origin[2] - r + random.next_int_bound(2 * r + 1)?;
        let p = [x, y, z];
        let left = h - manhattan(p, origin);
        if left >= 0 {
            let reach = reach.sample(random)?;
            placed |= basalt_column(b, world, sea, p, left, reach)?;
        }
    }
    Ok(placed)
}
fn basalt_column<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    sea: i32,
    p: [i32; 3],
    height: i32,
    reach: i32,
) -> Res<bool, W::Error> {
    let mut placed = false;
    for z in p[2] - reach..=p[2] + reach {
        for x in p[0] - reach..=p[0] + reach {
            let q = [x, p[1], z];
            let d = manhattan(q, p);
            let start = if air_or_lava_ocean(b, world, sea, q)? {
                find_surface(b, world, sea, q, d)?
            } else {
                find_air(b, world, q, d)?
            };
            let Some(mut at) = start else { continue };
            let mut left = height - d / 2;
            while left >= 0 {
                if air_or_lava_ocean(b, world, sea, at)? {
                    set(world, at, b.basalt, 3)?;
                    placed = true;
                } else if !b.basalt_any.contains(&get(world, at)?) {
                    break;
                }
                at[1] += 1;
                left -= 1;
            }
        }
    }
    Ok(placed)
}
fn find_surface<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    sea: i32,
    mut p: [i32; 3],
    mut d: i32,
) -> Res<Option<[i32; 3]>, W::Error> {
    let min = world.min_y()?;
    while p[1] > min + 1 && d > 0 {
        d -= 1;
        if can_place_at(b, world, sea, p)? {
            return Ok(Some(p));
        }
        p[1] -= 1;
    }
    Ok(None)
}
fn find_air<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    mut p: [i32; 3],
    mut d: i32,
) -> Res<Option<[i32; 3]>, W::Error> {
    let max = world.max_y()?;
    while p[1] <= max && d > 0 {
        d -= 1;
        let state = get(world, p)?;
        if b.cannot_place_on.contains(&state) {
            return Ok(None);
        }
        if b.air(state) {
            return Ok(Some(p));
        }
        p[1] += 1;
    }
    Ok(None)
}
fn can_place_at<W: FeatureWorld>(b: &HardcodedBlocks, world: &mut W, sea: i32, p: [i32; 3]) -> Res<bool, W::Error> {
    if !air_or_lava_ocean(b, world, sea, p)? {
        return Ok(false);
    }
    let below = get(world, off(p, 0, -1, 0))?;
    Ok(!b.air(below) && !b.cannot_place_on.contains(&below))
}
fn air_or_lava_ocean<W: FeatureWorld>(b: &HardcodedBlocks, world: &mut W, sea: i32, p: [i32; 3]) -> Res<bool, W::Error> {
    let state = get(world, p)?;
    Ok(b.air(state) || b.lava.contains(&state) && p[1] <= sea)
}

fn basalt_pillar<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
) -> Res<bool, W::Error> {
    if !empty(b, world, origin)? || empty(b, world, off(origin, 0, 1, 0))? {
        return Ok(false);
    }
    // placeHangOff.
    fn hang<W: FeatureWorld>(b: &HardcodedBlocks, world: &mut W, random: &mut impl RandomSource, p: [i32; 3]) -> Res<bool, W::Error> {
        if random.next_int_bound(10)? != 0 {
            set(world, p, b.basalt, 2)?;
            return Ok(true);
        }
        Ok(false)
    }
    let mut p = origin;
    let (mut north, mut south, mut west, mut east) = (true, true, true, true);
    while empty(b, world, p)? {
        if world.outside_build_height(p[1])? {
            return Ok(true);
        }
        set(world, p, b.basalt, 2)?;
        north = north && hang(b, world, random, off(p, 0, 0, -1))?;
        south = south && hang(b, world, random, off(p, 0, 0, 1))?;
        west = west && hang(b, world, random, off(p, -1, 0, 0))?;
        east = east && hang(b, world, random, off(p, 1, 0, 0))?;
        p[1] -= 1;
    }
    p[1] += 1;
    for [dx, dz] in [[0, -1], [0, 1], [-1, 0], [1, 0]] {
        if random.next_boolean() {
            set(world, off(p, dx, 0, dz), b.basalt, 2)?;
        }
    }
    p[1] -= 1;
    for dx in -3i32..4 {
        for dz in -3i32..4 {
            if random.next_int_bound(10)? < 10 - dx.abs() * dz.abs() {
                let mut q = off(p, dx, 0, dz);
                let mut steps = 3;
                while empty(b, world, off(q, 0, -1, 0))? {
                    q[1] -= 1;
                    steps -= 1;
                    if steps <= 0 {
                        break;
                    }
                }
                if !empty(b, world, off(q, 0, -1, 0))? {
                    set(world, q, b.basalt, 2)?;
                }
            }
        }
    }
    Ok(true)
}

fn glowstone<W: FeatureWorld>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
) -> Res<bool, W::Error> {
    if !empty(b, world, origin)? {
        return Ok(false);
    }
    let above = get(world, off(origin, 0, 1, 0))?;
    if !b.netherrack.contains(&above) && !b.basalt_any.contains(&above) && !b.blackstone.contains(&above) {
        return Ok(false);
    }
    set(world, origin, b.glowstone, 2)?;
    for _ in 0..1500 {
        let dx = random.next_int_bound(8)? - random.next_int_bound(8)?;
        let dy = -random.next_int_bound(12)?;
        let dz = random.next_int_bound(8)? - random.next_int_bound(8)?;
        let p = off(origin, dx, dy, dz);
        if b.air(get(world, p)?) && touching(world, p, |s| b.glowstone_any.contains(&s))? == 1 {
            set(world, p, b.glowstone, 2)?;
        }
    }
    Ok(true)
}
/// How many of the six neighbours match, counting no further than two.
fn touching<W: FeatureWorld>(world: &mut W, p: [i32; 3], matches: impl Fn(BlockStateId) -> bool) -> Res<i32, W::Error> {
    let mut count = 0;
    for [dx, dy, dz] in DIRECTIONS {
        if matches(get(world, off(p, dx, dy, dz))?) {
            count += 1;
        }
        if count > 1 {
            break;
        }
    }
    Ok(count)
}

fn nether_forest_vegetation<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
    provider: &BlockStateProvider,
    width: i32,
    height: i32,
) -> Res<bool, W::Error> {
    if !b.nylium.contains(&get(world, off(origin, 0, -1, 0))?) {
        return Ok(false);
    }
    let (min, max) = (world.min_y()?, world.max_y()?);
    if !(origin[1] >= min + 1 && origin[1] + 1 <= max) {
        return Ok(false);
    }
    let mut placed = 0;
    for _ in 0..i64::from(width) * i64::from(width) {
        let dx = random.next_int_bound(width)? - random.next_int_bound(width)?;
        let dy = random.next_int_bound(height)? - random.next_int_bound(height)?;
        let dz = random.next_int_bound(width)? - random.next_int_bound(width)?;
        let p = off(origin, dx, dy, dz);
        let state = provider.get_state(world, random, p).map_err(HardcodedFeatureError::Provider)?;
        if empty(b, world, p)? && p[1] > min && world.block_survives(state, p)? {
            set(world, p, state, 2)?;
            placed += 1;
        }
    }
    Ok(placed > 0)
}

fn twisting_vines<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
    width: i32,
    height: i32,
    max_height: i32,
) -> Res<bool, W::Error> {
    let invalid = |world: &mut W, p: [i32; 3]| -> Res<bool, W::Error> {
        if !empty(b, world, p)? {
            return Ok(true);
        }
        let below = get(world, off(p, 0, -1, 0))?;
        Ok(!b.netherrack.contains(&below) && !b.warped_nylium.contains(&below) && !b.warped_wart.contains(&below))
    };
    if invalid(world, origin)? {
        return Ok(false);
    }
    for _ in 0..i64::from(width) * i64::from(width) {
        let dx = mth_next_int(random, -width, width)?;
        let dy = mth_next_int(random, -height, height)?;
        let dz = mth_next_int(random, -width, width)?;
        let mut p = off(origin, dx, dy, dz);
        // findFirstAirBlockAboveGround.
        let grounded = loop {
            p[1] -= 1;
            if world.outside_build_height(p[1])? {
                break false;
            }
            if !b.air(get(world, p)?) {
                p[1] += 1;
                break true;
            }
        };
        if grounded && !invalid(world, p)? {
            let mut length = mth_next_int(random, 1, max_height)?;
            if random.next_int_bound(6)? == 0 {
                length *= 2;
            }
            if random.next_int_bound(5)? == 0 {
                length = 1;
            }
            for i in 1..=length {
                if empty(b, world, p)? {
                    if i == length || !empty(b, world, off(p, 0, 1, 0))? {
                        let age = mth_next_int(random, 17, 25)? - 17;
                        set(world, p, b.twisting[age as usize], 2)?;
                        break;
                    }
                    set(world, p, b.twisting_plant, 2)?;
                }
                p[1] += 1;
            }
        }
    }
    Ok(true)
}

fn weeping_vines<W: FeatureWorld>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
) -> Res<bool, W::Error> {
    let roof = |s: BlockStateId| b.netherrack.contains(&s) || b.wart_any.contains(&s);
    if !empty(b, world, origin)? || !roof(get(world, off(origin, 0, 1, 0))?) {
        return Ok(false);
    }
    set(world, origin, b.wart, 2)?;
    for _ in 0..200 {
        let dx = random.next_int_bound(6)? - random.next_int_bound(6)?;
        let dy = random.next_int_bound(2)? - random.next_int_bound(5)?;
        let dz = random.next_int_bound(6)? - random.next_int_bound(6)?;
        let p = off(origin, dx, dy, dz);
        if empty(b, world, p)? && touching(world, p, roof)? == 1 {
            set(world, p, b.wart, 2)?;
        }
    }
    for _ in 0..100 {
        let dx = random.next_int_bound(8)? - random.next_int_bound(8)?;
        let dy = random.next_int_bound(2)? - random.next_int_bound(7)?;
        let dz = random.next_int_bound(8)? - random.next_int_bound(8)?;
        let mut p = off(origin, dx, dy, dz);
        if empty(b, world, p)? && roof(get(world, off(p, 0, 1, 0))?) {
            let mut length = mth_next_int(random, 1, 8)?;
            if random.next_int_bound(6)? == 0 {
                length *= 2;
            }
            if random.next_int_bound(5)? == 0 {
                length = 1;
            }
            for i in 0..=length {
                if empty(b, world, p)? {
                    if i == length || !empty(b, world, off(p, 0, -1, 0))? {
                        let age = mth_next_int(random, 17, 25)? - 17;
                        set(world, p, b.weeping[age as usize], 2)?;
                        break;
                    }
                    set(world, p, b.weeping_plant, 2)?;
                }
                p[1] -= 1;
            }
        }
    }
    Ok(true)
}

fn desert_well<W: FeatureWorld + PlacementContext>(
    b: &HardcodedBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    origin: [i32; 3],
) -> Res<bool, W::Error> {
    let min = world.min_y()?;
    let mut o = off(origin, 0, 1, 0);
    while empty(b, world, o)? && o[1] > min + 2 {
        o[1] -= 1;
    }
    if !b.sand_any.contains(&get(world, o)?) {
        return Ok(false);
    }
    for x in -2..=2 {
        for z in -2..=2 {
            if empty(b, world, off(o, x, -1, z))? && empty(b, world, off(o, x, -2, z))? {
                return Ok(false);
            }
        }
    }
    for y in -2..=0 {
        for x in -2..=2 {
            for z in -2..=2 {
                set(world, off(o, x, y, z), b.sandstone, 2)?;
            }
        }
    }
    // Direction.Plane.HORIZONTAL: north, east, south, west.
    const HORIZONTAL: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
    set(world, o, b.water, 2)?;
    for [dx, dz] in HORIZONTAL {
        set(world, off(o, dx, 0, dz), b.water, 2)?;
    }
    let below = off(o, 0, -1, 0);
    set(world, below, b.sand, 2)?;
    for [dx, dz] in HORIZONTAL {
        set(world, off(below, dx, 0, dz), b.sand, 2)?;
    }
    for x in -2..=2 {
        for z in -2..=2 {
            if x == -2 || x == 2 || z == -2 || z == 2 {
                set(world, off(o, x, 1, z), b.sandstone, 2)?;
            }
        }
    }
    for [dx, dz] in [[2, 0], [-2, 0], [0, 2], [0, -2]] {
        set(world, off(o, dx, 1, dz), b.sandstone_slab, 2)?;
    }
    for x in -1..=1 {
        for z in -1..=1 {
            let state = if x == 0 && z == 0 { b.sandstone } else { b.sandstone_slab };
            set(world, off(o, x, 4, z), state, 2)?;
        }
    }
    for y in 1..=3 {
        for [x, z] in [[-1, -1], [-1, 1], [1, -1], [1, 1]] {
            set(world, off(o, x, y, z), b.sandstone, 2)?;
        }
    }
    // Util.getRandom over [here, east, south, west, north].
    let spots = [o, off(o, 1, 0, 0), off(o, 0, 0, 1), off(o, -1, 0, 0), off(o, 0, 0, -1)];
    for depth in [1, 2] {
        let p = off(spots[random.next_int_bound(5)? as usize], 0, -depth, 0);
        // placeSusSand: getBlockEntity finds the brushable block only where the write landed.
        if set(world, p, b.suspicious_sand, 3)? {
            use crate::nbt::Tag;
            let mut nbt = crate::nbt::Compound::new();
            nbt.insert("id".into(), Tag::String("minecraft:brushable_block".into()));
            nbt.insert("LootTable".into(), Tag::String("minecraft:archaeology/desert_well".into()));
            nbt.insert(
                "LootTableSeed".into(),
                Tag::Long(horizon_core::position::BlockPos::new(p[0], p[1], p[2]).pack()),
            );
            world.set_block_entity(p, nbt)?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightmap::HeightmapType;
    use crate::registry::BlockRegistry;
    use crate::worldgen::anchor::WorldGenerationContext;
    use horizon_core::random::LegacyRandom;
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, HashMap};

    fn registry() -> &'static BlockRegistry {
        let mut states = Vec::new();
        let mut defaults = serde_json::Map::new();
        let mut add = |name: &str, props: &[(&str, String)]| {
            let name = format!("minecraft:{name}");
            if !defaults.contains_key(&name) {
                defaults.insert(name.clone(), json!(states.len()));
            }
            let properties: serde_json::Map<String, Value> =
                props.iter().map(|(k, v)| ((*k).into(), json!(v))).collect();
            states.push(json!([name, false, false, "", {"id": name, "properties": properties}]));
        };
        for name in [
            "air", "cave_air", "void_air", "basalt", "bedrock", "magma_block", "soul_sand",
            "nether_bricks", "nether_brick_fence", "nether_brick_stairs", "nether_wart", "chest",
            "spawner", "netherrack", "blackstone", "glowstone", "nether_wart_block",
            "warped_nylium", "crimson_nylium", "warped_wart_block", "twisting_vines_plant",
            "weeping_vines_plant", "sand", "sandstone", "sandstone_slab", "suspicious_sand",
            "nether_sprouts",
        ] {
            add(name, &[]);
        }
        for fluid in ["water", "lava"] {
            add(fluid, &[("level", "0".into())]);
        }
        for vine in ["twisting_vines", "weeping_vines"] {
            for age in 0..=25 {
                add(vine, &[("age", age.to_string())]);
            }
        }
        let facts = json!({"states": states, "defaults": defaults});
        Box::leak(Box::new(BlockRegistry::from_facts_json(facts.to_string().as_bytes()).unwrap()))
    }
    fn tags() -> BTreeMap<String, Vec<String>> {
        [("minecraft:nylium".to_string(), vec!["minecraft:crimson_nylium".into(), "minecraft:warped_nylium".into()])]
            .into()
    }

    /// Blocks set by hand over a layered fill (`fill(y)`), and the block entities written.
    struct World {
        registry: &'static BlockRegistry,
        fill: fn(i32) -> &'static str,
        blocks: HashMap<[i32; 3], BlockStateId>,
        entities: Vec<([i32; 3], crate::nbt::Compound)>,
    }
    impl World {
        fn new(fill: fn(i32) -> &'static str) -> Self {
            Self { registry: registry(), fill, blocks: HashMap::new(), entities: Vec::new() }
        }
        fn id(&self, name: &str) -> BlockStateId {
            self.registry.default_state(&format!("minecraft:{name}")).unwrap()
        }
        fn name(&self, p: [i32; 3]) -> String {
            let id = self.blocks.get(&p).copied().unwrap_or_else(|| self.id((self.fill)(p[1])));
            let state = self.registry.state(id).unwrap();
            state.name.trim_start_matches("minecraft:").to_string()
        }
        /// Positions written with this block.
        fn placed(&self, name: &str) -> Vec<[i32; 3]> {
            let mut out: Vec<_> = self.blocks.keys().filter(|p| self.name(**p) == name).copied().collect();
            out.sort();
            out
        }
        fn blocks(&self) -> HardcodedBlocks {
            let bindings = FeatureBlockBindings::with_tags(self.registry, BTreeSet::new(), &tags()).unwrap();
            HardcodedBlocks::new(&bindings).unwrap()
        }
        fn run(&mut self, feature: &HardcodedFeature, origin: [i32; 3]) -> bool {
            let blocks = self.blocks();
            feature.place(&blocks, self, &mut LegacyRandom::new(42), origin).unwrap()
        }
    }
    impl FeatureWorld for World {
        type Error = std::convert::Infallible;
        fn block(&mut self, p: [i32; 3]) -> Result<BlockStateId, Self::Error> {
            Ok(self.blocks.get(&p).copied().unwrap_or_else(|| self.id((self.fill)(p[1]))))
        }
        fn set_block(&mut self, p: [i32; 3], state: BlockStateId, _: i32) -> Result<bool, Self::Error> {
            self.blocks.insert(p, state);
            Ok(true)
        }
    }
    impl PlacementContext for World {
        fn generation(&self) -> WorldGenerationContext {
            WorldGenerationContext::new(0, 128, 32)
        }
        fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
            Ok(0)
        }
        fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
            Ok(127)
        }
        fn height(&mut self, _: HeightmapType, _: i32, _: i32) -> Result<i32, PlacementQueryError> {
            Err(PlacementQueryError::Unavailable)
        }
        fn block_survives(&mut self, _: BlockStateId, _: [i32; 3]) -> Result<bool, PlacementQueryError> {
            Ok(true)
        }
        fn set_block_entity(&mut self, p: [i32; 3], nbt: crate::nbt::Compound) -> Result<(), PlacementQueryError> {
            self.entities.push((p, nbt));
            Ok(())
        }
    }
    fn decode(world: &World, config: Value) -> HardcodedFeature {
        let bindings = FeatureBlockBindings::with_tags(world.registry, BTreeSet::new(), &tags()).unwrap();
        let kind = config["type"].as_str().unwrap().to_string();
        let object: Object = serde_json::from_str(&config.to_string()).unwrap();
        HardcodedFeature::decode(&kind, &object, &bindings, 0, &mut 65536).unwrap()
    }

    #[test]
    fn basalt_columns_grow_from_the_floor() {
        let mut world = World::new(|y| if y <= 10 { "netherrack" } else { "air" });
        // findSurface gets the distance from the column's centre as its budget, so a
        // centre on open floor never takes basalt: reach 0 places nothing.
        let none = decode(&world, json!({"type": "minecraft:basalt_columns", "height": 3, "reach": 0}));
        assert!(!world.run(&none, [0, 11, 0]));
        assert!(world.blocks.is_empty());
        let feature = decode(&world, json!({"type": "minecraft:basalt_columns", "height": 3, "reach": 1}));
        assert!(world.run(&feature, [0, 11, 0]));
        let basalt = world.placed("basalt");
        assert!(!basalt.is_empty());
        // Columns stand on the floor within height 3 plus reach 1 of the origin.
        for p in &basalt {
            let d = p[0].abs() + p[2].abs();
            assert!(d <= 4 && p[1] >= 11 && p[1] <= 14, "{p:?}");
            assert!(p[1] == 11 || world.name([p[0], p[1] - 1, p[2]]) == "basalt");
        }
    }

    #[test]
    fn basalt_pillar_fills_down_to_the_floor() {
        let mut world = World::new(|y| if y <= 0 || y >= 10 { "netherrack" } else { "air" });
        assert!(world.run(&HardcodedFeature::BasaltPillar, [0, 9, 0]));
        for y in 1..=9 {
            assert_eq!(world.name([0, y, 0]), "basalt");
        }
        // No pillar under an open sky.
        let mut open = World::new(|y| if y <= 0 { "netherrack" } else { "air" });
        assert!(!open.run(&HardcodedFeature::BasaltPillar, [0, 9, 0]));
        assert!(open.blocks.is_empty());
    }

    #[test]
    fn glowstone_hangs_from_the_ceiling() {
        let mut world = World::new(|y| if y >= 20 { "netherrack" } else { "air" });
        assert!(world.run(&HardcodedFeature::Glowstone, [0, 19, 0]));
        let glow = world.placed("glowstone");
        assert!(glow.contains(&[0, 19, 0]) && glow.len() > 10, "{}", glow.len());
        assert!(glow.iter().all(|p| p[0].abs() <= 7 && p[2].abs() <= 7 && (8..=19).contains(&p[1])));
    }

    #[test]
    fn nether_vegetation_needs_nylium() {
        let mut world = World::new(|y| if y <= 0 { "crimson_nylium" } else { "air" });
        let bindings = super::super::state_provider::StateProviderBindings::new(world.registry, BTreeSet::new(), &BTreeMap::new()).unwrap();
        let provider = BlockStateProvider::from_json(r#"{"id": "minecraft:nether_sprouts"}"#, &bindings).unwrap();
        let feature = HardcodedFeature::NetherForestVegetation { provider, width: 3, height: 1 };
        assert!(world.run(&feature, [0, 1, 0]));
        let sprouts = world.placed("nether_sprouts");
        // spread_height 1: every try lands on the origin's layer; 9 tries over a 5x5 square.
        assert!(!sprouts.is_empty() && sprouts.len() <= 9);
        assert!(sprouts.iter().all(|p| p[1] == 1 && p[0].abs() <= 2 && p[2].abs() <= 2));
        let mut bare = World::new(|y| if y <= 0 { "netherrack" } else { "air" });
        assert!(!bare.run(&feature, [0, 1, 0]));
        assert!(bare.blocks.is_empty());
    }

    #[test]
    fn twisting_vines_rise_from_warped_ground() {
        let mut world = World::new(|y| if y <= 0 { "warped_nylium" } else { "air" });
        let feature = decode(&world, json!({"type": "minecraft:twisting_vines", "spread_width": 4, "spread_height": 2, "max_height": 6}));
        assert!(world.run(&feature, [0, 1, 0]));
        let heads = world.placed("twisting_vines");
        assert!(!heads.is_empty());
        for head in heads {
            let age: i32 = world.registry.state(world.blocks[&head]).unwrap().properties["age"].parse().unwrap();
            assert!((17..=25).contains(&age));
            // Plants below the head down to the ground.
            for y in 1..head[1] {
                assert_eq!(world.name([head[0], y, head[2]]), "twisting_vines_plant");
            }
        }
    }

    #[test]
    fn weeping_vines_hang_under_wart() {
        let mut world = World::new(|y| if y >= 20 { "netherrack" } else { "air" });
        assert!(world.run(&decode(&World::new(|_| "air"), json!({"type": "minecraft:weeping_vines"})), [0, 19, 0]));
        assert_eq!(world.name([0, 19, 0]), "nether_wart_block");
        let heads = world.placed("weeping_vines");
        assert!(!heads.is_empty());
        for head in heads {
            assert!(head[1] < 20);
            let above = world.name([head[0], head[1] + 1, head[2]]);
            assert!(["weeping_vines_plant", "netherrack", "nether_wart_block"].contains(&above.as_str()), "{above}");
        }
    }

    #[test]
    fn desert_well_is_built_on_sand() {
        let mut world = World::new(|y| if y <= 5 { "sand" } else { "air" });
        assert!(world.run(&HardcodedFeature::DesertWell, [3, 6, 3]));
        let o = [3, 5, 3];
        assert_eq!(world.name(o), "water");
        for [dx, dz] in [[0, -1], [1, 0], [0, 1], [-1, 0]] {
            assert_eq!(world.name(off(o, dx, 0, dz)), "water");
        }
        assert_eq!(world.name(off(o, 0, 4, 0)), "sandstone");
        assert_eq!(world.name(off(o, 1, 4, 0)), "sandstone_slab");
        assert_eq!(world.name(off(o, 2, 1, 0)), "sandstone_slab");
        assert_eq!(world.name(off(o, 2, 1, 2)), "sandstone");
        let sus = world.placed("suspicious_sand");
        assert_eq!(sus.len(), 2);
        assert_eq!(world.entities.len(), 2);
        for (p, nbt) in &world.entities {
            assert!(sus.contains(p));
            assert_eq!(nbt.get(&"LootTable".into()), Some(&crate::nbt::Tag::String("minecraft:archaeology/desert_well".into())));
        }
        let mut stone = World::new(|y| if y <= 5 { "netherrack" } else { "air" });
        assert!(!stone.run(&HardcodedFeature::DesertWell, [3, 6, 3]));
        assert!(stone.blocks.is_empty());
    }
}
