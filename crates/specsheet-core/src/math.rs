//! Small 3D maths shared by the parsers, assembly and rendering, in game
//! (truck) space: x right, y up, -z forward. Quaternions are `(x, y, z, w)`.

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    /// Smallest x, y and z.
    pub min: [f32; 3],
    /// Largest x, y and z.
    pub max: [f32; 3],
}

impl Aabb {
    /// Midpoint of `min` and `max`.
    pub fn center(&self) -> [f32; 3] {
        std::array::from_fn(|axis| (self.min[axis] + self.max[axis]) * 0.5)
    }

    /// Smallest box containing both boxes.
    pub fn union(&self, other: &Aabb) -> Aabb {
        Aabb {
            min: std::array::from_fn(|axis| self.min[axis].min(other.min[axis])),
            max: std::array::from_fn(|axis| self.max[axis].max(other.max[axis])),
        }
    }

    /// The box moved in by `margin` on every side. An axis never inverts: if
    /// the margin is larger than half the extent, that axis collapses to the
    /// centre.
    pub fn shrunk(&self, margin: f32) -> Aabb {
        let center = self.center();
        let axis = |i: usize| {
            let (lo, hi) = (self.min[i] + margin, self.max[i] - margin);
            if lo <= hi {
                (lo, hi)
            } else {
                (center[i], center[i])
            }
        };
        Aabb {
            min: std::array::from_fn(|i| axis(i).0),
            max: std::array::from_fn(|i| axis(i).1),
        }
    }

    /// Whether `p` lies inside, bounds included.
    pub fn contains_point(&self, p: [f32; 3]) -> bool {
        (0..3).all(|axis| self.min[axis] <= p[axis] && p[axis] <= self.max[axis])
    }

    /// Whether `other` lies entirely inside, bounds included.
    pub fn contains_box(&self, other: &Aabb) -> bool {
        self.contains_point(other.min) && self.contains_point(other.max)
    }
}

/// Rigid transform of a placed model, in game (truck) space: x right,
/// y up, -z forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Position, applied last.
    pub translation: [f32; 3],
    /// Unit quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    /// Uniform scale, applied first.
    pub scale: f32,
}

impl Transform {
    /// No translation, no rotation, scale 1.
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
    };

    /// `self` applied after `child`: the child's transform expressed in
    /// the parent's space (parent ∘ child).
    pub fn compose(&self, child: &Transform) -> Transform {
        let scaled = child.translation.map(|c| c * self.scale);
        let rotated = rotate(self.rotation, scaled);
        Transform {
            translation: [0, 1, 2].map(|i| self.translation[i] + rotated[i]),
            rotation: quat_mul(self.rotation, child.rotation),
            scale: self.scale * child.scale,
        }
    }
}

/// Hamilton product `a * b` of quaternions stored `(x, y, z, w)`.
fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotates `v` by the unit quaternion `q = (x, y, z, w)`.
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    // v' = v + w*t + q_xyz × t, with t = 2 * (q_xyz × v)
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

/// The axis-aligned box around `aabb`'s eight corners after `transform`.
pub fn transform_aabb(aabb: &Aabb, transform: &Transform) -> Aabb {
    let corners = (0..8).map(|corner| {
        let local = [
            if corner & 1 == 0 {
                aabb.min[0]
            } else {
                aabb.max[0]
            },
            if corner & 2 == 0 {
                aabb.min[1]
            } else {
                aabb.max[1]
            },
            if corner & 4 == 0 {
                aabb.min[2]
            } else {
                aabb.max[2]
            },
        ];
        let rotated = rotate(transform.rotation, local.map(|c| c * transform.scale));
        let world = [0, 1, 2].map(|i| rotated[i] + transform.translation[i]);
        Aabb {
            min: world,
            max: world,
        }
    });
    corners.reduce(|a, b| a.union(&b)).unwrap_or(*aabb)
}
