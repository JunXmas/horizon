package dev.horizonlab.parity;

import java.util.Map;

/** 1.21.11 block classes under the names 26.1 gave them, so exporters write the names Horizon reads. */
final class LegacyNames {
    private static final Map<String, String> RENAMED = Map.of(
        "FarmBlock", "FarmlandBlock",
        "WaterlilyBlock", "LilyPadBlock",
        "FungusBlock", "NetherFungusBlock",
        "RootsBlock", "NetherRootsBlock",
        "DirtPathBlock", "PathBlock",
        "RedStoneWireBlock", "RedstoneWireBlock",
        "SnowyDirtBlock", "SnowyBlock",
        "SpreadingSnowyDirtBlock", "SpreadingSnowyBlock");
        /** Overrides that only exist to halve the fall, which 26.1 does with Block.fallOn and the block's
            fallDistanceReduction (exported by VanillaMovementRegistryOracle). */
        private static final java.util.Set<String> PLAIN_FALL = java.util.Set.of("BedBlock");

        /** The class owning `fallOn` as Horizon's landing families name it. */
        static String fallOwner(Class<?> type) {
            return PLAIN_FALL.contains(type.getSimpleName()) ? "Block" : name(type);
        }

    static String name(Class<?> type) {
        return RENAMED.getOrDefault(type.getSimpleName(), type.getSimpleName());
    }

    /** Parents 26.1 inserted above a 1.21.11 class: BedBlock gained AbstractBedBlock (beds and the
        straw bed), PointedDripstoneBlock gained SpeleothemBlock. Horizon's behaviour hangs on them. */
    private static final Map<String, String> INSERTED_PARENT = Map.of(
        "BedBlock", "AbstractBedBlock",
        "PointedDripstoneBlock", "SpeleothemBlock");

    static String insertedParent(Class<?> type) {
        return INSERTED_PARENT.get(type.getSimpleName());
    }

    private LegacyNames() {}
}
