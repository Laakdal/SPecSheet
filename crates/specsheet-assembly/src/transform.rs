//! Rigid transforms of placed models and the quaternion helpers they use.

use specsheet_formats::pmg::{Aabb, Locator};

/// Rigid transform of a placed model, in game (truck) space: x right,
/// y up, -z forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Unit quaternion `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub scale: f32,
}

impl Transform {
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

    pub(super) fn from_locator(locator: &Locator) -> Self {
        let [w, x, y, z] = locator.rotation_wxyz;
        Self {
            translation: locator.position,
            rotation: [x, y, z, w],
            scale: locator.scale,
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
pub(super) fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
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

pub(super) fn transform_aabb(aabb: &Aabb, transform: &Transform) -> Aabb {
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
