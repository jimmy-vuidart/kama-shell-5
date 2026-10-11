//! Maths of the rounded-corner clip.
//!
//! The clip shader works from `gl_FragCoord`, so it needs to turn framebuffer
//! pixels back into pixels of the clipped rectangle, whatever the output
//! transform or the orientation of the render target.

/// Rows `(x, y)` of the affine map taking `(gl_FragCoord.xy, 1)` to pixels
/// relative to `origin`, a point in physical output coordinates.
///
/// `projection` is the renderer's column-major 3×3 matrix from physical
/// output coordinates to clip space; `viewport` is the target's size.
pub fn frag_to_local(
    projection: &[f32; 9],
    viewport: [f32; 2],
    origin: [f32; 2],
) -> ([f32; 3], [f32; 3]) {
    // Fragment to clip space: n = f * 2 / viewport - 1.
    let (sx, sy) = (2.0 / viewport[0], 2.0 / viewport[1]);

    // Clip space back to output pixels: invert the affine projection
    // [a c e; b d f], which is never singular for a real output.
    let [a, b, _, c, d, _, e, f, _] = *projection;
    let det = a * d - b * c;
    let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
    let (ie, i_f) = (-(ia * e + ic * f), -(ib * e + id * f));

    // Compose both, then move the origin.
    (
        [ia * sx, ic * sy, ie - ia - ic - origin[0]],
        [ib * sx, id * sy, i_f - ib - id - origin[1]],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 1920.0;
    const H: f32 = 1080.0;

    /// Output drawn upright: clip-space `y` grows upwards, so it is flipped.
    const NORMAL: [f32; 9] = [2.0 / W, 0.0, 0.0, 0.0, -2.0 / H, 0.0, -1.0, 1.0, 1.0];
    /// What the winit backend uses: the output is flipped vertically.
    const FLIPPED: [f32; 9] = [2.0 / W, 0.0, 0.0, 0.0, 2.0 / H, 0.0, -1.0, -1.0, 1.0];

    fn apply(rows: ([f32; 3], [f32; 3]), frag: [f32; 2]) -> [f32; 2] {
        let dot = |row: [f32; 3]| row[0] * frag[0] + row[1] * frag[1] + row[2];
        [dot(rows.0), dot(rows.1)]
    }

    fn assert_close(actual: [f32; 2], expected: [f32; 2]) {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-2;
        assert!(
            close(actual[0], expected[0]) && close(actual[1], expected[1]),
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn an_upright_target_has_its_fragments_counted_from_the_bottom() {
        let rows = frag_to_local(&NORMAL, [W, H], [100.0, 50.0]);

        // Output pixel (110, 70) is drawn at fragment (110, 1080 - 70).
        assert_close(apply(rows, [110.0, 1010.0]), [10.0, 20.0]);
    }

    #[test]
    fn a_flipped_target_has_its_fragments_counted_from_the_top() {
        let rows = frag_to_local(&FLIPPED, [W, H], [100.0, 50.0]);

        assert_close(apply(rows, [110.0, 70.0]), [10.0, 20.0]);
    }

    #[test]
    fn the_origin_maps_to_zero() {
        let rows = frag_to_local(&FLIPPED, [W, H], [640.0, 360.0]);

        assert_close(apply(rows, [640.0, 360.0]), [0.0, 0.0]);
    }
}
