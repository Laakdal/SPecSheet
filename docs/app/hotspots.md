# Hotspots (Design D7)

The clickable markers on the truck. How the game itself creates its pins is in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md) (F5, F8); which parts of this match the game is tracked in [../PARITY.md](../PARITY.md).

## Anchors (`specsheet-data::assembly`)

- Each slot's anchor is the centre of its placed models' bounding box.
- An anchor that would fall inside the cabin box (models spanning both sides, e.g. mirror pairs) moves to the box's −x face.
- **Empty slots** (nothing placed) use their bare locators and carry `Anchor::empty`. These can sit away from any geometry, e.g. `b_grill` ahead of the bumper, or `f_mudflap` on the centre line for an L/R pair.
- **Interior slots:** a slot is interior when its models lie entirely inside the cabin box shrunk by 5 cm.
- **Wheel groups:** every wheel part of an axle shares one position and a group key (`Anchor::group`, e.g. `wheel_f`, the locator prefix).

## Which slots get a marker (`specsheet-data::view::overview`)

Only accessory slots get a pin; base parts are picked through their fixed tab (F5, see [ui.md](ui.md)). An accessory slot gets a pin when it has at least two options or is an addon slot. Each `Hotspot` carries its tab (exterior or interior accessories), the selected part's name, and the `empty` and `group` flags. Slots without a pin are listed in `TruckOverview::general` with their tab.

## On screen (`crates/specsheet`)

Every frame, `ui::ui` projects each anchor through the truck's transform and the 3D camera:

- **Far-side culling** (`hotspots::side_facing`): an anchor is hidden when its horizontal direction from the truck centre (1.5 m up) points away from the camera, i.e. the dot product with the camera direction is below −0.15. Anchors within 20 cm of the centre line are always shown. There is no per-pixel depth test yet.
- **Only the open tab's pins** are drawn: exterior accessories or interior accessories. Fixed tabs show none.
- **Panels hide pins:** an anchor that projects under a drawn part of the game screen is dropped.
- **Clustering** (`hotspots::cluster`): each visible marker joins the first cluster whose centre lies within 22 px, otherwise it starts a new one. A cluster's centre is the mean of its members. Clustering is in screen space, so it adapts as the camera moves.

### Marker styles

| Marker | Look |
|---|---|
| Single slot | Dot in its category colour: exterior orange, chassis & cabin green, wheels blue, interior lilac. Hover shows the slot name. |
| Cluster | White dot with the member count; a click opens a list of its slots with their selected parts |
| Wheel group (all members share `group`) | Numbered dot in the wheel colour |
| Empty slots only | Hollow ring: the slot has nothing fitted and the marker sits on bare locators |
| Active slot (open in the sidebar) | Extra orange ring |

## Known gaps

- Anchors for empty slots float where the locator is. A better anchor would be the first compatible option placed at that locator.
- A wheel group sits on the left wheels of its axle, so from the right side the marker is on the far wheel.
- Markers have no depth test against the meshes.
- Which slots the game pins, and where, is still Unknown (REA target `0x14223d860`).
