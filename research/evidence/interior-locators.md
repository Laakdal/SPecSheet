# Where the cockpit goes: the cabin's `interior` locators

ETS2 1.60.1.7, data read with our own parsers (`model_info`, `scs`). The exe side is in `rea-pin-roots.txt` (static REA), which confirms the rule below.

## Summary

- **Every cabin model has two cockpit locators, one for each drive side (Confirmed, data).** All 42 cabin models under `/vehicle/truck/*/cabin/` (wipers and window models excluded) carry them beside `ext_interior`. The left-hand-drive one sits at about x −0.7 and the right-hand-drive one at about x +0.7, at the driver's head height (y 2.3–3.0).
- **The cockpit model is built around the driver's head (Confirmed, data).** In every cockpit `model` checked, `steering_w` sits at x ≈ 0, 0.32–0.54 m below and 0.41–0.56 m in front of the model's origin. So the model's origin is the driver's eye point, and it belongs on one of those two locators.
- **`ext_interior` is a different point (Confirmed, data).** On most cabins it is the truck's origin, (0, 0, 0) or close to it; the exterior-view `ext_model` is modelled in truck space and placed there (F10). It coincides with the left-hand-drive cockpit locator only on the Scania 2016 cabins.
- **Our app's bug (Confirmed, fixed 2026-11-04).** `specsheet-assembly` placed the full cockpit, and so the interior pins and interior parts, at `ext_interior`. That is right only on the EU Scania 2016. On a Renault T high sleeper it put the steering wheel at (−0.005, −0.54, −0.48) in truck space, below the floor in the middle of the cab, instead of about (−0.705, 2.44, −2.23).

## Cockpit locators per cabin

Positions are in the cabin model's coordinates (x right, y up, z back), in locator order as the model lists them.

| Cabins | Left-hand drive | Right-hand drive | `ext_interior` |
|---|---|---|---|
| Volvo FH 2012, 2021, 2024 (16 models) | `interior` (−0.718, 2.692, −1.894) | `interior_uk` (+0.723, 2.692, −1.894) | (0, 0, 0) |
| Volvo FH16 2009 `cab_ab`, `cab_b`, `cab_c` | `interior` (−0.710, 2.618, −2.056) | `interior` (+0.673, 2.618, −2.056), the same name | (0, 2.441, −2.232) |
| Scania 2016 R `cab_r_high`, `_8x4`, `cab_r_low` | `interior_r` (−0.683, 2.549, −1.863) | `interior_r` (+0.682, 2.549, −1.863), the same name | (−0.683, 2.549, −1.863) |
| Scania 2016 S `cab_s_high`, `_8x4`, `cab_s_low` | `interior_s` (−0.683, 2.705, −1.862) | `interior_s1` (+0.683, 2.705, −1.862) | (−0.683, 2.705, −1.862) |
| Renault T `cabin_a` (high sleeper) | `interior` (−0.700, 2.980, −1.750) | `interior_uk` (+0.700, 2.980, −1.750) | (0, 0, 0) |
| Renault T `cabin_b` (sleeper) | `interior` (−0.700, 2.739, −1.750) | `interior_uk` (+0.700, 2.739, −1.750) | (0, −0.241, 0) |
| Renault E-Tech T `cabin_b` | `interior` (−0.705, 2.603, −1.793) | `interior_uk` (+0.705, 2.603, −1.793) | (0, 0, 0) |
| MAN TGX 2020 `cab_gm`, `cab_gn`, `cab_gx` | `interior` (−0.734, 2.804, −1.774) | `interior_uk` (+0.734, 2.804, −1.774) | (−0.722, 2.720, −1.896) |
| MAN TGX `cab_a`, `cab_b`, `cab_c` | `interior` (−0.795, 2.800, −1.788) | `interior1` (+0.795, 2.800, −1.788) | (0, 0, 0) |
| Iveco S-Way `cab_as` | `interior` (−0.738, 2.708, −1.783) | `interior_uk` (+0.738, 2.708, −1.783) | (0, 0, 0) |
| DAF XF Electric `cab_slh`; DAF 2021 `cab_hi`, `cab_low`, `cab_mid` | `interior` (−0.722, 2.720, −1.896) | `interior_uk` (+0.722, 2.720, −1.896) | (0, 0.029, 0) |
| DAF XD `cab_day`, `cab_sl`, `cab_slh` | `interior` (−0.722, 2.310, −1.898) | `interior_uk` (+0.722, 2.310, −1.898) | (0, 0, 0) |

Name patterns, over the 42 models:

