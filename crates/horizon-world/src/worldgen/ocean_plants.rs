//! 1.21.11's seagrass, kelp and sea_pickle features (26.1 writes them as data):
//! `SeagrassFeature`, `KelpFeature` and `SeaPickleFeature`, with vanilla's draws in order.
use super::feature::FeatureWorld;
use super::placement::{PlacementContext, PlacementQueryError};
use super::value_provider::IntProvider;
use crate::chunk::BlockState;
use crate::heightmap::HeightmapType;
use crate::registry::{BlockRegistry, BlockStateId};
use horizon_core::random::{InvalidBound, RandomSource};

#[derive(Debug)]
pub enum OceanPlantError<E> {
    Random(InvalidBound),
    World(E),
    Query(PlacementQueryError),
}
impl<E: std::fmt::Display> std::fmt::Display for OceanPlantError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Random(e) => e.fmt(f),
            Self::World(e) => e.fmt(f),
            Self::Query(e) => e.fmt(f),
        }
    }
}
impl<E> From<InvalidBound> for OceanPlantError<E> {
    fn from(e: InvalidBound) -> Self {
        Self::Random(e)
    }
}
impl<E> From<PlacementQueryError> for OceanPlantError<E> {
    fn from(e: PlacementQueryError) -> Self {
        Self::Query(e)
    }
}

#[derive(Clone, Debug)]
pub struct OceanBlocks {
    water: BlockStateId,
    seagrass: BlockStateId,
    tall_lower: BlockStateId,
    tall_upper: BlockStateId,
    kelp_plant: BlockStateId,
    /// Kelp at ages 20..=23.
    kelp: [BlockStateId; 4],
    /// Sea pickles 1..=4, waterlogged (its default).
    pickles: [BlockStateId; 4],
}
impl OceanBlocks {
    pub fn new(registry: &BlockRegistry) -> std::io::Result<Self> {
        let get = |name: &str, props: &[(&str, &str)]| {
            registry.resolve(&BlockState {
                name: name.into(),
                properties: props.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
            })
        };
        Ok(Self {
            water: get("minecraft:water", &[])?,
            seagrass: get("minecraft:seagrass", &[])?,
            tall_lower: get("minecraft:tall_seagrass", &[("half", "lower")])?,
            tall_upper: get("minecraft:tall_seagrass", &[("half", "upper")])?,
            kelp_plant: get("minecraft:kelp_plant", &[])?,
            kelp: [
                get("minecraft:kelp", &[("age", "20")])?,
                get("minecraft:kelp", &[("age", "21")])?,
                get("minecraft:kelp", &[("age", "22")])?,
                get("minecraft:kelp", &[("age", "23")])?,
            ],
            pickles: [
                get("minecraft:sea_pickle", &[("pickles", "1")])?,
                get("minecraft:sea_pickle", &[("pickles", "2")])?,
                get("minecraft:sea_pickle", &[("pickles", "3")])?,
                get("minecraft:sea_pickle", &[("pickles", "4")])?,
            ],
        })
    }
}


#[derive(Clone, Debug)]
pub enum OceanPlant {
    Seagrass { probability: f64 },
    Kelp,
    SeaPickle { count: IntProvider },
}

impl OceanPlant {
    pub fn place<W: FeatureWorld + PlacementContext>(
        &self,
        blocks: &OceanBlocks,
        world: &mut W,
        random: &mut impl RandomSource,
        origin: [i32; 3],
    ) -> Result<bool, OceanPlantError<W::Error>> {
        match self {
            Self::Seagrass { probability } => {
                let dx = random.next_int_bound(8)? - random.next_int_bound(8)?;
                let dz = random.next_int_bound(8)? - random.next_int_bound(8)?;
                let (x, z) = (origin[0] + dx, origin[2] + dz);
                let y = world.height(HeightmapType::OceanFloor, x, z)?;
                let at = [x, y, z];
                if block(world, at)? != blocks.water {
                    return Ok(false);
                }
                let tall = random.next_double() < *probability;
                let state = if tall { blocks.tall_lower } else { blocks.seagrass };
                if !world.block_survives(state, at)? {
                    return Ok(false);
                }
                if tall {
                    let above = [x, y + 1, z];
                    if block(world, above)? == blocks.water {
                        set(world, at, state)?;
                        set(world, above, blocks.tall_upper)?;
                    }
                } else {
                    set(world, at, state)?;
                }
                Ok(true)
            }
            Self::Kelp => {
                let y = world.height(HeightmapType::OceanFloor, origin[0], origin[2])?;
                let mut at = [origin[0], y, origin[2]];
                if block(world, at)? != blocks.water {
                    return Ok(false);
                }
                let mut placed = 0;
                let height = 1 + random.next_int_bound(10)?;
                for i in 0..=height {
                    let above = [at[0], at[1] + 1, at[2]];
                    if block(world, at)? == blocks.water
                        && block(world, above)? == blocks.water
                        && world.block_survives(blocks.kelp_plant, at)?
                    {
                        if i == height {
                            let age = random.next_int_bound(4)? as usize;
                            set(world, at, blocks.kelp[age])?;
                            placed += 1;
                        } else {
                            set(world, at, blocks.kelp_plant)?;
                        }
                    } else if i > 0 {
                        let below = [at[0], at[1] - 1, at[2]];
                        let under = [at[0], at[1] - 2, at[2]];
                        if world.block_survives(blocks.kelp[0], below)?
                            && !blocks.kelp.contains(&block(world, under)?)
                        {
                            let age = random.next_int_bound(4)? as usize;
                            set(world, below, blocks.kelp[age])?;
                            placed += 1;
                        }
                        break;
                    }
                    at = above;
                }
                Ok(placed > 0)
            }
            Self::SeaPickle { count } => {
                let mut placed = 0;
                for _ in 0..count.sample(random)? {
                    let dx = random.next_int_bound(8)? - random.next_int_bound(8)?;
                    let dz = random.next_int_bound(8)? - random.next_int_bound(8)?;
                    let (x, z) = (origin[0] + dx, origin[2] + dz);
                    let y = world.height(HeightmapType::OceanFloor, x, z)?;
                    let at = [x, y, z];
                    let state = blocks.pickles[random.next_int_bound(4)? as usize];
                    if block(world, at)? == blocks.water && world.block_survives(state, at)? {
                        set(world, at, state)?;
                        placed += 1;
                    }
                }
                Ok(placed > 0)
            }
        }
    }
}

