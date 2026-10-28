# Hotspots (Design D7)

The clickable markers on the truck. How the game itself creates its pins is in [../REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md) (F5, F8); which parts of this match the game is tracked in [../PARITY.md](../PARITY.md).

## Anchors (`specsheet-data::assembly`)

- Each slot's anchor is the centre of its placed models' bounding box.
- An anchor that would fall inside the cabin box (models spanning both sides, e.g. mirror pairs) moves to the box's −x face.
- **Empty slots** (nothing placed) use their bare locators and carry `Anchor::empty`. These can sit away from any geometry, e.g. `b_grill` ahead of the bumper, or `f_mudflap` on the centre line for an L/R pair.
- **Interior slots:** a slot is interior when its models lie entirely inside the cabin box shrunk by 5 cm.
- **Wheel groups:** every wheel part of an axle shares one position and a group key (`Anchor::group`, e.g. `wheel_f`, the locator prefix).
- **Cockpit slots:** interior-only accessories (the steering wheel, curtains, toys…) attach to locators on the interior's full cockpit, moved into truck space. Their anchors come from those placements or locators, so they get pins in the interior view (F10).

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
- **Wheels don't match the game.** The game gives each wheel part its own pin around the wheel: tyre 0.4 m up, disc 0.25 m up, rim and hub at the centre, cover behind, nuts in front. Its rim pin sits on the wheel locator ([F5](../REVERSE_ENGINEERING.md), `research/evidence/pin-table.txt`). We stack all seven wheel slots, including `f_wheel`, which has no pin entry, on the mean of the axle group's left locators. That shows as one dot counting 7, and on multi-axle trucks it falls between two axles.
- A wheel group sits on the left wheels of its axle, so from the right side the marker is on the far wheel.
- Markers have no depth test against the meshes.
- Our anchors ignore the game's pin table (`TuningScene::pins`: `pin_offset`, `pin_dir`, `double_sided`). How the game applies it to non-wheel parts is Unknown; REA on the pin reader is in progress.
