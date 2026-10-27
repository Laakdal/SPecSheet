#!/usr/bin/env bash
# Builds research/evidence/ui-layout-survey.txt from research/extracted/ (run from the repo root,
# after the extraction commands listed at the top of that file).
X=research/extracted
S=../../../tools/research/ui_survey.py
T="truck_configuration_left.sii truck_configuration_center.sii truck_configuration_right.sii truck_configuration_bcg.sii truck_config_filters.sii"
OUT=research/evidence/ui-layout-survey.txt
survey() { (cd "$X/ui" && python -I "$S" "$@"); }

# Intermediate outputs (all under the gitignored research/extracted/).
mkdir -p "$X/ui_tc5"
for f in $T; do cp "$X/ui/$f" "$X/ui_tc5/"; done
survey . stats > "$X/s_all.txt"
(cd "$X/ui" && python -I "$S" ../ui_tc5 stats) > "$X/s_tc5.txt"
if [ -n "$ETS2_DIR" ]; then
  cargo run -q --release -p specsheet-data --example ui_icon_survey -- /def/vehicle/truck 3 > "$X/icon_survey_truck.txt"
  cargo run -q --release -p specsheet-data --example def_stats -- /ui > "$X/def_stats_ui.txt"; echo "def_stats /ui exit $?" >> "$X/def_stats_ui.txt"
  SCS="cargo run -q --release -p specsheet-archive --example scs --"
  $SCS find def/vehicle/truck /accessory/ | sed -E 's#.*/accessory/([^/]+)/.*#\1#' | sort -u > "$X/truck_slot_dirs.txt"
  $SCS ls def/vehicle | grep '/$' | tr -d '/' >> "$X/truck_slot_dirs.txt"
  sort -u -o "$X/truck_slot_dirs.txt" "$X/truck_slot_dirs.txt"
fi

