use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use kurbo::{BezPath, CubicBez, PathEl, Point};
use norad::{Contour as NoradContour, ContourPoint, Name as NoradName, PointType};
use skrifa::MetadataProvider;
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::glyf::{GlyfLocaBuilder, Glyph, SimpleGlyph};
use write_fonts::tables::gsub::builders::LigatureSubBuilder;
use write_fonts::tables::gsub::{Gsub, SubstitutionLookup, SubstitutionLookupList};
use write_fonts::tables::head::{Flags, Head, MacStyle};
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::{Hmtx, LongMetric};
use write_fonts::tables::layout::builders::Builder as LayoutBuilder;
use write_fonts::tables::layout::{
    Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag, Script, ScriptList,
    ScriptRecord,
};
use write_fonts::tables::loca::LocaFormat;
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::{Os2, SelectionFlags};
use write_fonts::tables::post::Post;
use write_fonts::tables::variations::ivs_builder::VariationStoreBuilder;
use write_fonts::types::{FWord, Fixed, GlyphId, GlyphId16, LongDateTime, NameId, Tag, UfWord};
use write_fonts::FontBuilder;

use crate::model::{
    Anchor, CompiledFontResult, Contour, FontMetrics, FoundryProject, GlyphOutline, KerningPair,
    LigatureRule,
};
use crate::pathops::{add_extrema, normalize_winding, remove_overlap};

fn contours_to_quad_bezpath(contours: &[Contour]) -> BezPath {
    let mut quad_bp = BezPath::new();
    for c in contours {
        if c.anchors.len() < 2 {
            continue;
        }
        let bp = c.to_bezpath();
        let mut cur = Point::ORIGIN;
        let mut sub_start = Point::ORIGIN;
        for el in bp.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    quad_bp.move_to(p);
                    cur = p;
                    sub_start = p;
                }
                PathEl::LineTo(p) => {
                    quad_bp.line_to(p);
                    cur = p;
                }
                PathEl::QuadTo(q1, p) => {
                    quad_bp.quad_to(q1, p);
                    cur = p;
                }
                PathEl::CurveTo(c1, c2, p) => {
                    let cb = CubicBez::new(cur, c1, c2, p);
                    for (_, _, q) in cb.to_quads(0.5) {
                        quad_bp.quad_to(q.p1, q.p2);
                    }
                    cur = p;
                }
                PathEl::ClosePath => {
                    quad_bp.close_path();
                    cur = sub_start;
                }
            }
        }
    }
    quad_bp
}

fn build_notdef_glyph() -> Result<SimpleGlyph, String> {
    let mut bp = BezPath::new();
    // Outer CW
    bp.move_to((60.0, 0.0));
    bp.line_to((60.0, 720.0));
    bp.line_to((460.0, 720.0));
    bp.line_to((460.0, 0.0));
    bp.close_path();
    // Inner CCW
    bp.move_to((120.0, 60.0));
    bp.line_to((400.0, 60.0));
    bp.line_to((400.0, 660.0));
    bp.line_to((120.0, 660.0));
    bp.close_path();

    SimpleGlyph::from_bezpath(&bp).map_err(|e| format!("failed to build .notdef glyph: {e:?}"))
}

fn sanitize_post_glyph_name(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_graphic() && !c.is_control())
        .take(63)
        .collect();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

fn sanitize_ps_font_name(family: &str, style: &str) -> String {
    let combined = format!("{family}-{style}");
    let cleaned: String = combined
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect();
    if cleaned.is_empty() {
        "CosmicFoundry-Regular".to_string()
    } else {
        cleaned
    }
}

fn build_name_table(project: &FoundryProject) -> Name {
    let family = if project.family_name.trim().is_empty() {
        "Cosmic Foundry"
    } else {
        project.family_name.trim()
    };
    let style = if project.style_name.trim().is_empty() {
        "Regular"
    } else {
        project.style_name.trim()
    };
    let full_name = format!("{family} {style}");
    let ps_name = sanitize_ps_font_name(family, style);
    let unique_id = format!("CosmicFoundry;{ps_name};v{}", project.version.max(1));
    let version_str = format!("Version {}.000", project.version.max(1));

    let mut entries: Vec<(NameId, String)> = vec![
        (NameId::FAMILY_NAME, family.to_string()),
        (NameId::SUBFAMILY_NAME, style.to_string()),
        (NameId::UNIQUE_ID, unique_id),
        (NameId::FULL_NAME, full_name),
        (NameId::VERSION_STRING, version_str),
        (NameId::POSTSCRIPT_NAME, ps_name),
    ];
    if !project.designer.trim().is_empty() {
        entries.push((NameId::DESIGNER, project.designer.trim().to_string()));
    }
    if !project.license.trim().is_empty() {
        entries.push((
            NameId::LICENSE_DESCRIPTION,
            project.license.trim().to_string(),
        ));
    }

    let mut records = Vec::with_capacity(entries.len());
    for (name_id, val) in entries {
        records.push(NameRecord::new(3, 1, 0x0409, name_id, val.into()));
    }
    records.sort_by_key(|r| (r.platform_id, r.encoding_id, r.language_id, r.name_id));
    Name::new(records)
}

