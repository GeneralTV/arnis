/// Generates the coordinates for a line between two points using the Bresenham algorithm.
/// The result is a vector of 3D coordinates (x, y, z).
pub fn bresenham_line(
    x1: i32,
    y1: i32,
    z1: i32,
    x2: i32,
    y2: i32,
    z2: i32,
) -> Vec<(i32, i32, i32)> {
    // Calculate max possible points needed
    let dx = if x2 > x1 { x2 - x1 } else { x1 - x2 };
    let dy = if y2 > y1 { y2 - y1 } else { y1 - y2 };
    let dz = if z2 > z1 { z2 - z1 } else { z1 - z2 };

    // Pre-allocate vector with exact size needed
    let capacity = dx.max(dy).max(dz) + 1;
    let mut points = Vec::with_capacity(capacity as usize);
    points.reserve_exact(capacity as usize);

    let xs = if x1 < x2 { 1 } else { -1 };
    let ys = if y1 < y2 { 1 } else { -1 };
    let zs = if z1 < z2 { 1 } else { -1 };

    let mut x = x1;
    let mut y = y1;
    let mut z = z1;

    // Determine dominant axis once, outside the loop
    if dx >= dy && dx >= dz {
        let mut p1 = 2 * dy - dx;
        let mut p2 = 2 * dz - dx;

        while x != x2 {
            points.push((x, y, z));

            if p1 >= 0 {
                y += ys;
                p1 -= 2 * dx;
            }
            if p2 >= 0 {
                z += zs;
                p2 -= 2 * dx;
            }
            p1 += 2 * dy;
            p2 += 2 * dz;
            x += xs;
        }
    } else if dy >= dx && dy >= dz {
        let mut p1 = 2 * dx - dy;
        let mut p2 = 2 * dz - dy;

        while y != y2 {
            points.push((x, y, z));

            if p1 >= 0 {
                x += xs;
                p1 -= 2 * dy;
            }
            if p2 >= 0 {
                z += zs;
                p2 -= 2 * dy;
            }
            p1 += 2 * dx;
            p2 += 2 * dz;
            y += ys;
        }
    } else {
        let mut p1 = 2 * dy - dz;
        let mut p2 = 2 * dx - dz;

        while z != z2 {
            points.push((x, y, z));

            if p1 >= 0 {
                y += ys;
                p1 -= 2 * dz;
            }
            if p2 >= 0 {
                x += xs;
                p2 -= 2 * dz;
            }
            p1 += 2 * dy;
            p2 += 2 * dx;
            z += zs;
        }
    }

    points.push((x2, y2, z2));
    points
}

/// Generates a 4-connected (orthogonal) XZ line between two points.
///
/// Standard `bresenham_line` is 8-connected: a diagonal step produces
/// cells that are corner-adjacent (`(x, z)` and `(x+1, z+1)`) rather
/// than edge-adjacent. For Minecraft block kinds whose model relies on
/// orthogonal-neighbour state — fences, walls, rails, glass panes — a
/// corner-adjacent step leaves a one-block gap that the player can walk
/// through and that breaks the auto-connecting fence/wall geometry.
///
/// This helper inserts a single corner cell on every diagonal step so
/// the result is always 4-connected: each consecutive pair shares a
/// face. Y is dropped because callers placing fences / rails always
/// derive the Y from the local terrain at each `(x, z)`.
pub fn bresenham_line_4_connected(x1: i32, z1: i32, x2: i32, z2: i32) -> Vec<(i32, i32)> {
    let raw = bresenham_line(x1, 0, z1, x2, 0, z2);
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(raw.len() * 2);
    let mut prev: Option<(i32, i32)> = None;
    for (bx, _, bz) in raw {
        if let Some((px, pz)) = prev {
            if bx != px && bz != pz {
                // Diagonal step: bridge with the corner cell that keeps
                // the fence/wall on its dominant axis (prefer the one
                // sharing the previous Z so a long X-major run reads as
                // a clean horizontal then a single vertical step).
                out.push((bx, pz));
            }
        }
        out.push((bx, bz));
        prev = Some((bx, bz));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_4_connected(points: &[(i32, i32)]) -> bool {
        points
            .windows(2)
            .all(|w| (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs() == 1)
    }

    #[test]
    fn line_4_connected_single_step() {
        let pts = bresenham_line_4_connected(0, 0, 1, 0);
        assert_eq!(pts, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn line_4_connected_diagonal() {
        let pts = bresenham_line_4_connected(0, 0, 3, 3);
        assert!(is_4_connected(&pts));
        assert_eq!(pts.first(), Some(&(0, 0)));
        assert_eq!(pts.last(), Some(&(3, 3)));
    }

    #[test]
    fn line_4_connected_x_major() {
        let pts = bresenham_line_4_connected(0, 0, 10, 3);
        assert!(is_4_connected(&pts));
    }

    #[test]
    fn line_4_connected_z_major() {
        let pts = bresenham_line_4_connected(0, 0, 3, 10);
        assert!(is_4_connected(&pts));
    }

    #[test]
    fn line_4_connected_negative_direction() {
        let pts = bresenham_line_4_connected(5, 5, 0, 0);
        assert!(is_4_connected(&pts));
        assert_eq!(pts.first(), Some(&(5, 5)));
        assert_eq!(pts.last(), Some(&(0, 0)));
    }
}