{
cat <<'EOF'
UI layout survey: /ui/truck_configuration_*.sii, truck_config_filters.sii, templates, fonts, icons, locale
ETS2 1.60.1.7. Produced 2026-10-27. Counts and short excerpts only; no game file is reproduced.

Commands (Git Bash, repo root)
  export ETS2_DIR="D:/Steam Library/steamapps/common/Euro Truck Simulator 2" MSYS_NO_PATHCONV=1
  SCS=<target>/release/examples/scs        # cargo build --release -p specsheet-archive --example scs
  # 1. extract /ui/ (488 .sii/.sui + credits.txt) and a few locale/font/def files into the gitignored research/extracted/
  $SCS find ui "" > /tmp/ui.lst
  while read p; do mkdir -p "research/extracted/$(dirname "$p")"; $SCS cat "$p" > "research/extracted/$p"; done < /tmp/ui.lst
  for f in locale/common.sui locale/ui_colors.sui locale/en_us/local.sii locale/en_us/localization.sui \
           locale/en_us/accessories.sui font/normal.font font/glyphs_xb.font font/big_0.mat; do
    mkdir -p research/extracted/$(dirname $f); $SCS cat $f > research/extracted/$f; done
  $SCS cat def/truck_config_scene.sii > research/extracted/truck_config_scene.sii
  # 2. this whole file: bash tools/research/ui_evidence.sh (with ETS2_DIR set it also runs the cargo examples below)
  #    single steps (tools/research/ui_survey.py; run from research/extracted/ui):
  python -I ../../../tools/research/ui_survey.py . stats | area | coords [files] | tree <f> | refs <f..> | templates <f..>
  python -I ../../../tools/research/ui_survey.py ../ui_tc5 stats
  python -I ../../../tools/research/ui_survey.py . loc ../locale <key | ~key-regex | =value-regex>...
  # 3. icons (crates/specsheet-data/examples/ui_icon_survey.rs)
  cargo run --release -p specsheet-data --example ui_icon_survey -- /def/vehicle/truck 3
  cargo run --release -p specsheet-data --example ui_icon_survey -- /def/vehicle 0
  cargo run --release -p specsheet-data --example ui_icon_survey -- - 0 <icon or /abs/path.tobj>...
  cargo run --release -p specsheet-data --example def_stats -- /ui      # our SII parser over every /ui/*.sii
  # 4. runtime: D:/SCS/Euro Truck Simulator 2/game.log.txt (pack set 1.60.1.7): grep -n "UI reference mode"
  Tree/value markup below is replaced by <markup>; only ids, classes, rects, attribute names and short tokens are kept.

==========================================================================
A. Runtime: UI reference resolution
==========================================================================
game.log.txt:24   [ufs] Loaded pack set version 1.60.1.7 ...
game.log.txt:499  00:00:04.887 : Setting UI reference mode to: 1440x900...
game.log.txt:587  [win] new windowed size: 2560 1440        (actual display mode of this run)
config.cfg: no UI-scale cvar (grep -i scale|aspect: only r_scale_x/y, r_mirror_scale_*, r_imgui_scale, stereo UI vars)

==========================================================================
B. Whole /ui/ tree (ui_survey.py . stats / area / coords)
==========================================================================
EOF
head -1 $X/s_all.txt
echo "top classes (units, files):"
grep "^== " $X/s_all.txt | head -32 | sed 's/^== /  /'
echo
echo "root window attributes (from stats):"
grep -A25 "^== ui_blend_window" $X/s_all.txt | grep -E "keep_aspect|clip_children|my_parent " | sed 's/^/  ui_blend_window/'
grep -A23 "^== ui::window " $X/s_all.txt | grep -E "keep_aspect|clip_children|my_parent " | sed 's/^/  ui::window/'
echo
echo "pointer / tab over all units (grep -h '^ *pointer:' -r research/extracted/ui | sort | uniq -c):"
grep -h "^ *pointer:" -r $X/ui | sort | uniq -c | sed 's/^/  /'
grep -h "^ *tab:" -r $X/ui | sort | uniq -c | sort -rn | head -6 | sed 's/^/  /'
echo
survey . area
echo
survey . coords | head -2
cat <<'EOF'

==========================================================================
C. The five tuning-screen files (ui_survey.py ../ui_tc5 stats): class -> attribute count [value kinds]
   (coords_*/area_*/id/layer/tab/pointer/my_parent omitted: present on every widget unit)
==========================================================================
EOF
grep -v "coords_\|area_\|^   id \|^   tab \|^   pointer \|^   my_parent \|^   layer " $X/s_tc5.txt
echo
# shellcheck disable=SC2086
survey . coords $T | head -2
cat <<'EOF'

screen_desc truck.config (ui/desc/truck.sui): ui_script[] truck_config_left|/ui/truck_configuration_left.sii,
  truck_config_right|/ui/truck_configuration_right.sii, truck_config|/ui/truck_configuration_center.sii; no background: line
  (truck.service / truck.dealer have background: "/ui/truck_*_bcg.sii").
screen_desc truck.config.accessory.filters: ui_script[] accessory_filters|/ui/truck_config_filters.sii, overlay: true, modal: true.
grep -rl "truck_configuration_bcg\|truck_config_scene" research/extracted/ui -> only truck_configuration_bcg.sii itself.
truck_configuration_bcg.sii: ui_truck_scene config_path "/def/truck_config_scene.sii" (camera_fov 50, camera_aim (0, 2.6, -1.465));
/def/desktop/config.sii: ui_truck_scene_base_config, camera_fov 45, scene_cinematic_camera (F7).

==========================================================================
D. Widget trees (ui_survey.py . tree <file>): [id] class name L<layer> tab ptr rect (x l..r, y b..t) (w x h) attrs
==========================================================================
EOF
for f in truck_configuration_bcg truck_configuration_center truck_configuration_left truck_configuration_right truck_config_filters; do
  echo "--- $f.sii"
  survey . tree $f.sii
done
cat <<'EOF'

==========================================================================
E. References made directly by the five files (ui_survey.py . refs <5 files>)
==========================================================================
EOF
# shellcheck disable=SC2086
survey . refs $T
cat <<'EOF'

==========================================================================
F. look_template resolution (ui_survey.py . templates <5 files>): template class, width/height, fonts, materials
==========================================================================
EOF
# shellcheck disable=SC2086
survey . templates $T | grep -v "^   fields"
cat <<'EOF'

btn.truck_acc (accessory tile, ui/template/button.sii:1101), parameters as used by its n_pml:
  %0 name, %1 icon (img src=/material/ui/accessory/%1, no extension), %2 optional info, %3 price,
  %4/%5 alpha of the new.mat / owned.mat badges, %6 locked text (d_pml), %8 extra, %9 origin badge accessory_%9.mat.
  Layout: 22 px header strip + 74 px body = item_height 96 of _.acclist; icon at hshift 10, vshift 30.
btn.truck_grp (category tab, button.sii:1172): img src=/material/ui/truck_config/%0.mat, width 64, height 64;
  %0 comes from button_row value_list "cabin~~chassis~~engine~~transmission~~color~~e_accessories~~interior~~i_accessories".
txt.normal.color.center (text.sii): "<color value=%0>%1"; used with value "@@clr_sel@@|@@select_cat@@" -> value splits on '|'.

==========================================================================
G. Fonts and UI textures
==========================================================================
font/normal.font header: "# SCS Font (SDF format)"; "metrics assume rendering at a reference resolution of 1024x768";
  vert_span 20.98, line_spacing 0, default_scale 0.83; 7 "image:" lines (/font/normal_N.mat, w, h, scale x, scale y, SDF padding);
  glyph rows "xHHHH, atlas x, atlas y, width, height, plane x, plane y, advance, image index".
font/glyphs_xb.font: same format, default_scale 1.0, 2 images (icon glyphs; prev/next tab use U+E8C3 / U+E8C5).
font/big_0.mat: effect "ui.font.msdf.rfx", aux[0] {1024, 512, 7.845584, 0}, texture source "/font/normal_0.tobj", clamp, linear, mip none.
ui_icon_survey -- - 0 <paths>:
  /material/ui/truck_config/cabin.tobj          64x64 dxgi 88 mips 1   (cabin.mat: effect "ui.sdf.rfx", aux[0] {64, 64, 2, 0})
  /material/ui/truck_config/e_accessories.tobj  64x64 dxgi 88 mips 1
  /material/ui/white.tobj                        1x1 dxgi 93 mips 1    (white.mat: effect "ui.rfx")
  /material/ui/corner_4px.tobj                  16x16 dxgi 91 mips 1
  /font/normal_0.tobj                         1024x512 dxgi 88 mips 1
  /font/glyphs_xb_0.tobj                         64x64 dxgi 88 mips 1
scs ls material/ui/truck_config: cabin chassis engine transmission color e_accessories interior i_accessories lights camera
  default_view walk_mode fullscreen fullscreen_exit show_vehicles cart sell new owned leave add brand btn_* accessory_* trailer_*
  (each as .mat + .tobj)

==========================================================================
H. Accessory icons (ui_icon_survey -- /def/vehicle/truck 3)
==========================================================================
EOF
grep -v "^    |\|^---\|exists;\|^$" $X/icon_survey_truck.txt
cat <<'EOF'
examples (def_grep <class> icon 1 /def/vehicle/truck/scania.s_2016; ui_icon_survey -- - 0 <icon>):
  accessory_cabin_data     cabin/highline.sii     icon "truck/scania_2016/accessory/cabin/cab_scania_s_2016_highline" -> 256x64 dxgi 91
  accessory_chassis_data   chassis/4x2.sii        icon "truck/scania_2016/accessory/chassis/chassis_scania_2016_4x2"  -> 256x64 dxgi 91
  accessory_engine_data    engine/dc13_370.sii    icon "engine_scania_dc13"                                           -> 64x64 dxgi 91
  accessory_addon_data     accessory/windowtrim/windowtrim_mg_01.sii icon "windowtrim/scania_2016/windowtrim_mg_01"     -> 256x64 dxgi 91
  accessory_paint_job_data paint_job/770s_met1_a.sii icon "paintjob/scania_770s_red"                                  -> 256x64 dxgi 91
  each of these .mat: effect "ui.rfx" { texture : "texture" { source : "<basename>.tobj", u/v clamp, mip_filter none } }
  legacy form: material : "ui" { texture : "<basename>.tobj" texture_name : "texture" }
ui_icon_survey -- /def/vehicle 0 (incl. trailers): 9006 distinct icon values, 8960 resolve to /material/ui/accessory/<icon>.mat,
  46 unresolved (43 under trailer_owned).

==========================================================================
I. Localisation (ui_survey.py . loc ../locale ...)
==========================================================================
EOF
survey . loc ../locale truck_config_t select_cat basket_t checkout clear_cart total refund price tc_new_total tc_extra_price \
  tc_orig_price truck_analysis_t current_truck_params search clear_input accessory_filters_t add_all remove_all \
  notification_walking_camera imv_unf_joy_nav_prv imv_unf_joy_nav_nxt cabin chassis engine transmission interior color \
  e_accessories i_accessories cabin_t chassis_t engine_t transmission_t interior_t accessory_t paint_job_t accessory \
  in_accessory lights '=^Interior Accessories$' '~^imv_'
echo
echo "colour keys (locale/ui_colors.sui, included by locale/common.sui:1079):"
survey . loc ../locale clr_red clr_green clr_blue clr_white clr_sel clr_txt clr_bg_main | tail -n +2
survey . loc ../locale '~^clr_' | tail -n 1 | cut -c1-40
cat <<'EOF'

==========================================================================
J. /def/truck_config_scene.sii (config_path of truck_configuration_bcg.sii), counts only
==========================================================================
EOF
F=$X/truck_config_scene.sii
grep -o "^\s*[a-z_]*\(\[\]\)\?\s*:" $F | tr -d ' \t' | sort | uniq -c | sed 's/^/  /'
echo "  pin_look values:"
grep "pin_look" $F | awk '{print $2}' | sort | uniq -c | sed 's/^/   /'
echo "  pin_group values (65): $(grep 'pin_group' $F | awk '{print $2}' | tr '\n' ' ')"
echo "  interior_only (17): $(grep 'interior_only' $F | awk '{print $2}' | tr '\n' ' ')"
echo "  hookups_exterior_only: $(grep 'hookups_exterior_only' $F | awk '{print $2}')"
echo "  example entry: pin_group beacon / pin_look custom_acc / pin_offset (0, 0.3, 0) / pin_dir (0, 0, 0) / pin_dir_uk (0, 0, 0) / double_sided false"
} > "$OUT" 2>&1

