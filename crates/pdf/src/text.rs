//! Composed text → PDF text objects with embedded (subsetted) fonts.
//!
//! Each line is split into runs of glyphs sharing face, style, scale and baseline offset. A run is
//! drawn with one `draw_glyphs` call: the run origin, horizontal scale and skew go into the
//! transform, glyph advances are taken from the composed x positions (so justification, tracking
//! and kerning are exact), and every glyph carries the story text it came from so viewers can
//! select, search and copy it.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use designcraft_compose::{ComposedStory, FrameText, PlacedGlyph};
use designcraft_fonts::{FontDb, FontFace};
use designcraft_geom::kurbo::Shape as _;
use designcraft_geom::{Affine, BezPath, Rect};
use krilla::configure::PdfVersion;
use krilla::geom::Point;
use krilla::paint::Stroke;
use krilla::surface::Surface;
use krilla::tagging::{Artifact, ArtifactType, ContentTag, SpanTag};
use krilla::text::{Font, GlyphId, KrillaGlyph};

use crate::export::{Exporter, solid_fill, tf, to_path};

/// Private-use characters DesignCraft stores for markers (page numbers, section markers, breaks).
fn is_marker(c: char) -> bool {
    ('\u{E000}'..='\u{E1FF}').contains(&c)
}

/// Can `b` join the run `a` is in? A missing glyph (.notdef) is a run of its own.
fn same_run(a: &PlacedGlyph, b: &PlacedGlyph) -> bool {
    b.visible
        && a.gid != 0
        && b.gid != 0
        && a.face.id() == b.face.id()
        && a.style == b.style
        && (a.sx - b.sx).abs() < 1e-9
        && (a.sy - b.sy).abs() < 1e-9
        && (a.y - b.y).abs() < 1e-6
}