fn build_kern_table(kerning: &[KerningPair], cp_to_gid: &HashMap<u32, u16>) -> Option<Vec<u8>> {
    let mut pairs_map: BTreeMap<(u16, u16), i16> = BTreeMap::new();
    for pair in kerning {
        if pair.value == 0 {
            continue;
        }
        if let (Some(&lgid), Some(&rgid)) = (cp_to_gid.get(&pair.left), cp_to_gid.get(&pair.right))
        {
            pairs_map.insert((lgid, rgid), pair.value);
        }
    }
    if pairs_map.is_empty() {
        return None;
    }

    let n_pairs = pairs_map.len().min(10920);
    let subtable_len = 14 + n_pairs * 6;
    if subtable_len > u16::MAX as usize {
        return None;
    }

    let max_pow2 = if n_pairs == 0 {
        0
    } else {
        let mut p = 1usize;
        while p * 2 <= n_pairs {
            p *= 2;
        }
        p
    };
    let search_range = (max_pow2 * 6) as u16;
    let entry_selector = if max_pow2 == 0 {
        0
    } else {
        max_pow2.ilog2() as u16
    };
    let range_shift = ((n_pairs * 6).saturating_sub(search_range as usize)) as u16;

    let mut out = Vec::with_capacity(4 + subtable_len);
    // kern table header: version=0, nTables=1
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    // subtable header: version=0, length, coverage=1 (horizontal, format 0)
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(subtable_len as u16).to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    // format 0 header
    out.extend_from_slice(&(n_pairs as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    for ((lgid, rgid), val) in pairs_map.into_iter().take(n_pairs) {
        out.extend_from_slice(&lgid.to_be_bytes());
        out.extend_from_slice(&rgid.to_be_bytes());
        out.extend_from_slice(&val.to_be_bytes());
    }
    Some(out)
}

fn build_gsub_table(
    ligatures: &[LigatureRule],
    cp_to_gid: &HashMap<u32, u16>,
    key_to_gid: &HashMap<String, u16>,
    name_to_gid: &HashMap<String, u16>,
) -> Option<(Gsub, u16)> {
    if ligatures.is_empty() {
        return None;
    }

    let mut lig_builder = LigatureSubBuilder::default();
    let mut max_context = 0_usize;

    for rule in ligatures {
        if rule.components.len() < 2 {
            continue;
        }
        let mut comp_gids = Vec::with_capacity(rule.components.len());
        let mut all_resolved = true;
        for &cp in &rule.components {
            if let Some(&gid) = cp_to_gid.get(&cp) {
                comp_gids.push(GlyphId16::new(gid));
            } else {
                all_resolved = false;
                break;
            }
        }
        if !all_resolved || comp_gids.len() < 2 {
            continue;
        }

        let target_gid = key_to_gid
            .get(&rule.glyph_key)
            .or_else(|| name_to_gid.get(&rule.glyph_key))
            .or_else(|| name_to_gid.get(&rule.name))
            .copied();
        let Some(target_gid) = target_gid else {
            continue;
        };

        max_context = max_context.max(comp_gids.len());
        lig_builder.insert(comp_gids, GlyphId16::new(target_gid));
    }

    if lig_builder.is_empty() {
        return None;
    }

    let mut var_store = VariationStoreBuilder::new(0);
    let subtables = lig_builder.build(&mut var_store);
    if subtables.is_empty() {
        return None;
    }

    let lookup: SubstitutionLookup = Lookup::new(LookupFlag::empty(), subtables).into();
    let lookup_list = SubstitutionLookupList::new(vec![lookup]);
    let feature = Feature::new(None, vec![0]);
    let feature_list = FeatureList::new(vec![FeatureRecord::new(Tag::new(b"liga"), feature)]);
    let lang_sys = LangSys::new(vec![0]);
    let script = Script::new(Some(lang_sys), Vec::new());
    let script_list = ScriptList::new(vec![ScriptRecord::new(Tag::new(b"DFLT"), script)]);
    let gsub = Gsub::new(script_list, feature_list, lookup_list);

    Some((gsub, max_context.min(u16::MAX as usize) as u16))
}

/// Compiles a `FoundryProject` into a TrueType `.ttf` binary via `write-fonts`.
pub fn compile_ttf(
    project: &FoundryProject,
    clean_outlines: bool,
) -> Result<CompiledFontResult, String> {
    let upm = project.metrics.upm.max(16);

    let mut glyf_builder = GlyfLocaBuilder::new();
    let mut h_metrics: Vec<LongMetric> = Vec::new();
    let mut glyph_names: Vec<String> = Vec::new();
    let mut cmap_pairs: Vec<(char, GlyphId)> = Vec::new();
    let mut cp_to_gid: HashMap<u32, u16> = HashMap::new();
    let mut key_to_gid: HashMap<String, u16> = HashMap::new();
    let mut name_to_gid: HashMap<String, u16> = HashMap::new();

    // GID 0: .notdef
    let notdef_glyph = build_notdef_glyph()?;
    let notdef_bbox = notdef_glyph.bbox;
    glyf_builder
        .add_glyph(&notdef_glyph)
        .map_err(|e| format!("failed to add .notdef glyph: {e}"))?;
    h_metrics.push(LongMetric::new(520, 60));
    glyph_names.push(".notdef".to_string());
    let mut font_x_min = notdef_bbox.x_min;
    let mut font_y_min = notdef_bbox.y_min;
    let mut font_x_max = notdef_bbox.x_max;
    let mut font_y_max = notdef_bbox.y_max;
    let mut min_lsb = 60_i16;
    let mut min_rsb = 520 - notdef_bbox.x_max;
    let mut max_extent = 60 + (notdef_bbox.x_max - notdef_bbox.x_min);
    let mut max_adv = 520_u16;
    let mut max_points = 8_u16;
    let mut max_contours = 2_u16;
    let mut total_contours = 0_usize;
    let mut total_points = 0_usize;
    let mut adv_sum = 0_i64;
    let mut adv_count = 0_i64;

    // GID 1: space (0x20)
    let space_entry = project.glyphs.iter().find(|(_, g)| g.codepoint == 0x20);
    let space_adv = space_entry
        .map(|(_, g)| g.advance_width.clamp(0, u16::MAX as i32) as u16)
        .unwrap_or_else(|| (upm / 3).max(200));
    glyf_builder
        .add_glyph(&Glyph::Empty)
        .map_err(|e| format!("failed to add space glyph: {e}"))?;
    h_metrics.push(LongMetric::new(space_adv, 0));
    glyph_names.push("space".to_string());
    cmap_pairs.push((' ', GlyphId::new(1)));
    cp_to_gid.insert(0x20, 1);
    if let Some((space_key, space_glyph)) = space_entry {
        key_to_gid.insert(space_key.clone(), 1);
        name_to_gid.insert(space_glyph.name.clone(), 1);
    }
    name_to_gid.entry("space".to_string()).or_insert(1);
    max_adv = max_adv.max(space_adv);
    if space_adv > 0 {
        adv_sum += space_adv as i64;
        adv_count += 1;
    }

    // Remaining glyphs sorted by codepoint, then name, then key.
    let mut sorted_glyphs: Vec<(&String, &GlyphOutline)> = project
        .glyphs
        .iter()
        .filter(|(_, g)| g.codepoint != 0x20 && g.name != ".notdef")
        .collect();
    sorted_glyphs.sort_by(|(ka, a), (kb, b)| {
        a.codepoint
            .cmp(&b.codepoint)
            .then(a.name.cmp(&b.name))
            .then(ka.cmp(kb))
    });

    let mut seen_codepoints = HashSet::new();
    seen_codepoints.insert(0x20_u32);

    for (glyph_key, glyph) in sorted_glyphs {
        let gid_num = h_metrics.len();
        if gid_num >= u16::MAX as usize {
            break;
        }
        let gid = GlyphId::new(gid_num as u32);

        let contours = if clean_outlines && !glyph.contours.is_empty() {
            let fused = remove_overlap(&glyph.contours);
            let with_extrema = add_extrema(&fused);
            normalize_winding(&with_extrema, true)
        } else {
            glyph.contours.clone()
        };

        let quad_bp = contours_to_quad_bezpath(&contours);
        let built_glyph = if quad_bp.elements().is_empty() {
            Glyph::Empty
        } else {
            let sg = SimpleGlyph::from_bezpath(&quad_bp)
                .map_err(|e| format!("failed to build glyph '{}': {e:?}", glyph.name))?;
            Glyph::from(sg)
        };

        let adv = glyph.advance_width.clamp(0, u16::MAX as i32) as u16;
        let lsb = match &built_glyph {
            Glyph::Simple(sg) => sg.bbox.x_min,
            Glyph::Composite(cg) => cg.bbox.x_min,
            Glyph::Empty => glyph.lsb.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        };

        if let Glyph::Simple(sg) = &built_glyph {
            let n_contours = sg.contours.len();
            let n_points: usize = sg.contours.iter().map(|c| c.len()).sum();
            total_contours += n_contours;
            total_points += n_points;
            max_contours = max_contours.max(n_contours.min(u16::MAX as usize) as u16);
            max_points = max_points.max(n_points.min(u16::MAX as usize) as u16);

            let bb = sg.bbox;
            let rsb = (adv as i32 - bb.x_max as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let ext = (lsb as i32 + (bb.x_max as i32 - bb.x_min as i32))
                .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            font_x_min = font_x_min.min(bb.x_min);
            font_y_min = font_y_min.min(bb.y_min);
            font_x_max = font_x_max.max(bb.x_max);
            font_y_max = font_y_max.max(bb.y_max);
            min_lsb = min_lsb.min(lsb);
            min_rsb = min_rsb.min(rsb);
            max_extent = max_extent.max(ext);
        }

        glyf_builder
            .add_glyph(&built_glyph)
            .map_err(|e| format!("failed to compile glyph '{}': {e}", glyph.name))?;
        h_metrics.push(LongMetric::new(adv, lsb));
        let sanitized_name = sanitize_post_glyph_name(&glyph.name, &format!("glyph{gid_num}"));
        glyph_names.push(sanitized_name.clone());

        key_to_gid.insert(glyph_key.clone(), gid_num as u16);
        if !glyph.name.is_empty() {
            name_to_gid
                .entry(glyph.name.clone())
                .or_insert(gid_num as u16);
        }
        name_to_gid.entry(sanitized_name).or_insert(gid_num as u16);

        max_adv = max_adv.max(adv);
        if adv > 0 {
            adv_sum += adv as i64;
            adv_count += 1;
        }

        if glyph.codepoint > 0 && seen_codepoints.insert(glyph.codepoint) {
            if let Some(ch) = char::from_u32(glyph.codepoint) {
                cmap_pairs.push((ch, gid));
                cp_to_gid.insert(glyph.codepoint, gid_num as u16);
            }
        }
    }

    let gsub_built = build_gsub_table(&project.ligatures, &cp_to_gid, &key_to_gid, &name_to_gid);
    let max_context = gsub_built.as_ref().map(|(_, ctx)| *ctx).unwrap_or(0);

    let num_glyphs = h_metrics.len() as u16;
    let (glyf, loca, loca_format) = glyf_builder.build();

    let style_lower = project.style_name.to_ascii_lowercase();
    let is_bold = style_lower.contains("bold");
    let is_italic = style_lower.contains("italic") || style_lower.contains("oblique");
    let mut mac_style = MacStyle::empty();
    if is_bold {
        mac_style |= MacStyle::BOLD;
    }
    if is_italic {
        mac_style |= MacStyle::ITALIC;
    }

    let head = Head {
        font_revision: Fixed::from_f64(1.0),
        checksum_adjustment: 0,
        flags: Flags::BASELINE_AT_Y_0 | Flags::LSB_AT_X_0,
        units_per_em: upm,
        created: LongDateTime::new(3_800_000_000),
        modified: LongDateTime::new(3_800_000_000),
        x_min: font_x_min,
        y_min: font_y_min,
        x_max: font_x_max,
        y_max: font_y_max,
        mac_style,
        lowest_rec_ppem: 8,
        index_to_loc_format: match loca_format {
            LocaFormat::Short => 0,
            LocaFormat::Long => 1,
        },
        ..Default::default()
    };

    let hhea = Hhea {
        ascender: FWord::new(project.metrics.ascender),
        descender: FWord::new(project.metrics.descender),
        line_gap: FWord::new(project.metrics.line_gap),
        advance_width_max: UfWord::new(max_adv),
        min_left_side_bearing: FWord::new(min_lsb),
        min_right_side_bearing: FWord::new(min_rsb),
        x_max_extent: FWord::new(max_extent),
        caret_slope_rise: 1,
        caret_slope_run: 0,
        caret_offset: 0,
        number_of_h_metrics: num_glyphs,
    };

    let hmtx = Hmtx::new(h_metrics, Vec::new());

    let maxp = Maxp {
        num_glyphs,
        max_points: Some(max_points),
        max_contours: Some(max_contours),
        max_composite_points: Some(0),
        max_composite_contours: Some(0),
        max_zones: Some(1),
        max_twilight_points: Some(0),
        max_storage: Some(0),
        max_function_defs: Some(0),
        max_instruction_defs: Some(0),
        max_stack_elements: Some(0),
        max_size_of_instructions: Some(0),
        max_component_elements: Some(0),
        max_component_depth: Some(0),
    };

    let avg_char_w = if adv_count > 0 {
        (adv_sum / adv_count).clamp(0, i16::MAX as i64) as i16
    } else {
        (upm / 2) as i16
    };
    let first_cp = cmap_pairs
        .iter()
        .map(|(c, _)| (*c as u32).min(u16::MAX as u32) as u16)
        .min()
        .unwrap_or(0x20);
    let last_cp = cmap_pairs
        .iter()
        .map(|(c, _)| (*c as u32).min(u16::MAX as u32) as u16)
        .max()
        .unwrap_or(0x20);

    let mut fs_selection = SelectionFlags::USE_TYPO_METRICS;
    if is_bold {
        fs_selection |= SelectionFlags::BOLD;
    }
    if is_italic {
        fs_selection |= SelectionFlags::ITALIC;
    }
    if !is_bold && !is_italic {
        fs_selection |= SelectionFlags::REGULAR;
    }

    let win_ascent = (project.metrics.ascender.max(font_y_max)).max(0) as u16;
    let win_descent = (-project.metrics.descender.min(font_y_min)).max(0) as u16;

    let os2 = Os2 {
        x_avg_char_width: avg_char_w,
        us_weight_class: if is_bold { 700 } else { 400 },
        us_width_class: 5,
        fs_type: 0,
        y_subscript_x_size: (upm as f64 * 0.65) as i16,
        y_subscript_y_size: (upm as f64 * 0.60) as i16,
        y_subscript_x_offset: 0,
        y_subscript_y_offset: (upm as f64 * 0.14) as i16,
        y_superscript_x_size: (upm as f64 * 0.65) as i16,
        y_superscript_y_size: (upm as f64 * 0.60) as i16,
        y_superscript_x_offset: 0,
        y_superscript_y_offset: (upm as f64 * 0.48) as i16,
        y_strikeout_size: (upm as f64 * 0.05).max(1.0) as i16,
        y_strikeout_position: (upm as f64 * 0.26) as i16,
        s_family_class: 0,
        panose_10: [2, 0, 5, 3, 0, 0, 0, 0, 0, 0],
        ul_unicode_range_1: 1,
        ul_unicode_range_2: 0,
        ul_unicode_range_3: 0,
        ul_unicode_range_4: 0,
        ach_vend_id: Tag::new(b"CSMC"),
        fs_selection,
        us_first_char_index: first_cp,
        us_last_char_index: last_cp,
        s_typo_ascender: project.metrics.ascender,
        s_typo_descender: project.metrics.descender,
        s_typo_line_gap: project.metrics.line_gap,
        us_win_ascent: win_ascent,
        us_win_descent: win_descent,
        ul_code_page_range_1: Some(1),
        ul_code_page_range_2: Some(0),
        sx_height: Some(project.metrics.x_height),
        s_cap_height: Some(project.metrics.cap_height),
        us_default_char: Some(0),
        us_break_char: Some(0x20),
        us_max_context: Some(max_context),
        us_lower_optical_point_size: None,
        us_upper_optical_point_size: None,
    };

    let post = Post::new_v2(glyph_names.iter().map(String::as_str));
    let name_table = build_name_table(project);
    let cmap =
        Cmap::from_mappings(cmap_pairs).map_err(|e| format!("failed to build cmap table: {e}"))?;

    let mut builder = FontBuilder::new();
    builder
        .add_table(&head)
        .map_err(|e| format!("failed to add head: {e}"))?;
    builder
        .add_table(&hhea)
        .map_err(|e| format!("failed to add hhea: {e}"))?;
    builder
        .add_table(&maxp)
        .map_err(|e| format!("failed to add maxp: {e}"))?;
    builder
        .add_table(&os2)
        .map_err(|e| format!("failed to add OS/2: {e}"))?;
    builder
        .add_table(&hmtx)
        .map_err(|e| format!("failed to add hmtx: {e}"))?;
    builder
        .add_table(&cmap)
        .map_err(|e| format!("failed to add cmap: {e}"))?;
    builder
        .add_table(&loca)
        .map_err(|e| format!("failed to add loca: {e}"))?;
    builder
        .add_table(&glyf)
        .map_err(|e| format!("failed to add glyf: {e}"))?;
    builder
        .add_table(&name_table)
        .map_err(|e| format!("failed to add name: {e}"))?;
    builder
        .add_table(&post)
        .map_err(|e| format!("failed to add post: {e}"))?;

    if let Some(kern_bytes) = build_kern_table(&project.kerning, &cp_to_gid) {
        builder.add_raw(Tag::new(b"kern"), kern_bytes);
    }
    if let Some((gsub, _)) = gsub_built {
        builder
            .add_table(&gsub)
            .map_err(|e| format!("failed to add GSUB: {e}"))?;
    }

    let ttf_bytes = builder.build();

    Ok(CompiledFontResult {
        ttf_bytes,
        glyph_count: num_glyphs as usize,
        total_contours,
        total_points,
    })
}

/// Converts a compiled SFNT (`.ttf`/`.otf`) binary into WOFF 1.0 format.
pub fn compile_woff(ttf_bytes: &[u8]) -> Result<Vec<u8>, String> {
    cosmic_core::woff::sfnt_to_woff(ttf_bytes)
}

fn valid_norad_name(raw: &str, fallback: &str) -> Result<NoradName, String> {
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).collect();
    if !sanitized.is_empty() {
        if let Ok(name) = NoradName::new(&sanitized) {
            return Ok(name);
        }
    }
    NoradName::new(fallback).map_err(|e| format!("invalid glyph name '{fallback}': {e}"))
}

fn contour_to_norad(c: &Contour) -> NoradContour {
    let n = c.anchors.len();
    if n == 0 {
        return NoradContour::new(Vec::new(), None);
    }

    let mut pts = Vec::with_capacity(n * 3);
    if c.closed {
        let last = &c.anchors[n - 1];
        let first = &c.anchors[0];
        let closing_curved = last.has_h_out() || first.has_h_in();
        pts.push(ContourPoint::new(
            first.x,
            first.y,
            if closing_curved {
                PointType::Curve
            } else {
                PointType::Line
            },
            first.smooth,
            None,
            None,
        ));
        for i in 1..n {
            let prev = &c.anchors[i - 1];
            let curr = &c.anchors[i];
            if prev.has_h_out() || curr.has_h_in() {
                pts.push(ContourPoint::new(
                    prev.x + prev.h_out[0],
                    prev.y + prev.h_out[1],
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                ));
                pts.push(ContourPoint::new(
                    curr.x + curr.h_in[0],
                    curr.y + curr.h_in[1],
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                ));
                pts.push(ContourPoint::new(
                    curr.x,
                    curr.y,
                    PointType::Curve,
                    curr.smooth,
                    None,
                    None,
                ));
            } else {
                pts.push(ContourPoint::new(
                    curr.x,
                    curr.y,
                    PointType::Line,
                    curr.smooth,
                    None,
                    None,
                ));
            }
        }
        if closing_curved {
            pts.push(ContourPoint::new(
                last.x + last.h_out[0],
                last.y + last.h_out[1],
                PointType::OffCurve,
                false,
                None,
                None,
            ));
            pts.push(ContourPoint::new(
                first.x + first.h_in[0],
                first.y + first.h_in[1],
                PointType::OffCurve,
                false,
                None,
                None,
            ));
        }
    } else {
        let first = &c.anchors[0];
        pts.push(ContourPoint::new(
            first.x,
            first.y,
            PointType::Move,
            first.smooth,
            None,
            None,
        ));
        for i in 1..n {
            let prev = &c.anchors[i - 1];
            let curr = &c.anchors[i];
            if prev.has_h_out() || curr.has_h_in() {
                pts.push(ContourPoint::new(
                    prev.x + prev.h_out[0],
                    prev.y + prev.h_out[1],
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                ));
                pts.push(ContourPoint::new(
                    curr.x + curr.h_in[0],
                    curr.y + curr.h_in[1],
                    PointType::OffCurve,
                    false,
                    None,
                    None,
                ));
                pts.push(ContourPoint::new(
                    curr.x,
                    curr.y,
                    PointType::Curve,
                    curr.smooth,
                    None,
                    None,
                ));
            } else {
                pts.push(ContourPoint::new(
                    curr.x,
                    curr.y,
                    PointType::Line,
                    curr.smooth,
                    None,
                    None,
                ));
            }
        }
    }

    NoradContour::new(pts, None)
}

fn norad_contour_to_model(nc: &NoradContour) -> Option<Contour> {
    if nc.points.is_empty() {
        return None;
    }

    let is_open = nc.points[0].typ == PointType::Move;
    let mut pts: Vec<&ContourPoint> = nc.points.iter().collect();

    if !is_open {
        let first_on = pts.iter().position(|p| p.typ != PointType::OffCurve)?;
        pts.rotate_left(first_on);
    }

    let mut anchors: Vec<Anchor> = Vec::new();
    let mut offcurves: Vec<&ContourPoint> = Vec::new();

    let push_segment =
        |anchors: &mut Vec<Anchor>, offs: &[&ContourPoint], target: &ContourPoint| {
            let mut next = Anchor {
                x: target.x,
                y: target.y,
                h_in: [0.0, 0.0],
                h_out: [0.0, 0.0],
                smooth: target.smooth,
            };
            if let Some(prev) = anchors.last_mut() {
                match offs.len() {
                    0 => {}
                    1 => {
                        let q1 = offs[0];
                        prev.h_out = [(2.0 / 3.0) * (q1.x - prev.x), (2.0 / 3.0) * (q1.y - prev.y)];
                        next.h_in = [(2.0 / 3.0) * (q1.x - next.x), (2.0 / 3.0) * (q1.y - next.y)];
                    }
                    _ => {
                        let c1 = offs[0];
                        let c2 = offs[offs.len() - 1];
                        prev.h_out = [c1.x - prev.x, c1.y - prev.y];
                        next.h_in = [c2.x - next.x, c2.y - next.y];
                    }
                }
            }
            anchors.push(next);
        };

    for (idx, &p) in pts.iter().enumerate() {
        if p.typ == PointType::OffCurve {
            offcurves.push(p);
            continue;
        }
        if idx == 0 {
            anchors.push(Anchor {
                x: p.x,
                y: p.y,
                h_in: [0.0, 0.0],
                h_out: [0.0, 0.0],
                smooth: p.smooth,
            });
            offcurves.clear();
        } else {
            push_segment(&mut anchors, &offcurves, p);
            offcurves.clear();
        }
    }

    if !is_open && !offcurves.is_empty() && !anchors.is_empty() {
        let last_idx = anchors.len() - 1;
        let last_x = anchors[last_idx].x;
        let last_y = anchors[last_idx].y;
        let first_x = anchors[0].x;
        let first_y = anchors[0].y;
        match offcurves.len() {
            1 => {
                let q1 = offcurves[0];
                anchors[last_idx].h_out =
                    [(2.0 / 3.0) * (q1.x - last_x), (2.0 / 3.0) * (q1.y - last_y)];
                anchors[0].h_in = [
                    (2.0 / 3.0) * (q1.x - first_x),
                    (2.0 / 3.0) * (q1.y - first_y),
                ];
            }
            _ => {
                let c1 = offcurves[0];
                let c2 = offcurves[offcurves.len() - 1];
                anchors[last_idx].h_out = [c1.x - last_x, c1.y - last_y];
                anchors[0].h_in = [c2.x - first_x, c2.y - first_y];
            }
        }
    }

    if anchors.is_empty() {
        None
    } else {
        Some(Contour {
            anchors,
            closed: !is_open,
        })
    }
}

/// Exports a `FoundryProject` as a UFO v3 directory via `norad`.
pub fn export_ufo(project: &FoundryProject, dest_dir: &Path) -> Result<(), String> {
    let mut font = norad::Font::new();
    font.font_info.family_name = Some(project.family_name.clone());
    font.font_info.style_name = Some(project.style_name.clone());
    font.font_info.units_per_em = Some((project.metrics.upm.max(16) as u32).into());
    font.font_info.ascender = Some(project.metrics.ascender as f64);
    font.font_info.descender = Some(project.metrics.descender as f64);
    font.font_info.x_height = Some(project.metrics.x_height as f64);
    font.font_info.cap_height = Some(project.metrics.cap_height as f64);
    font.font_info.open_type_hhea_line_gap = Some(project.metrics.line_gap as i32);
    if !project.designer.trim().is_empty() {
        font.font_info.open_type_name_designer = Some(project.designer.clone());
    }
    if !project.license.trim().is_empty() {
        font.font_info.open_type_name_license = Some(project.license.clone());
    }

    let mut used_names = HashSet::new();
    let mut cp_to_name: HashMap<u32, NoradName> = HashMap::new();

    {
        let layer = font.layers.default_layer_mut();
        for (idx, glyph) in project.glyphs.values().enumerate() {
            let fallback = if glyph.codepoint > 0 {
                format!("uni{:04X}", glyph.codepoint)
            } else {
                format!("glyph{idx}")
            };
            let base_name = valid_norad_name(&glyph.name, &fallback)?;
            let final_name = if used_names.insert(base_name.as_str().to_string()) {
                base_name
            } else {
                let unique = format!("{}_{idx}", base_name.as_str());
                let n = valid_norad_name(&unique, &format!("glyph_{idx}"))?;
                used_names.insert(n.as_str().to_string());
                n
            };

            let mut ng = norad::Glyph::new(final_name.as_str());
            ng.width = glyph.advance_width as f64;
            if glyph.codepoint > 0 {
                if let Some(ch) = char::from_u32(glyph.codepoint) {
                    ng.codepoints.insert(ch);
                }
                cp_to_name.insert(glyph.codepoint, final_name.clone());
            }
            for c in &glyph.contours {
                if !c.anchors.is_empty() {
                    ng.contours.push(contour_to_norad(c));
                }
            }
            layer.insert_glyph(ng);
        }
    }

    for pair in &project.kerning {
        if let (Some(left), Some(right)) = (cp_to_name.get(&pair.left), cp_to_name.get(&pair.right))
        {
            font.kerning
                .entry(left.clone())
                .or_default()
                .insert(right.clone(), pair.value as f64);
        }
    }

    font.save(dest_dir)
        .map_err(|e| format!("failed to save UFO to {}: {e}", dest_dir.display()))
}

/// Imports a UFO directory into a `FoundryProject` via `norad`.
pub fn import_ufo(src_dir: &Path) -> Result<FoundryProject, String> {
    let font = norad::Font::load(src_dir)
        .map_err(|e| format!("failed to load UFO from {}: {e}", src_dir.display()))?;

    let info = &font.font_info;
    let family_name = info
        .family_name
        .clone()
        .unwrap_or_else(|| "UFO Font".to_string());
    let style_name = info
        .style_name
        .clone()
        .unwrap_or_else(|| "Regular".to_string());
    let designer = info.open_type_name_designer.clone().unwrap_or_default();
    let license = info.open_type_name_license.clone().unwrap_or_default();
    let upm = info
        .units_per_em
        .map(|v| v.as_f64().round().clamp(16.0, 16384.0) as u16)
        .unwrap_or(1000);
    let ascender = info
        .ascender
        .map(|v| v.round() as i16)
        .unwrap_or((upm as f64 * 0.8) as i16);
    let descender = info
        .descender
        .map(|v| v.round() as i16)
        .unwrap_or(-((upm as f64 * 0.2) as i16));
    let x_height = info
        .x_height
        .map(|v| v.round() as i16)
        .unwrap_or((upm as f64 * 0.52) as i16);
    let cap_height = info
        .cap_height
        .map(|v| v.round() as i16)
        .unwrap_or((upm as f64 * 0.72) as i16);
    let line_gap = info.open_type_hhea_line_gap.map(|v| v as i16).unwrap_or(0);

    let mut glyphs = BTreeMap::new();
    let mut name_to_cp: HashMap<String, u32> = HashMap::new();

    for ng in font.layers.default_layer().iter() {
        let codepoint = ng.codepoints.iter().next().map(|c| c as u32).unwrap_or(0);
        let name = ng.name().as_str().to_string();
        if codepoint > 0 {
            name_to_cp.insert(name.clone(), codepoint);
        }

        let contours: Vec<Contour> = ng
            .contours
            .iter()
            .filter_map(norad_contour_to_model)
            .collect();
        let lsb = contours
            .first()
            .and_then(|c| c.anchors.first())
            .map(|a| a.x.round() as i32)
            .unwrap_or(0);

        let key = if codepoint > 0 {
            char::from_u32(codepoint)
                .map(|c| c.to_string())
                .unwrap_or_else(|| name.clone())
        } else {
            name.clone()
        };

        glyphs.insert(
            key,
            GlyphOutline {
                codepoint,
                name,
                advance_width: ng.width.round() as i32,
                lsb,
                contours,
            },
        );
    }

    let mut kerning = Vec::new();
    for (left_name, rights) in &font.kerning {
        let Some(&left_cp) = name_to_cp.get(left_name.as_str()) else {
            continue;
        };
        for (right_name, &val) in rights {
            let Some(&right_cp) = name_to_cp.get(right_name.as_str()) else {
                continue;
            };
            kerning.push(KerningPair {
                left: left_cp,
                right: right_cp,
                value: val.round() as i16,
            });
        }
    }

    Ok(FoundryProject {
        version: 1,
        family_name,
        style_name,
        designer,
        license,
        metrics: FontMetrics {
            upm,
            ascender,
            descender,
            x_height,
            cap_height,
            line_gap,
        },
        glyphs,
        kerning,
        ligatures: Vec::new(),
        linked_metrics: Vec::new(),
        anchors: BTreeMap::new(),
    })
}

/// Subsets a font file to only the glyphs required by `text` (plus `.notdef`),
/// optionally instancing variable axes.
pub fn subset_font(path: &Path, text: &str, axes: &[(String, f32)]) -> Result<Vec<u8>, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let font = skrifa::FontRef::from_index(&bytes, 0)
        .map_err(|e| format!("failed to parse font {}: {e}", path.display()))?;

    let charmap = font.charmap();
    let mut remapper = subsetter::GlyphRemapper::new();
    remapper.remap(0);
    for ch in text.chars() {
        if let Some(gid) = charmap.map(ch as u32) {
            if let Ok(gid16) = u16::try_from(gid.to_u32()) {
                remapper.remap(gid16);
            }
        }
    }

    if axes.is_empty() {
        subsetter::subset(&bytes, 0, &remapper).map_err(|e| format!("subsetter failed: {e:?}"))
    } else {
        let mut coords = Vec::with_capacity(axes.len());
        for (tag_str, val) in axes {
            let tag = subsetter::Tag::from_str(tag_str)
                .map_err(|_| format!("invalid OpenType variation axis tag '{tag_str}'"))?;
            coords.push((tag, *val));
        }
        subsetter::subset_with_variations(&bytes, 0, &coords, &remapper)
            .map_err(|e| format!("subsetter variation instancing failed: {e:?}"))
    }
}