{
cat <<'EOS'

==========================================================================
K. Tab ids and pin ids (cross-check of F5; grep over research/extracted/ui)
==========================================================================
grep -n "id_list\|value_list" truck_configuration_*.sii trailer_configuration_center.sii :
EOS
(cd "$X/ui" && grep -n "id_list\|value_list" truck_configuration_*.sii trailer_configuration_center.sii | grep -v '""' | sed 's/^/  /')
echo 'grep -n "^ id: [0-9]\{5,\}" truck_configuration_*.sii truck_config_filters.sii (ids >= 10000):'
n=$(cd "$X/ui" && grep -c "^ id: [0-9]\{5,\}" truck_configuration_*.sii truck_config_filters.sii | awk -F: '{s+=$2} END {print s}')
echo "  $n matches"
} >> "$OUT"

{
cat <<'EOS'

==========================================================================
L. Our SII parser over /ui/ (cargo run --release -p specsheet-data --example def_stats -- /ui)
==========================================================================
EOS
echo "  classes reported: $(grep -c '^[^ ]' "$X/def_stats_ui.txt" | head -1) (lines without indent, incl. the exit line)"
grep "def_stats /ui exit" "$X/def_stats_ui.txt" | sed 's/^/  /'
cat <<'EOS'

==========================================================================
M. Do pin_group / interior_only values name slots? (truck_config_scene.sii vs slot directories)
   slot dirs = names after /accessory/ in `scs find def/vehicle/truck /accessory/`, plus /def/vehicle/<dir> (wheel slots)
==========================================================================
EOS
grep pin_group "$X/truck_config_scene.sii" | awk '{print $2}' | sort -u > "$X/pin_groups.txt"
grep interior_only "$X/truck_config_scene.sii" | awk '{print $2}' | sort -u > "$X/interior_only.txt"
echo "  pin_group matching a slot dir: $(comm -12 "$X/pin_groups.txt" "$X/truck_slot_dirs.txt" | wc -l) of $(wc -l < "$X/pin_groups.txt")"
echo "  unmatched: $(comm -23 "$X/pin_groups.txt" "$X/truck_slot_dirs.txt" | tr '\n' ' ')"
echo "  interior_only matching a slot dir: $(comm -12 "$X/interior_only.txt" "$X/truck_slot_dirs.txt" | wc -l) of $(wc -l < "$X/interior_only.txt")"
cat <<'EOS'

==========================================================================
N. Executable string (existing REA evidence, research/evidence/rea-search-tuning-strings.txt:297-300)
==========================================================================
EOS
sed -n 297,300p research/evidence/rea-search-tuning-strings.txt | sed 's/^/  /'
echo "  grep -c 'pin_group|interior_only|truck_config_scene' rea-search-tuning-strings.txt: $(grep -c -E 'pin_group|interior_only|truck_config_scene' research/evidence/rea-search-tuning-strings.txt) (not in that regex)"
} >> "$OUT"
if [ -n "$ETS2_DIR" ]; then
  {
  echo
  echo "O. imv_unf_* keys in every en_us locale file and the shared ones (grep -c imv_unf after scs cat):"
  for f in locale/en_us/local.sii locale/en_us/localization.sui locale/en_us/accessories.sui locale/en_us/academy.sui \
           locale/en_us/loading_screens.sui locale/en_us/local.override.sii locale/en_us/local.steam.sii locale/en_us/mails.sii \
           locale/en_us/paintjobs.sui locale/en_us/photoalbum.sui locale/en_us/tutorial_hints.sui locale/common.sui \
           locale/ui_colors.sui locale/fuel_price_strings.sui; do
    echo "  $f: $(cargo run -q --release -p specsheet-archive --example scs -- cat $f | grep -c imv_unf)"
  done
  echo "  /ui/ files using @@imv_unf_*@@: $(grep -rl imv_unf "$X/ui" | wc -l)"
  } >> "$OUT"
fi
wc -l "$OUT"