| Locator names | Models |
|---|---|
| `interior`, `interior_uk` | 30 |
| `interior_s`, `interior_s1` | 3 |
| `interior_r`, `interior_r` | 3 |
| `interior`, `interior1` | 3 |
| `interior`, `interior` | 3 |

Every name starts with `interior`. `ext_interior` doesn't, so a prefix search for `interior` never picks it. The left-hand-drive locator is always the first `interior…` locator in model order, and the right-hand-drive one the second.

## Cockpit models are head-relative

`steering_w` in each cockpit `model` (the interior definition's `model`, not `ext_model`):

| Cockpit model | `steering_w` |
|---|---|
| `/vehicle/truck/renault_t/interior/model_24/antracite_slh_24.pmd` | (−0.005, −0.540, −0.475) |
| `/vehicle/truck/scania_2016/interior/model.pmd` | (−0.001, −0.372, −0.555) |
| `/vehicle/truck/volvo_fh_2024/interior/model/gt.pmd` | (−0.003, −0.324, −0.556) |
| `/vehicle/truck/daf_xd/interior/model/day.pmd` | (+0.003, −0.398, −0.529) |
| `/vehicle/truck/man_tgx_2020/interior/model/gm.pmd` | (−0.000, −0.363, −0.521) |
| `/vehicle/truck/iveco_sway/interior/model/as.pmd` | (+0.003, −0.324, −0.412) |

On the Renault T low sleeper, the cockpit locator is 0.241 m lower than on the high sleeper. That matches its `ext_interior`, which is 0.241 m below the origin. The whole interior moves down with the lower cab.

## The interior camera unit

Each truck's `data.sii` names a driving camera, e.g. `interior_camera: camera.interior.renault.t`. Its `vehicle_interior_camera` unit is in `/def/camera/units/interior_<truck>.sui`, and carries the camera settings for that cockpit:

| Unit | `camera_fov` | `head_offset` | `mouse_up_down_default` |
|---|---|---|---|
| `camera.interior.renault.t` | 72 | (0, −0.17, 0.01) | −5.5 |
| `camera.interior.renault.etech_t` | 65 | (0, −0.065, 0.05) | −5.5 |
| `camera.interior.scania.s_2016`, `.r_2016` | 71 | (0, 0.01, 0.03) | −9 |
| `camera.interior.volvo.fh_2021`, `.fh_2024` | 65 | (0, 0, 0) | −7 |
| `camera.interior.volvo.fh16_2012` | 70 | (0, −0.05, 0.1) | −7 |
| `camera.interior.volvo.fh16_09` | 77 | (0, 0, 0.12) | −7 |
| `camera.interior.daf.2021`, `.daf.xd`, `.daf.xf_electric`, `iveco.sway`, `man.tgx_2020` | 65 | (0, 0, 0.04) | −8 (MAN −9) |
| `camera.interior.man.tgx` | 77 | (0, 0, −0.04) | −5 |

Each unit also has look limits and presets, a shake animation and per-azimuth `head_offset_offset` ranges. Whether the tuning screen's interior view uses this driving camera at all is Unknown.

## Labels

- **Confirmed (data):** the locator names, positions and order above; cockpit models are head-relative; the camera unit values.
- **Confirmed (exe, `rea-pin-roots.txt`):** the game places the cockpit `model` at the first visible locator whose name starts with `interior`, position only, searched like the pins (the interior's exterior model, the cabin, the axle models, the chassis). There is no separate UK name: the cabin's variant decides, because each drive side's locator sits on its own variant's part. "First in model order" above is how the data looks, not the rule.
- **Confirmed (exe):** the interior pins add the same eye point in the interior camera. The camera unit's `head_offset` is not used for pins.
- **Unknown:** whether the camera's `head_offset` moves the eye in the tuning screen's interior view, and which field of view that view uses.

## Done and next

- Done: the cockpit is placed at the eye point (test `real_install::every_preset_has_a_cockpit_at_the_drivers_eye`, every truck in both drives); F10, `docs/app/scenes.md` and `docs/app/hotspots.md` corrected.
- Found while checking: interior-only parts skipped a cockpit locator when the cabin had one matching the slot, so the Renault T 2024 carpet (`carpet` on the cockpit floor, `carpet_hi` on the cabin) sat at the eye point and covered the view. Fixed; test `an_interior_part_takes_the_cockpits_locator_even_when_the_cabin_has_one`.
- Next: whether the tuning screen's interior view applies the camera unit's `head_offset` and `camera_fov`.
