//! Screen-space handling of hotspot anchors (Design D7): far-side culling
//! and grouping of markers that would overlap. Many slot locators sit
//! within a few centimetres of each other (e.g. on the front face), so
//! nearby markers merge into one cluster that lists its slots.

use bevy::math::{Vec2, Vec3};

/// Markers closer than this (logical pixels) merge into one cluster.
pub const CLUSTER_RADIUS: f32 = 22.0;

/// Height of the point the far-side test measures from, in truck space.
pub const TRUCK_CENTER_HEIGHT: f32 = 1.5;

/// True unless `anchor` lies on the far side of the truck as seen from
/// `camera`. There is no depth test against the meshes: the horizontal
/// direction from the truck centre to the anchor is compared with the
/// direction to the camera (see `docs/RENDERING.md`).
pub fn side_facing(anchor: Vec3, truck_center: Vec3, camera: Vec3) -> bool {
    let to_camera = (camera - truck_center).normalize_or_zero();
    let mut outward = anchor - truck_center;
    outward.y = 0.0;
    outward.length_squared() < 0.04 || outward.normalize().dot(to_camera) > -0.15
}

/// A visible anchor in viewport coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    /// Index into the caller's hotspot list.
    pub index: usize,
    pub position: Vec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub position: Vec2,
    /// Indices of the member hotspots, in input order.
    pub members: Vec<usize>,
}

/// Greedy clustering: each anchor joins the first cluster whose centre is
/// within `radius`, otherwise it starts a new one. Centres are the mean of
/// their members.
pub fn cluster(anchors: &[Projected], radius: f32) -> Vec<Cluster> {
    let mut clusters: Vec<(Vec2, Vec<Projected>)> = Vec::new();
    for anchor in anchors {
        let near = clusters
            .iter_mut()
            .find(|(center, _)| center.distance(anchor.position) <= radius);
        match near {
            Some((center, members)) => {
                members.push(*anchor);
                *center = members.iter().map(|m| m.position).sum::<Vec2>() / members.len() as f32;
            }
            None => clusters.push((anchor.position, vec![*anchor])),
        }
    }
    clusters
        .into_iter()
        .map(|(position, members)| Cluster {
            position,
            members: members.iter().map(|m| m.index).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(index: usize, x: f32, y: f32) -> Projected {
        Projected {
            index,
            position: Vec2::new(x, y),
        }
    }

    #[test]
    fn merges_nearby_markers_and_averages_their_centre() {
        let clusters = cluster(
            &[at(0, 0.0, 0.0), at(1, 10.0, 0.0), at(2, 100.0, 0.0)],
            22.0,
        );
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[0].position, Vec2::new(5.0, 0.0));
        assert_eq!(clusters[1].members, vec![2]);
    }

    #[test]
    fn joins_the_first_cluster_in_range() {
        // 2 is within range of both centres; it joins the first.
        let clusters = cluster(&[at(0, 0.0, 0.0), at(1, 40.0, 0.0), at(2, 20.0, 0.0)], 22.0);
        assert_eq!(clusters[0].members, vec![0, 2]);
        assert_eq!(clusters[1].members, vec![1]);
    }

    #[test]
    fn hides_anchors_on_the_far_side() {
        let center = Vec3::new(0.0, 1.5, 0.0);
        let camera = Vec3::new(-10.0, 3.0, 0.0);
        assert!(side_facing(Vec3::new(-1.2, 1.0, 0.0), center, camera));
        assert!(!side_facing(Vec3::new(1.2, 1.0, 0.0), center, camera));
        // Anchors near the centre line are always shown.
        assert!(side_facing(Vec3::new(0.1, 3.0, 0.1), center, camera));
    }
}
