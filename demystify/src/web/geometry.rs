//! Board geometry abstraction for the SVG renderer.
//!
//! All output coordinates are in **cell units**: one cell is one unit, the
//! origin is the top-left of cell (row 0, col 0), x increases with column and y
//! with row. The renderer expresses every position, size and stroke width in
//! these units and lets the outer `viewBox` do the scaling, so the same drawing
//! code works for square and (later) hexagonal boards.
//!
//! A `Geometry` answers three questions for a logical cell `(row, col)`: where
//! is its centre, what polygon does it occupy, and which cells are its
//! neighbours (so shared edges can be de-duplicated when stroking outlines).

/// A point in cell-unit output coordinates.
pub type Point = (f64, f64);

/// Maps logical `(row, col)` cell indices to output geometry.
pub trait Geometry {
    fn width(&self) -> i64;
    fn height(&self) -> i64;

    /// Centre of cell `(row, col)` in cell-unit coordinates.
    fn cell_centre(&self, row: i64, col: i64) -> Point;

    /// Polygon vertices of cell `(row, col)`, in order (the closing edge back to
    /// the first vertex is implied).
    fn cell_polygon(&self, row: i64, col: i64) -> Vec<Point>;

    /// The cells sharing an edge with `(row, col)`, paired with the index into
    /// [`Geometry::cell_polygon`] of the edge they share (edge `k` runs from
    /// vertex `k` to vertex `k+1`). Used to skip interior edges when stroking
    /// the outline of a set of cells.
    fn neighbours(&self, row: i64, col: i64) -> Vec<(i64, i64, usize)>;

    /// Polygon for a cell's background, expressed in the cell's *local* `0..1`
    /// content space (the space established by the renderer's per-cell transform,
    /// which centres a `0.9`-scaled box on the cell). The shape follows the cell
    /// outline so backgrounds/highlights match square or hex cells.
    fn local_bg_points(&self) -> Vec<Point>;

    /// Bounding box of all grid cells (no margin): `(min_x, min_y, max_x, max_y)`.
    fn bounds(&self) -> (f64, f64, f64, f64);
}

/// Axis-aligned unit squares: cell `(row, col)` occupies `[col, col+1] ×
/// [row, row+1]`.
pub struct SquareGeometry {
    width: i64,
    height: i64,
}

impl SquareGeometry {
    #[must_use]
    pub fn new(width: i64, height: i64) -> Self {
        SquareGeometry { width, height }
    }
}

impl Geometry for SquareGeometry {
    fn width(&self) -> i64 {
        self.width
    }

    fn height(&self) -> i64 {
        self.height
    }

    fn cell_centre(&self, row: i64, col: i64) -> Point {
        (col as f64 + 0.5, row as f64 + 0.5)
    }

    fn cell_polygon(&self, row: i64, col: i64) -> Vec<Point> {
        let x = col as f64;
        let y = row as f64;
        // Vertex order: top-left, top-right, bottom-right, bottom-left.
        // Edge k (vertex k → k+1): 0 = top, 1 = right, 2 = bottom, 3 = left.
        vec![(x, y), (x + 1.0, y), (x + 1.0, y + 1.0), (x, y + 1.0)]
    }

    fn neighbours(&self, row: i64, col: i64) -> Vec<(i64, i64, usize)> {
        // (neighbour row, neighbour col, index of the shared edge on THIS cell).
        vec![
            (row - 1, col, 0), // top
            (row, col + 1, 1), // right
            (row + 1, col, 2), // bottom
            (row, col - 1, 3), // left
        ]
    }

    fn local_bg_points(&self) -> Vec<Point> {
        // Unit square; the per-cell 0.9 scale leaves the usual small margin.
        vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    }

    fn bounds(&self) -> (f64, f64, f64, f64) {
        (0.0, 0.0, self.width as f64, self.height as f64)
    }
}

/// Pointy-top hexagons in axial coordinates, matching bloomsweep's `hex.ts`:
/// the cell at `(row, col)` is the hex with axial `q = col`, `r = row`, centre
/// `(√3·(q + r/2), 1.5·r)` and centre→vertex radius 1. The board is the full
/// `height × width` rhombus of `(r, q)` pairs; a presence mask (handled by the
/// renderer) carves out other shapes such as a radius-N hexagon.
pub struct HexGeometry {
    width: i64,
    height: i64,
}

impl HexGeometry {
    #[must_use]
    pub fn new(width: i64, height: i64) -> Self {
        HexGeometry { width, height }
    }
}

/// `√3`, the pointy-top hex width factor (see `hex.ts`).
const SQRT3: f64 = 1.732_050_807_568_877_2;

impl Geometry for HexGeometry {
    fn width(&self) -> i64 {
        self.width
    }

    fn height(&self) -> i64 {
        self.height
    }

    fn cell_centre(&self, row: i64, col: i64) -> Point {
        let (q, r) = (col as f64, row as f64);
        (SQRT3 * (q + r / 2.0), 1.5 * r)
    }

    fn cell_polygon(&self, row: i64, col: i64) -> Vec<Point> {
        let (cx, cy) = self.cell_centre(row, col);
        // Vertices at 30°, 90°, …, 330°. Edge k runs vertex k → vertex k+1; its
        // outward normal points at 60·(k+1)°, which fixes the neighbour mapping
        // in `neighbours` below.
        (0..6)
            .map(|i| {
                let a = std::f64::consts::PI / 180.0 * (60.0 * i as f64 + 30.0);
                (cx + a.cos(), cy + a.sin())
            })
            .collect()
    }

    fn neighbours(&self, row: i64, col: i64) -> Vec<(i64, i64, usize)> {
        // (neighbour row, neighbour col, shared-edge index on THIS cell).
        // Derived from each edge's outward normal direction (see cell_polygon).
        vec![
            (row + 1, col, 0),     // edge 0, normal 60°
            (row + 1, col - 1, 1), // edge 1, normal 120°
            (row, col - 1, 2),     // edge 2, normal 180°
            (row - 1, col, 3),     // edge 3, normal 240°
            (row - 1, col + 1, 4), // edge 4, normal 300°
            (row, col + 1, 5),     // edge 5, normal 0°
        ]
    }

    fn local_bg_points(&self) -> Vec<Point> {
        // Hexagon of radius 1 about local (0.5, 0.5); the per-cell 0.9 scale maps
        // it to radius 0.9 about the cell centre, matching the square's margin.
        (0..6)
            .map(|i| {
                let a = std::f64::consts::PI / 180.0 * (60.0 * i as f64 + 30.0);
                (0.5 + a.cos(), 0.5 + a.sin())
            })
            .collect()
    }

    fn bounds(&self) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for row in 0..self.height {
            for col in 0..self.width {
                for (x, y) in self.cell_polygon(row, col) {
                    b.0 = b.0.min(x);
                    b.1 = b.1.min(y);
                    b.2 = b.2.max(x);
                    b.3 = b.3.max(y);
                }
            }
        }
        if b.0 > b.2 {
            // No cells.
            (0.0, 0.0, 0.0, 0.0)
        } else {
            b
        }
    }
}
