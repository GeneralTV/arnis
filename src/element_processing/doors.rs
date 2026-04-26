use crate::block_definitions::*;
use crate::osm_parser::{ProcessedElement, ProcessedNode};
use crate::world_editor::WorldEditor;
use std::collections::{HashMap, HashSet};

/// A door / entrance node that may belong to a building.
///
/// Building generation looks these up by `(x, z)` and, when a coordinate
/// falls on a building wall, places the door at the correct floor level
/// (with terrain and `start_y_offset` taken into account) — instead of the
/// fallback ground-level door produced by [`generate_doors`].
#[derive(Debug, Clone)]
pub struct BuildingEntrance {
    pub node_id: u64,
    /// True when the node carries `entrance=main`/`primary`/`yes` etc.
    /// Used by building generation to pick a heavier door material for
    /// the front entrance.
    pub is_main_entrance: bool,
}

/// Pre-pass: scan all processed nodes for `door=*` / `entrance=*` and
/// build an `(x, z) -> BuildingEntrance` lookup table. The table is used
/// by building generation to place doors on the correct floor level and
/// by [`generate_doors`] (via `consumed_entrances`) to avoid double-placing
/// fallback ground-level doors over the same node.
pub fn collect_building_entrances(
    elements: &[ProcessedElement],
) -> HashMap<(i32, i32), BuildingEntrance> {
    let mut map = HashMap::new();
    for el in elements {
        if let ProcessedElement::Node(node) = el {
            let has_door = node.tags.contains_key("door");
            let has_entrance = node.tags.contains_key("entrance");
            if !has_door && !has_entrance {
                continue;
            }

            // Skip non-ground-level entrances — those are part of an
            // upper-floor building:part and shouldn't pierce the ground floor.
            let level = node
                .tags
                .get("level")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            if level != 0 {
                continue;
            }

            let is_main_entrance = node
                .tags
                .get("entrance")
                .map(|v| {
                    matches!(
                        v.as_str(),
                        "main" | "primary" | "yes" | "home" | "shop" | "service"
                    )
                })
                .unwrap_or(false);

            let _ = level; // kept above for the early-return guard
            map.insert(
                (node.x, node.z),
                BuildingEntrance {
                    node_id: node.id,
                    is_main_entrance,
                },
            );
        }
    }
    map
}

/// Fallback ground-level door placement.
///
/// Building generation places entrance doors directly on the wall (at the
/// correct floor level) when a `door=*` / `entrance=*` node coincides with
/// a building wall coordinate. For nodes that don't fall on any building's
/// wall — orphaned entrance markers, doors on amenities, etc. — this
/// function still drops a basic dark-oak door at `y=1` so the marker is
/// not lost. Skip nodes already consumed by a building.
pub fn generate_doors(
    editor: &mut WorldEditor,
    element: &ProcessedNode,
    consumed_entrances: &HashSet<u64>,
) {
    if consumed_entrances.contains(&element.id) {
        return;
    }

    // Check if the element is a door or entrance
    if element.tags.contains_key("door") || element.tags.contains_key("entrance") {
        // Check for the "level" tag and skip doors that are not at ground level
        if let Some(level_str) = element.tags.get("level") {
            if let Ok(level) = level_str.parse::<i32>() {
                if level != 0 {
                    return; // Skip doors not on ground level
                }
            }
        }

        let x: i32 = element.x;
        let z: i32 = element.z;

        // Set the ground block and the door blocks
        editor.set_block(GRAY_CONCRETE, x, 0, z, None, None);
        editor.set_block(DARK_OAK_DOOR_LOWER, x, 1, z, None, None);
        editor.set_block(DARK_OAK_DOOR_UPPER, x, 2, z, None, None);
    }
}
