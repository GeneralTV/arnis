//! Target Minecraft Java version support.
//!
//! By default Arnis writes worlds in the latest Java Anvil format
//! (currently 1.21.x — `DataVersion = 3955`, post-1.18 chunk schema,
//! `Y = -64..319`). Some users specifically need worlds that open in
//! **Java 1.16.5** — that version pre-dates the 1.18 chunk format and
//! has a much smaller block palette: anything introduced after 1.16.5
//! (deepslate, mud, tuff, copper, dirt_path, short_grass, tinted glass,
//! quartz_bricks, etc.) is unknown and will either fail to load or be
//! silently turned into invisible / broken blocks.
//!
//! This module provides:
//!
//! - [`TargetVersion`] — the user-selected target.
//! - [`TargetVersion::data_version`] — the integer DataVersion to write
//!   into chunk NBT.
//! - [`TargetVersion::min_y`] / [`TargetVersion::max_y`] — Y clamp for
//!   block placement so we never write outside the target's world height.
//! - [`TargetVersion::supports_modern_chunk_format`] — gate for the
//!   legacy Anvil writer added in a follow-up PR.
//! - [`TargetVersion::map_block_name`] — replacement table that turns
//!   any post-1.16.5 block name into the closest 1.16.5 equivalent.
//!
//! For target == [`TargetVersion::Latest`] every helper is a no-op /
//! identity, so default behaviour is unchanged.

/// Which Minecraft Java release the generated world should target.
///
/// Default is [`TargetVersion::Latest`] — current (1.21.x) format with
/// the full modern block palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetVersion {
    /// Latest supported Java release (1.21.x). No block remapping,
    /// post-1.18 chunk format, world height -64..319.
    #[default]
    Latest,
    /// Java 1.16.5. DataVersion 2586, world height 0..255, blocks
    /// introduced after 1.16.5 are remapped to 1.16.5-compatible
    /// substitutes. The full 1.16.5 chunk schema (`Level/Sections/
    /// Palette/BlockStates/Status/Biomes`) is implemented in a
    /// follow-up PR — see [`Self::supports_modern_chunk_format`].
    Java1_16_5,
}

impl TargetVersion {
    /// Parse a CLI value such as `latest` or `1.16.5`. Accepts a small
    /// number of common aliases. Returns a user-facing error message
    /// (used by clap's `value_parser`) on unknown input.
    pub fn parse_cli(s: &str) -> Result<Self, String> {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "" | "latest" | "current" | "default" | "auto" => Ok(Self::Latest),
            "1.16.5" | "1_16_5" | "java-1.16.5" | "java-1_16_5" | "1.16" => Ok(Self::Java1_16_5),
            other => Err(format!(
                "unknown target Java version `{other}` (supported: `latest`, `1.16.5`)"
            )),
        }
    }

    /// Integer DataVersion stamped into chunk NBT.
    /// 3955 is 1.21.1; 2586 is 1.16.5 (the .5 patch).
    #[inline]
    pub fn data_version(self) -> i32 {
        match self {
            Self::Latest => 3955,
            Self::Java1_16_5 => 2586,
        }
    }

    /// Lowest world Y that the target accepts. Block placements below
    /// this Y are dropped (callers should clamp / skip).
    #[inline]
    pub fn min_y(self) -> i32 {
        match self {
            Self::Latest => -64,
            Self::Java1_16_5 => 0,
        }
    }

    /// Highest world Y that the target accepts (inclusive).
    #[inline]
    pub fn max_y(self) -> i32 {
        match self {
            Self::Latest => 319,
            Self::Java1_16_5 => 255,
        }
    }

    /// True when the target uses the post-1.18 chunk schema (top-level
    /// `sections / yPos / block_states`). False for [`Self::Java1_16_5`],
    /// which needs the legacy `Level / Sections / Palette / BlockStates`
    /// layout written by the legacy writer added in a follow-up PR.
    #[inline]
    #[allow(dead_code)]
    pub fn supports_modern_chunk_format(self) -> bool {
        !matches!(self, Self::Java1_16_5)
    }

    /// Map a Minecraft block name to the closest 1.16.5-compatible
    /// substitute. Returns `name` unchanged when targeting
    /// [`Self::Latest`] or when the name is already valid in 1.16.5.
    ///
    /// The substitution table is intentionally conservative: every
    /// replacement preserves the *visual category* (rock, dirt, glass,
    /// etc.) so that a city or terrain rendered for 1.21 still reads
    /// the same in 1.16.5, just with a slightly smaller block palette.
    pub fn map_block_name(self, name: &str) -> &str {
        if !matches!(self, Self::Java1_16_5) {
            return name;
        }
        legacy_block_substitute(name).unwrap_or(name)
    }
}