fn block<W: FeatureWorld>(world: &mut W, at: [i32; 3]) -> Result<BlockStateId, OceanPlantError<W::Error>> {
    world.block(at).map_err(OceanPlantError::World)
}
fn set<W: FeatureWorld>(world: &mut W, at: [i32; 3], state: BlockStateId) -> Result<(), OceanPlantError<W::Error>> {
    world.set_block(at, state, 2).map(|_| ()).map_err(OceanPlantError::World)
}

/// 1.21.11's coral_tree, coral_claw and coral_mushroom (CoralFeature and its three shapes):
/// a random `#coral_blocks` block, each placed with `placeCoralBlock`'s decorations.
#[derive(Clone, Copy, Debug)]
pub enum CoralShape {
    Tree,
    Claw,
    Mushroom,
}

/// The coral tags in their tag files' order (`getRandomElementOf` draws by index).
#[derive(Clone, Debug)]
pub struct CoralBlocks {
    water: BlockStateId,
    blocks: Vec<BlockStateId>,
    corals: Vec<BlockStateId>,
    /// Wall fans, then their states facing north, east, south and west.
    wall: Vec<[BlockStateId; 4]>,
    pickles: [BlockStateId; 4],
    /// Every state of a `#corals` block (any waterlogged value).
    coral_set: std::collections::BTreeSet<BlockStateId>,
    }
