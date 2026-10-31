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