impl Exporter<'_> {
    pub(crate) fn font(&mut self, face: &FontFace) -> Option<Font> {
        self.fonts
            .entry(face.id())
            .or_insert_with(|| {
                if face.is_variable() {
                    // A named instance: krilla embeds the font at these axis settings.
                    let coords: Vec<(krilla::text::Tag, f32)> = face.coords.iter().map(|(t, v)| (krilla::text::Tag::new(t), *v)).collect();
                    Font::new_variable(face.data().to_vec().into(), face.index(), &coords)
                } else {
                    Font::new(face.data().to_vec().into(), face.index())
                }
            })
            .clone()
    }

    fn reverse_cmap(&mut self, face: &FontFace) -> Arc<HashMap<u32, char>> {
        self.reverse_cmaps
            .entry(face.id())
            .or_insert_with(|| {
                let mut m = HashMap::new();
                // Sorted by code point: the lowest code point wins (`-` before U+2010).
                for (c, g) in face.chars() {
                    m.entry(g).or_insert(c);
                }
                Arc::new(m)
            })
            .clone()
    }

    /// Draw `f` as an artifact while a story is being tagged (nested text isn't tagged).
    fn as_artifact(&mut self, s: &mut Surface, f: impl FnOnce(&mut Self, &mut Surface)) {
        match self.tag_story.take() {
            Some(sid) => {
                s.start_tagged(ContentTag::Artifact(Artifact::new(ArtifactType::Other, None)));
                f(self, s);
                s.end_tagged();
                self.tag_story = Some(sid);
            }
            None => f(self, s),
        }
    }

    /// A glyph run, tagged under its paragraph while a story is being tagged.
    fn tagged_run(&mut self, s: &mut Surface, cs: &ComposedStory, gs: &[PlacedGlyph], baseline: f64, story: &str) {
        match self.tag_story {
            Some(sid) => {
                let pi = self.doc.story(sid).map_or(0, |st| st.para_at(gs[0].byte.min(st.len())));
                let id = s.start_tagged(ContentTag::Span(SpanTag::empty()));
                self.run(s, cs, gs, baseline, story);
                s.end_tagged();
                self.para_tags.entry((sid, pi)).or_default().push(id);
            }
            None => self.run(s, cs, gs, baseline, story),
        }
    }

    pub(crate) fn frame_text(&mut self, s: &mut Surface, cs: &ComposedStory, ft: &FrameText, story: &str) {
        // Paragraph shading and rules under the text.
        self.as_artifact(s, |me, s| {
            for d in &ft.decos {
                if let Some(c) = me.swatch_color(&d.color, d.tint) {
                    me.fill_rect(s, d.rect, c);
                }
            }
            for b in &ft.svg_bullets {
                if let Some(c) = me.swatch_color(&b.color, b.tint) {
                    me.svg_bullet(s, b, c);
                }
            }
            if !ft.tables.is_empty() {
                me.tables(s, ft);
            }
        });
        let mut deco: Vec<(designcraft_compose::Rule, Rect)> = Vec::new();
        for l in &ft.lines {
            let gs = &l.glyphs;
            let mut i = 0;
            while i < gs.len() {
                let g = &gs[i];
                if !g.visible || g.sx <= 0.0 || g.sy <= 0.0 {
                    i += 1;
                    continue;
                }
                // Vertical type: an upright glyph turns back a quarter about its em box centre.
                if ft.vertical
                    && let Some(turn) = g.vertical_xf(l.baseline)
                {
                    s.push_transform(&crate::export::tf(turn));
                    self.tagged_run(s, cs, &gs[i..i + 1], l.baseline, story);
                    s.pop();
                    i += 1;
                    continue;
                }
                let mut j = i + 1;
                while j < gs.len() && same_run(g, &gs[j]) && !(ft.vertical && (gs[j].upright || gs[j].tcy.is_some())) {
                    j += 1;
                }
                self.tagged_run(s, cs, &gs[i..j], l.baseline, story);
                i = j;
            }
            // Underline / strikethrough, merged per style along the line.
            for g in gs.iter().filter(|g| g.visible) {
                let st = &cs.styles[g.style as usize];
                for (rule, r) in st.rules(g.x, g.x + g.adv, l.baseline) {
                    match deco.last_mut() {
                        Some((last_rule, last)) if *last_rule == *rule && (last.y0 - r.y0).abs() < 1e-6 && (r.x0 - last.x1).abs() < 0.5 => {
                            last.x1 = r.x1
                        }
                        _ => deco.push((rule.clone(), r)),
                    }
                }
            }
        }
        self.as_artifact(s, |me, s| {
            for (rule, r) in deco {
                if let Some(c) = me.swatch_color(&rule.color, rule.tint) {
                    me.fill_rect(s, r, c);
                }
            }
            for n in &ft.notes {
                if let Some(nft) = n.text.frames.first() {
                    s.push_transform(&tf(Affine::translate(n.origin.to_vec2())));
                    me.frame_text(s, &n.text, nft, &n.source);
                    s.pop();
                }
            }
        });
    }

    /// Table fragments: cell fills, cell text (real text), edges and the border.
    fn tables(&mut self, s: &mut Surface, ft: &FrameText) {
        for t in &ft.tables {
            for c in &t.cells {
                if let Some((sw, tint)) = &c.fill
                    && let Some(col) = self.swatch_color(sw, *tint)
                {
                    self.fill_rect(s, c.rect, col);
                }
            }
            for c in &t.cells {
                if let Some(g) = &c.graphic {
                    if let Some(clip) = to_path(&c.clip.to_path(0.1)) {
                        s.push_clip_path(&clip, &krilla::paint::FillRule::NonZero);
                        s.push_transform(&tf(Affine::translate((c.clip.x0, c.clip.y0))));
                        self.graphic(s, g);
                        s.pop();
                        s.pop();
                    }
                } else if let Some(cft) = c.text.frames.first() {
                    s.push_transform(&tf(Affine::translate(c.origin.to_vec2())));
                    self.frame_text(s, &c.text, cft, &c.source);
                    s.pop();
                }
            }
            for seg in &t.strokes {
                let st = &seg.stroke;
                let Some(col) = self.swatch_color(&st.color, st.tint) else { continue };
                let (cap, dash) = match &st.kind {
                    designcraft_doc::StrokeType::Dashed { pattern } if pattern.iter().any(|v| *v > 0.0) => {
                        let mut pat: Vec<f32> = pattern.iter().map(|v| v.max(0.0) as f32).collect();
                        if pat.len() % 2 == 1 {
                            pat.extend(pat.clone());
                        }
                        (krilla::paint::LineCap::Butt, Some(krilla::paint::StrokeDash { array: pat, offset: 0.0 }))
                    }
                    designcraft_doc::StrokeType::Dotted => {
                        (krilla::paint::LineCap::Round, Some(krilla::paint::StrokeDash { array: vec![0.0, (st.weight * 2.0) as f32], offset: 0.0 }))
                    }
                    _ => (krilla::paint::LineCap::Square, None),
                };
                let mut bp = BezPath::new();
                bp.move_to(seg.a);
                bp.line_to(seg.b);
                let Some(p) = to_path(&bp) else { continue };
                s.set_fill(None);
                s.set_stroke(Some(Stroke { paint: col.into(), width: st.weight as f32, line_cap: cap, dash, ..Default::default() }));
                s.draw_path(&p);
                s.set_stroke(None);
            }
        }
    }

    fn fill_rect(&self, s: &mut Surface, r: Rect, c: krilla::color::Color) {
        if let Some(p) = to_path(&r.to_path(0.1)) {
            s.set_stroke(None);
            s.set_fill(Some(solid_fill(c, 1.0)));
            s.draw_path(&p);
            s.set_fill(None);
        }
    }

    fn svg_bullet(&self, s: &mut Surface, b: &designcraft_compose::SvgBullet, c: krilla::color::Color) {
        let r = b.rect;
        let str_svg = b.svg.trim();
        if str_svg.contains("<svg") {
            let hex = self.doc.resolve_color(&b.color, b.tint).map(|col| col.to_hex()).unwrap_or_else(|| "#000000".to_string());
            let mut svg_src = if str_svg.contains("currentColor") { str_svg.replace("currentColor", &hex) } else { str_svg.to_string() };
            if !svg_src.contains("xmlns=") {
                svg_src = svg_src.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1);
            }
            if let Some(tree) = designcraft_images::svg_tree(svg_src.as_bytes()) {
                use krilla_svg::SurfaceExt;
                if let Some(size) = krilla::geom::Size::from_wh(r.width().max(1e-3) as f32, r.height().max(1e-3) as f32) {
                    s.push_transform(&tf(Affine::translate((r.x0, r.y0))));
                    let _ = s.draw_svg(&tree, size, krilla_svg::SvgSettings::default());
                    s.pop();
                    return;
                }
            }
        }
        if str_svg.starts_with("data:application/pdf") || str_svg.starts_with("data:application/x-pdf") || str_svg.starts_with("%PDF-") {
            let bytes = if str_svg.starts_with("%PDF-") {
                str_svg.as_bytes().to_vec()
            } else {
                let b64 = if let Some((_, b64_part)) = str_svg.split_once(";base64,") {
                    b64_part
                } else if let Some((_, b64_part)) = str_svg.split_once(',') {
                    b64_part
                } else {
                    str_svg
                };
                decode_base64(b64)
            };
            if let Some(size) = krilla::geom::Size::from_wh(r.width().max(1e-3) as f32, r.height().max(1e-3) as f32) {
                let arc_bytes = std::sync::Arc::new(bytes);
                let target_header = match self.version {
                    PdfVersion::Pdf14 => b"%PDF-1.4",
                    PdfVersion::Pdf15 => b"%PDF-1.5",
                    PdfVersion::Pdf16 => b"%PDF-1.6",
                    _ => b"%PDF-1.7",
                };
                let max_v = match self.version {
                    PdfVersion::Pdf14 => hayro_syntax::PdfVersion::Pdf14,
                    PdfVersion::Pdf15 => hayro_syntax::PdfVersion::Pdf15,
                    PdfVersion::Pdf16 => hayro_syntax::PdfVersion::Pdf16,
                    PdfVersion::Pdf17 => hayro_syntax::PdfVersion::Pdf17,
                    PdfVersion::Pdf20 => hayro_syntax::PdfVersion::Pdf20,
                };
                let mut pdf_opt = krilla::pdf::Pdf::new(arc_bytes.clone()).ok().filter(|p| p.version() <= max_v);
                if pdf_opt.is_none() {
                    for src in [b"%PDF-1.7", b"%PDF-1.6", b"%PDF-1.5"] {
                        if let Some(rel) = crate::relabel_pdf_header(&arc_bytes, src, target_header)
                            && let Ok(p) = krilla::pdf::Pdf::new(std::sync::Arc::new(rel))
                            && p.version() <= max_v
                        {
                            pdf_opt = Some(p);
                            break;
                        }
                    }
                }
                if let Some(pdf) = pdf_opt {
                    let doc = krilla::pdf::PdfDocument::new(std::sync::Arc::new(pdf));
                    s.push_transform(&tf(Affine::translate((r.x0, r.y0))));
                    s.draw_pdf_page(&doc, size, 0);
                    s.pop();
                    return;
                }
            }
        }
        if str_svg.starts_with("data:image/") || str_svg.starts_with("iVBOR") {
            let b64 = if let Some((_, b64_part)) = str_svg.split_once(";base64,") {
                b64_part
            } else if let Some((_, b64_part)) = str_svg.split_once(',') {
                b64_part
            } else {
                str_svg
            };
            let bytes = decode_base64(b64);
            if let Ok(img) = image::load_from_memory(&bytes) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                if let Some(size) = krilla::geom::Size::from_wh(r.width().max(1e-3) as f32, r.height().max(1e-3) as f32) {
                    let krilla_img = krilla::image::Image::from_rgba8(rgba.into_raw(), w, h);
                    s.push_transform(&tf(Affine::translate((r.x0, r.y0))));
                    s.draw_image(krilla_img, size);
                    s.pop();
                    return;
                }
            }
        }
        if str_svg.eq_ignore_ascii_case("check") {
            let mut p = designcraft_geom::BezPath::new();
            p.move_to(designcraft_geom::Point::new(r.x0 + r.width() * 0.1, r.y0 + r.height() * 0.5));
            p.line_to(designcraft_geom::Point::new(r.x0 + r.width() * 0.4, r.y1 - r.height() * 0.15));
            p.line_to(designcraft_geom::Point::new(r.x1 - r.width() * 0.1, r.y0 + r.height() * 0.15));
            if let Some(path) = to_path(&p) {
                s.set_fill(None);
                s.set_stroke(Some(Stroke {
                    paint: c.into(),
                    width: (r.width() * 0.18).max(1.5) as f32,
                    line_cap: krilla::paint::LineCap::Round,
                    line_join: krilla::paint::LineJoin::Round,
                    ..Default::default()
                }));
                s.draw_path(&path);
                s.set_stroke(None);
            }
            return;
        }
        let path = if str_svg.eq_ignore_ascii_case("square") {
            Some(r.to_path(0.1))
        } else if str_svg.eq_ignore_ascii_case("circle") {
            let cx = (r.x0 + r.x1) / 2.0;
            let cy = (r.y0 + r.y1) / 2.0;
            let rad = (r.width().min(r.height())) / 2.0;
            Some(designcraft_geom::kurbo::Circle::new(designcraft_geom::Point::new(cx, cy), rad).to_path(0.1))
        } else if str_svg.eq_ignore_ascii_case("diamond") {
            let cx = (r.x0 + r.x1) / 2.0;
            let cy = (r.y0 + r.y1) / 2.0;
            let mut p = designcraft_geom::BezPath::new();
            p.move_to(designcraft_geom::Point::new(cx, r.y0));
            p.line_to(designcraft_geom::Point::new(r.x1, cy));
            p.line_to(designcraft_geom::Point::new(cx, r.y1));
            p.line_to(designcraft_geom::Point::new(r.x0, cy));
            p.close_path();
            Some(p)
        } else if str_svg.eq_ignore_ascii_case("triangle") || str_svg.eq_ignore_ascii_case("arrow") {
            let cy = (r.y0 + r.y1) / 2.0;
            let mut p = designcraft_geom::BezPath::new();
            p.move_to(designcraft_geom::Point::new(r.x0, r.y0));
            p.line_to(designcraft_geom::Point::new(r.x1, cy));
            p.line_to(designcraft_geom::Point::new(r.x0, r.y1));
            p.close_path();
            Some(p)
        } else if str_svg.eq_ignore_ascii_case("dash") || str_svg.eq_ignore_ascii_case("line") {
            let cy = (r.y0 + r.y1) / 2.0;
            let h = (r.height() * 0.25).max(1.0);
            Some(Rect::new(r.x0, cy - h / 2.0, r.x1, cy + h / 2.0).to_path(0.1))
        } else if str_svg.eq_ignore_ascii_case("star") {
            let cx = (r.x0 + r.x1) / 2.0;
            let cy = (r.y0 + r.y1) / 2.0;
            let r_outer = (r.width().min(r.height())) / 2.0;
            let r_inner = r_outer * 0.45;
            let mut p = designcraft_geom::BezPath::new();
            for i in 0..10 {
                let angle = (i as f64) * std::f64::consts::PI / 5.0 - std::f64::consts::FRAC_PI_2;
                let rad = if i % 2 == 0 { r_outer } else { r_inner };
                let pt = designcraft_geom::Point::new(cx + angle.cos() * rad, cy + angle.sin() * rad);
                if i == 0 {
                    p.move_to(pt);
                } else {
                    p.line_to(pt);
                }
            }
            p.close_path();
            Some(p)
        } else if let Ok(mut bp) = designcraft_geom::kurbo::BezPath::from_svg(str_svg) {
            let bbox = bp.bounding_box();
            if bbox.width() > 0.0 && bbox.height() > 0.0 {
                let sx = r.width() / bbox.width();
                let sy = r.height() / bbox.height();
                let aff = designcraft_geom::Affine::translate((r.x0 - bbox.x0 * sx, r.y0 - bbox.y0 * sy))
                    * designcraft_geom::Affine::scale_non_uniform(sx, sy);
                bp.apply_affine(aff);
                Some(bp)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(bp) = path
            && let Some(p) = to_path(&bp)
        {
            s.set_stroke(None);
            s.set_fill(Some(solid_fill(c, 1.0)));
            s.draw_path(&p);
            s.set_fill(None);
        }
    }

    /// Unicode text for a run's glyphs: (text, per-glyph byte range into it).
    fn run_text(&mut self, glyphs: &[PlacedGlyph], story: &str) -> (String, Vec<Range<usize>>) {
        let cmap = self.reverse_cmap(&glyphs[0].face);
        let mut text = String::new();
        let mut ranges: Vec<Range<usize>> = Vec::with_capacity(glyphs.len());
        // Byte of the previous glyph when it came straight from the story (cluster continuation).
        let mut prev_src: Option<usize> = None;
        for g in glyphs {
            let src = story.get(g.byte..g.byte + g.len).filter(|t| !t.is_empty() && !t.chars().any(is_marker));
            let range = if let Some(t) = src {
                prev_src = Some(g.byte);
                let a = text.len();
                text.push_str(t);
                a..text.len()
            } else if g.len == 0 && prev_src == Some(g.byte) && !ranges.is_empty() {
                // Second glyph of a multi-glyph cluster: share the cluster's text.
                ranges[ranges.len() - 1].clone()
            } else {
                // Inserted glyphs (page numbers, list labels, hyphens): map back through the font.
                prev_src = None;
                match cmap.get(&g.gid) {
                    Some(c) => {
                        let a = text.len();
                        text.push(*c);
                        a..text.len()
                    }
                    None => match ranges.last() {
                        Some(r) => r.clone(),
                        None => {
                            text.push('\u{FFFD}');
                            0..text.len()
                        }
                    },
                }
            };
            ranges.push(range);
        }
        (text, ranges)
    }

    fn run(&mut self, s: &mut Surface, cs: &ComposedStory, glyphs: &[PlacedGlyph], baseline: f64, story: &str) {
        let g0 = &glyphs[0];
        let st = &cs.styles[g0.style as usize];
        let fill = self.swatch_color(&st.fill, st.fill_tint);
        let stroke =
            if st.stroke != designcraft_color::swatch::NONE && st.stroke_weight > 0.0 { self.swatch_color(&st.stroke, st.stroke_tint) } else { None };
        if fill.is_none() && stroke.is_none() {
            return;
        }
        let face = g0.face;
        let size = g0.sy * face.upem;
        let hs = g0.sx / g0.sy;
        let skew = if st.skew != 0.0 { Affine::new([1.0, 0.0, -st.skew.to_radians().tan(), 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
        let origin = Affine::translate((g0.x, baseline + g0.y)) * skew * Affine::scale_non_uniform(hs, 1.0);
        // A missing glyph is drawn as its box: PDF/A and PDF/UA forbid showing .notdef as text.
        if g0.gid == 0 {
            self.outline_run(s, glyphs, origin, size, fill, stroke, st.stroke_weight);
            return;
        }
        let Some(font) = self.font(&face) else {
            self.warn(format!("font `{} {}` could not be embedded; its text was drawn as outlines", face.family, face.style));
            self.outline_run(s, glyphs, origin, size, fill, stroke, st.stroke_weight);
            return;
        };
        let (text, ranges) = self.run_text(glyphs, story);
        let k = 1.0 / (hs * size);
        let kg: Vec<KrillaGlyph> = glyphs
            .iter()
            .zip(ranges)
            .enumerate()
            .map(|(i, (g, r))| {
                let next = glyphs.get(i + 1).map_or(g.x + g.adv, |n| n.x);
                KrillaGlyph::new(GlyphId::new(g.gid), ((next - g.x) * k) as f32, 0.0, 0.0, 0.0, r, None)
            })
            .collect();
        s.push_transform(&tf(origin));
        s.set_fill(fill.map(|c| solid_fill(c, 1.0)));
        s.set_stroke(stroke.map(|c| Stroke { paint: c.into(), width: st.stroke_weight as f32, ..Default::default() }));
        s.draw_glyphs(Point::from_xy(0.0, 0.0), &kg, font, &text, size as f32, false);
        s.set_fill(None);
        s.set_stroke(None);
        s.pop();
    }

    /// Fallback for faces krilla can't embed: glyph outlines as paths.
    #[allow(clippy::too_many_arguments)]
    fn outline_run(
        &mut self,
        s: &mut Surface,
        glyphs: &[PlacedGlyph],
        origin: Affine,
        size: f64,
        fill: Option<krilla::color::Color>,
        stroke: Option<krilla::color::Color>,
        stroke_weight: f64,
    ) {
        let db = FontDb::global();
        let g0 = &glyphs[0];
        let k = size / g0.face.upem;
        let mut bp = BezPath::new();
        for g in glyphs {
            let a = Affine::translate(((g.x - g0.x) / (g0.sx / g0.sy), 0.0)) * Affine::scale(k);
            let mut o = (*db.outline(&g.face, g.gid)).clone();
            o.apply_affine(a);
            bp.extend(o.iter());
        }
        let Some(p) = to_path(&bp) else { return };
        s.push_transform(&tf(origin));
        s.set_fill(fill.map(|c| solid_fill(c, 1.0)));
        s.set_stroke(stroke.map(|c| Stroke { paint: c.into(), width: stroke_weight as f32, ..Default::default() }));
        s.draw_path(&p);
        s.set_fill(None);
        s.set_stroke(None);
        s.pop();
    }

    /// One line of plain text in the bundled UI face (page information mark).
    pub(crate) fn plain_text(&mut self, s: &mut Surface, text: &str, at: (f64, f64), size: f64, color: krilla::color::Color) {
        let face = FontDb::global().face(designcraft_fonts::FALLBACK_FAMILY, "Regular");
        let Some(font) = self.font(&face) else { return };
        let shaped = designcraft_fonts::shape(&face, text, &[], |c| c);
        let upem = face.upem as f32;
        let glyphs: Vec<KrillaGlyph> = shaped
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let end = shaped.iter().skip(i + 1).map(|n| n.cluster).find(|c| *c != g.cluster).unwrap_or(text.len());
                let end = if end > g.cluster { end } else { text.len() };
                KrillaGlyph::new(
                    GlyphId::new(g.gid),
                    g.x_advance as f32 / upem,
                    g.x_offset as f32 / upem,
                    g.y_offset as f32 / upem,
                    0.0,
                    g.cluster..end,
                    None,
                )
            })
            .collect();
        s.set_stroke(None);
        s.set_fill(Some(solid_fill(color, 1.0)));
        s.draw_glyphs(Point::from_xy(at.0 as f32, at.1 as f32), &glyphs, font, text, size as f32, false);
        s.set_fill(None);
    }
}

fn decode_base64(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0);
    for b in s.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        } as u32;
        buf = buf << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}