impl CoralBlocks {
    pub fn new(registry: &BlockRegistry) -> std::io::Result<Self> {
        let get = |name: &str, props: &[(&str, &str)]| {
            registry.resolve(&BlockState {
                name: name.into(),
                properties: props.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
            })
        };
        let kinds = ["tube", "brain", "bubble", "fire", "horn"];
        let blocks = kinds.iter().map(|k| get(&format!("minecraft:{k}_coral_block"), &[])).collect::<Result<Vec<_>, _>>()?;
        // #corals = #coral_plants then the fans.
        let mut corals = kinds.iter().map(|k| get(&format!("minecraft:{k}_coral"), &[])).collect::<Result<Vec<_>, _>>()?;
        corals.extend(kinds.iter().map(|k| get(&format!("minecraft:{k}_coral_fan"), &[])).collect::<Result<Vec<_>, _>>()?);
        let wall = kinds
            .iter()
            .map(|k| {
                let name = format!("minecraft:{k}_coral_wall_fan");
                Ok([get(&name, &[("facing", "north")])?, get(&name, &[("facing", "east")])?,
                    get(&name, &[("facing", "south")])?, get(&name, &[("facing", "west")])?])
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        let names: std::collections::BTreeSet<&str> =
            corals.iter().filter_map(|c| registry.state(*c)).map(|s| s.name.as_str()).collect();
        let coral_set = (0..registry.len() as u32)
            .map(BlockStateId)
            .filter(|id| registry.state(*id).is_some_and(|s| names.contains(s.name.as_str())))
            .collect();
        Ok(Self {
            water: get("minecraft:water", &[])?,
            blocks,
            corals,
            wall,
            pickles: [
                get("minecraft:sea_pickle", &[("pickles", "1")])?,
                get("minecraft:sea_pickle", &[("pickles", "2")])?,
                get("minecraft:sea_pickle", &[("pickles", "3")])?,
                get("minecraft:sea_pickle", &[("pickles", "4")])?,
            ],
            coral_set,
        })
    }
}

const HORIZONTAL: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
fn step(p: [i32; 3], d: usize) -> [i32; 3] {
    [p[0] + HORIZONTAL[d][0], p[1], p[2] + HORIZONTAL[d][1]]
}
/// Direction indices: 0..4 the horizontal ones (north, east, south, west), 4 up.
fn go(p: [i32; 3], d: usize, n: i32) -> [i32; 3] {
    if d == 4 {
        [p[0], p[1] + n, p[2]]
    } else {
        [p[0] + HORIZONTAL[d][0] * n, p[1], p[2] + HORIZONTAL[d][1] * n]
    }
}

impl CoralShape {
    pub fn place<W: FeatureWorld>(
        self,
        c: &CoralBlocks,
        world: &mut W,
        random: &mut impl RandomSource,
        origin: [i32; 3],
    ) -> Result<bool, OceanPlantError<W::Error>> {
        let state = c.blocks[random.next_int_bound(c.blocks.len() as i32)? as usize];
        let mut place = |world: &mut W, random: &mut _, at| coral_block(c, world, random, at, state);
        match self {
            Self::Tree => {
                let mut pos = origin;
                for _ in 0..random.next_int_bound(3)? + 1 {
                    if !place(world, random, pos)? {
                        return Ok(true);
                    }
                    pos[1] += 1;
                }
                let top = pos;
                let branches = random.next_int_bound(3)? + 2;
                let mut dirs = [0usize, 1, 2, 3];
                super::tree_blocks::shuffle(&mut dirs, random)?;
                for &d in &dirs[..branches as usize] {
                    let mut pos = step(top, d);
                    let height = random.next_int_bound(5)? + 2;
                    let mut segment = 0;
                    let mut i = 0;
                    while i < height && place(world, random, pos)? {
                        segment += 1;
                        pos[1] += 1;
                        if i == 0 || segment >= 2 && random.next_float() < 0.25 {
                            pos = step(pos, d);
                            segment = 0;
                        }
                        i += 1;
                    }
                }
                Ok(true)
            }
            Self::Claw => {
                if !place(world, random, origin)? {
                    return Ok(false);
                }
                let claw = random.next_int_bound(4)? as usize;
                let branches = random.next_int_bound(2)? + 2;
                let mut dirs = [claw, (claw + 1) % 4, (claw + 3) % 4];
                super::tree_blocks::shuffle(&mut dirs, random)?;
                for &d in &dirs[..branches as usize] {
                    let mut pos = step(origin, d);
                    let sideways = random.next_int_bound(2)? + 1;
                    let (segment, inway) = if d == claw {
                        (claw, random.next_int_bound(3)? + 2)
                    } else {
                        pos[1] += 1;
                        let options = [d, 4];
                        (options[random.next_int_bound(2)? as usize], random.next_int_bound(3)? + 3)
                    };
                    let mut i = 0;
                    while i < sideways && place(world, random, pos)? {
                        pos = go(pos, segment, 1);
                        i += 1;
                    }
                    pos = go(pos, segment, -1);
                    pos[1] += 1;
                    for _ in 0..inway {
                        pos = step(pos, claw);
                        if !place(world, random, pos)? {
                            break;
                        }
                        if random.next_float() < 0.25 {
                            pos[1] += 1;
                        }
                    }
                }
                Ok(true)
            }
            Self::Mushroom => {
                let y_size = random.next_int_bound(3)? + 3;
                let x_size = random.next_int_bound(3)? + 3;
                let z_size = random.next_int_bound(3)? + 3;
                let down = random.next_int_bound(3)? + 1;
                for x in 0..=x_size {
                    for y in 0..=y_size {
                        for z in 0..=z_size {
                            let edge = |v: i32, max: i32| v == 0 || v == max;
                            let (ex, ey, ez) = (edge(x, x_size), edge(y, y_size), edge(z, z_size));
                            if !(ex && ey) && !(ez && ey) && !(ex && ez) && (ex || ey || ez) && random.next_float() >= 0.1 {
                                let at = [origin[0] + x, origin[1] + y - down, origin[2] + z];
                                place(world, random, at)?;
                            }
                        }
                    }
                }
                Ok(true)
            }
        }
    }
}

/// CoralFeature.placeCoralBlock.
fn coral_block<W: FeatureWorld>(
    c: &CoralBlocks,
    world: &mut W,
    random: &mut impl RandomSource,
    at: [i32; 3],
    state: BlockStateId,
) -> Result<bool, OceanPlantError<W::Error>> {
    let above = [at[0], at[1] + 1, at[2]];
    let here = block(world, at)?;
    if !((here == c.water || c.coral_set.contains(&here)) && block(world, above)? == c.water) {
        return Ok(false);
    }
    world.set_block(at, state, 3).map_err(OceanPlantError::World)?;
    if random.next_float() < 0.25 {
        let coral = c.corals[random.next_int_bound(c.corals.len() as i32)? as usize];
        set(world, above, coral)?;
    } else if random.next_float() < 0.05 {
        set(world, above, c.pickles[random.next_int_bound(4)? as usize])?;
    }
    for d in 0..4 {
        if random.next_float() < 0.2 {
            let side = step(at, d);
            if block(world, side)? == c.water {
                let fan = &c.wall[random.next_int_bound(c.wall.len() as i32)? as usize];
                set(world, side, fan[d])?;
            }
        }
    }
    Ok(true)
}
