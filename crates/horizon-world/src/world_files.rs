//! Where 1.21.11 keeps a world's files (`LevelResource`, `DimensionType.getStorageFolder`,
//! `DimensionDataStorage`): the overworld in the world folder, the Nether in `DIM-1`, the End in
//! `DIM1`, other dimensions in `dimensions/<namespace>/<path>`; saved data in each dimension
//! folder's `data/<id>.dat`; players in `playerdata`, `advancements` and `stats`.
use std::path::{Path, PathBuf};

/// `LevelResource.PLAYER_DATA_DIR`: `<uuid>.dat`.
pub const PLAYER_DATA: &str = "playerdata";
/// `LevelResource.PLAYER_ADVANCEMENTS_DIR`: `<uuid>.json`.
pub const ADVANCEMENTS: &str = "advancements";
/// `LevelResource.PLAYER_STATS_DIR`: `<uuid>.json`.
pub const STATS: &str = "stats";

/// `DimensionType.getStorageFolder`: the folder of a dimension (`minecraft:the_nether` or
/// `the_nether`) holding its `region`, `entities`, `poi` and `data`.
pub fn dimension_dir(world: &Path, dimension: &str) -> PathBuf {
    let (namespace, path) = dimension
        .split_once(':')
        .unwrap_or(("minecraft", dimension));
    match (namespace, path) {
        ("minecraft", "overworld") => world.to_owned(),
        ("minecraft", "the_nether") => world.join("DIM-1"),
        ("minecraft", "the_end") => world.join("DIM1"),
        _ => world.join("dimensions").join(namespace).join(path),
    }
}

/// `DimensionDataStorage.getDataFile`: the saved data `id` (`SavedDataType` name, such as
/// `raids` or `map_3`) of the dimension whose folder is `dimension_dir`.
pub fn saved_data(dimension_dir: &Path, id: &str) -> PathBuf {
    dimension_dir.join("data").join(format!("{id}.dat"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_and_saved_data_sit_where_1_21_11_keeps_them() {
        let w = Path::new("w");
        assert_eq!(dimension_dir(w, "minecraft:overworld"), Path::new("w"));
        assert_eq!(dimension_dir(w, "the_nether"), Path::new("w/DIM-1"));
        assert_eq!(dimension_dir(w, "minecraft:the_end"), Path::new("w/DIM1"));
        assert_eq!(
            dimension_dir(w, "mod:sky"),
            Path::new("w/dimensions/mod/sky")
        );
        assert_eq!(saved_data(w, "raids"), Path::new("w/data/raids.dat"));
        assert_eq!(
            saved_data(&dimension_dir(w, "minecraft:the_end"), "raids_end"),
            Path::new("w/DIM1/data/raids_end.dat")
        );
    }
}