/// Closest 1.16.5 substitute for a block introduced after 1.16.5.
/// Returns `None` when `name` is already valid in 1.16.5 (most blocks).
///
/// Substitutions try to preserve the visual category. Rules:
/// - `deepslate*` → `cobblestone` / `stone_bricks` (dark rock layer
///   doesn't exist in 1.16.5; cobblestone is the closest stand-in for
///   the bare deepslate, stone_bricks for the bricks/tile variants).
/// - `tuff` → `andesite`.
/// - `mud*` → `dirt` / `bricks` (1.16.5 has no mud blocks).
/// - `copper*` → `cut_sandstone` / `iron_block` (shiny / orange
///   stand-in; not perfect but readable).
/// - `dirt_path` → `grass_path` (same block, 1.16.5 name).
/// - `short_grass` → `grass` (same block, renamed in 1.20).
/// - `quartz_bricks` → `quartz_block` (same colour family).
/// - `tinted_glass` → `gray_stained_glass` (closest dark-glass in 1.16).
/// - `lightning_rod` → `iron_bars` (decorative metal rod).
/// - Mangrove / cherry / azalea / amethyst / dripstone / candle /
///   moss_block etc. fall back to existing 1.16 equivalents.
fn legacy_block_substitute(name: &str) -> Option<&'static str> {
    // Strip the optional `minecraft:` namespace so callers can pass
    // either `deepslate` or `minecraft:deepslate`. The returned string
    // is unprefixed; caller re-prefixes if needed.
    let key = name.strip_prefix("minecraft:").unwrap_or(name);

    Some(match key {
        // ---------- Deepslate family (1.17+) ----------
        "deepslate" | "cobbled_deepslate" | "infested_deepslate" => "cobblestone",
        "polished_deepslate" => "stone",
        "deepslate_bricks"
        | "cracked_deepslate_bricks"
        | "deepslate_tiles"
        | "cracked_deepslate_tiles" => "stone_bricks",
        "polished_deepslate_slab" => "stone_slab",
        "polished_deepslate_stairs" => "stone_stairs",
        "polished_deepslate_wall" => "cobblestone_wall",
        "deepslate_brick_slab" => "stone_brick_slab",
        "deepslate_brick_stairs" => "stone_brick_stairs",
        "deepslate_brick_wall" => "stone_brick_wall",
        "deepslate_tile_slab" => "stone_brick_slab",
        "deepslate_tile_stairs" => "stone_brick_stairs",
        "deepslate_tile_wall" => "stone_brick_wall",

        // Deepslate ores → 1.16 plain ores (the deepslate prefix is gone).
        "deepslate_iron_ore" => "iron_ore",
        "deepslate_gold_ore" => "gold_ore",
        "deepslate_diamond_ore" => "diamond_ore",
        "deepslate_lapis_ore" => "lapis_ore",
        "deepslate_redstone_ore" => "redstone_ore",
        "deepslate_emerald_ore" => "emerald_ore",
        "deepslate_coal_ore" => "coal_ore",
        "deepslate_copper_ore" => "iron_ore",

        // ---------- Tuff (1.17) ----------
        "tuff" => "andesite",
        "polished_tuff" => "polished_andesite",
        "tuff_slab" => "andesite_slab",
        "tuff_stairs" => "andesite_stairs",
        "tuff_wall" => "andesite_wall",
        "polished_tuff_slab" => "polished_andesite_slab",
        "polished_tuff_stairs" => "polished_andesite_stairs",
        "polished_tuff_wall" => "andesite_wall",
        "tuff_bricks" => "stone_bricks",
        "tuff_brick_slab" => "stone_brick_slab",
        "tuff_brick_stairs" => "stone_brick_stairs",
        "tuff_brick_wall" => "stone_brick_wall",
        "chiseled_tuff" => "chiseled_stone_bricks",
        "chiseled_tuff_bricks" => "chiseled_stone_bricks",

        // ---------- Mud / mangrove (1.19) ----------
        "mud" => "dirt",
        "muddy_mangrove_roots" => "podzol",
        "packed_mud" => "coarse_dirt",
        "mud_bricks" => "bricks",
        "mud_brick_slab" => "brick_slab",
        "mud_brick_stairs" => "brick_stairs",
        "mud_brick_wall" => "brick_wall",
        "mangrove_log" => "oak_log",
        "mangrove_wood" => "oak_wood",
        "mangrove_planks" => "oak_planks",
        "mangrove_leaves" => "oak_leaves",
        "mangrove_propagule" => "oak_sapling",
        "mangrove_roots" => "oak_log",
        "mangrove_slab" => "oak_slab",
        "mangrove_stairs" => "oak_stairs",
        "mangrove_fence" => "oak_fence",
        "mangrove_fence_gate" => "oak_fence_gate",
        "mangrove_door" => "oak_door",
        "mangrove_trapdoor" => "oak_trapdoor",
        "mangrove_button" => "oak_button",
        "mangrove_pressure_plate" => "oak_pressure_plate",
        "mangrove_sign" => "oak_sign",
        "mangrove_wall_sign" => "oak_wall_sign",

        // ---------- Cherry (1.20) ----------
        "cherry_log" => "birch_log",
        "cherry_wood" => "birch_wood",
        "cherry_planks" => "birch_planks",
        "cherry_leaves" => "pink_wool",
        "cherry_sapling" => "birch_sapling",
        "cherry_slab" => "birch_slab",
        "cherry_stairs" => "birch_stairs",
        "cherry_fence" => "birch_fence",
        "cherry_fence_gate" => "birch_fence_gate",
        "cherry_door" => "birch_door",
        "cherry_trapdoor" => "birch_trapdoor",
        "pink_petals" => "pink_tulip",

        // ---------- Bamboo wood (1.20) ----------
        "bamboo_block" => "bamboo",
        "stripped_bamboo_block" => "bamboo",
        "bamboo_planks" => "jungle_planks",
        "bamboo_mosaic" => "jungle_planks",
        "bamboo_slab" => "jungle_slab",
        "bamboo_mosaic_slab" => "jungle_slab",
        "bamboo_stairs" => "jungle_stairs",
        "bamboo_mosaic_stairs" => "jungle_stairs",
        "bamboo_fence" => "jungle_fence",
        "bamboo_fence_gate" => "jungle_fence_gate",
        "bamboo_door" => "jungle_door",
        "bamboo_trapdoor" => "jungle_trapdoor",

        // ---------- Pale Oak (1.21) ----------
        "pale_oak_log" => "birch_log",
        "pale_oak_wood" => "birch_wood",
        "pale_oak_planks" => "birch_planks",
        "pale_oak_leaves" => "birch_leaves",
        "pale_oak_sapling" => "birch_sapling",
        "pale_oak_slab" => "birch_slab",
        "pale_oak_stairs" => "birch_stairs",
        "pale_oak_fence" => "birch_fence",
        "pale_oak_door" => "birch_door",
        "pale_oak_trapdoor" => "birch_trapdoor",
        "pale_moss_block" => "moss_block",
        "pale_moss_carpet" => "moss_carpet",
        "pale_hanging_moss" => "vine",

        // ---------- Copper (1.17) ----------
        "copper_block" | "exposed_copper" | "weathered_copper" | "oxidized_copper" => "iron_block",
        "waxed_copper_block"
        | "waxed_exposed_copper"
        | "waxed_weathered_copper"
        | "waxed_oxidized_copper" => "iron_block",
        "cut_copper" | "exposed_cut_copper" | "weathered_cut_copper" | "oxidized_cut_copper" => {
            "cut_sandstone"
        }
        "waxed_cut_copper"
        | "waxed_exposed_cut_copper"
        | "waxed_weathered_cut_copper"
        | "waxed_oxidized_cut_copper" => "cut_sandstone",
        "copper_ore" => "iron_ore",
        "raw_copper_block" => "raw_iron_block",
        "lightning_rod" => "iron_bars",
        "copper_grate"
        | "exposed_copper_grate"
        | "weathered_copper_grate"
        | "oxidized_copper_grate" => "iron_bars",
        "copper_bulb"
        | "exposed_copper_bulb"
        | "weathered_copper_bulb"
        | "oxidized_copper_bulb" => "redstone_lamp",
        "copper_door" => "oak_door",
        "copper_trapdoor" => "oak_trapdoor",

        // ---------- Glass / decorative ----------
        "tinted_glass" => "gray_stained_glass",

        // ---------- Misc 1.17+ blocks ----------
        "amethyst_block" | "budding_amethyst" => "purpur_block",
        "amethyst_cluster"
        | "small_amethyst_bud"
        | "medium_amethyst_bud"
        | "large_amethyst_bud" => "end_rod",
        "calcite" => "diorite",
        "smooth_basalt" => "basalt",
        "rooted_dirt" => "dirt",
        "moss_block" => "grass_block",
        "moss_carpet" => "vine",
        "azalea" => "oak_sapling",
        "flowering_azalea" => "oak_sapling",
        "azalea_leaves" => "oak_leaves",
        "flowering_azalea_leaves" => "oak_leaves",
        "spore_blossom" => "vine",
        "glow_lichen" => "vine",
        "cave_vines" | "cave_vines_plant" => "vine",
        "small_dripleaf" | "big_dripleaf" | "big_dripleaf_stem" => "lily_pad",
        "hanging_roots" => "vine",
        "pointed_dripstone" => "stone",
        "dripstone_block" => "smooth_stone",
        "powder_snow" => "snow_block",
        "powder_snow_cauldron" => "cauldron",
        "water_cauldron" => "cauldron",
        "lava_cauldron" => "cauldron",

        // ---------- Renamed in 1.17/1.20 ----------
        "dirt_path" => "grass_path",
        "short_grass" => "grass",
        "quartz_bricks" => "quartz_block",

        // ---------- Trial (1.21) ----------
        "trial_spawner" | "vault" => "spawner",
        "chiseled_bookshelf" => "bookshelf",
        "decorated_pot" => "flower_pot",
        "candle" => "torch",
        "white_candle" | "orange_candle" | "magenta_candle" | "light_blue_candle"
        | "yellow_candle" | "lime_candle" | "pink_candle" | "gray_candle" | "light_gray_candle"
        | "cyan_candle" | "purple_candle" | "blue_candle" | "brown_candle" | "green_candle"
        | "red_candle" | "black_candle" => "torch",

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases() {
        assert_eq!(
            TargetVersion::parse_cli("latest").unwrap(),
            TargetVersion::Latest
        );
        assert_eq!(
            TargetVersion::parse_cli("default").unwrap(),
            TargetVersion::Latest
        );
        assert_eq!(
            TargetVersion::parse_cli("1.16.5").unwrap(),
            TargetVersion::Java1_16_5
        );
        assert_eq!(
            TargetVersion::parse_cli("java-1.16.5").unwrap(),
            TargetVersion::Java1_16_5
        );
        assert_eq!(
            TargetVersion::parse_cli("1_16_5").unwrap(),
            TargetVersion::Java1_16_5
        );
        assert!(TargetVersion::parse_cli("1.20").is_err());
    }

    #[test]
    fn data_version_and_height() {
        assert_eq!(TargetVersion::Latest.data_version(), 3955);
        assert_eq!(TargetVersion::Java1_16_5.data_version(), 2586);
        assert_eq!(TargetVersion::Latest.min_y(), -64);
        assert_eq!(TargetVersion::Latest.max_y(), 319);
        assert_eq!(TargetVersion::Java1_16_5.min_y(), 0);
        assert_eq!(TargetVersion::Java1_16_5.max_y(), 255);
    }

    #[test]
    fn modern_format_only_for_latest() {
        assert!(TargetVersion::Latest.supports_modern_chunk_format());
        assert!(!TargetVersion::Java1_16_5.supports_modern_chunk_format());
    }

    #[test]
    fn no_remap_for_latest() {
        let t = TargetVersion::Latest;
        for n in ["deepslate", "tuff", "mud_bricks", "tinted_glass", "stone"] {
            assert_eq!(t.map_block_name(n), n);
        }
    }

    #[test]
    fn remap_post_1_16_blocks() {
        let t = TargetVersion::Java1_16_5;
        assert_eq!(t.map_block_name("deepslate"), "cobblestone");
        assert_eq!(t.map_block_name("polished_deepslate"), "stone");
        assert_eq!(t.map_block_name("deepslate_bricks"), "stone_bricks");
        assert_eq!(t.map_block_name("tuff"), "andesite");
        assert_eq!(t.map_block_name("mud_bricks"), "bricks");
        assert_eq!(t.map_block_name("dirt_path"), "grass_path");
        assert_eq!(t.map_block_name("short_grass"), "grass");
        assert_eq!(t.map_block_name("tinted_glass"), "gray_stained_glass");
        assert_eq!(t.map_block_name("quartz_bricks"), "quartz_block");
        assert_eq!(t.map_block_name("lightning_rod"), "iron_bars");
        assert_eq!(t.map_block_name("copper_block"), "iron_block");
        assert_eq!(t.map_block_name("calcite"), "diorite");
        assert_eq!(t.map_block_name("amethyst_block"), "purpur_block");
        assert_eq!(t.map_block_name("moss_block"), "grass_block");
    }

    #[test]
    fn remap_accepts_namespaced_input() {
        let t = TargetVersion::Java1_16_5;
        assert_eq!(t.map_block_name("minecraft:deepslate"), "cobblestone");
        assert_eq!(t.map_block_name("minecraft:tuff"), "andesite");
    }

    #[test]
    fn unknown_blocks_pass_through() {
        let t = TargetVersion::Java1_16_5;
        // Already valid in 1.16.5 → unchanged.
        assert_eq!(t.map_block_name("stone"), "stone");
        assert_eq!(t.map_block_name("oak_planks"), "oak_planks");
        assert_eq!(t.map_block_name("bricks"), "bricks");
    }
}
