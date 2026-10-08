//! Run-time font registration (`fonts::add_runtime_font`): a font handed over after start-up
//! fills in CJK glyphs the bundled fonts lack. The repository holds no CJK font, so the test
//! builds a tiny TrueType font here: one square glyph mapped to 測 and 試.

use photocraft_doc::TextLayer;

use crate::{TextEngine, fonts};

const MAPPED: [u16; 2] = [0x6E2C, 0x8A66]; // 測 試

fn be16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}
fn be32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn checksum(t: &[u8]) -> u32 {
    t.chunks(4).fold(0u32, |s, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(w))
    })
}

/// A minimal TrueType font: .notdef (empty) and one 800×800 square, `family` Regular.
fn tiny_font(family: &str) -> Vec<u8> {
    let mut head = Vec::new();
    for x in [0x0001_0000u32, 0x0001_0000, 0, 0x5F0F_3CF5] {
        be32(&mut head, x);
    }
    be16(&mut head, 0x000B); // flags
    be16(&mut head, 1000); // unitsPerEm
    head.extend_from_slice(&[0; 16]); // created, modified
    for x in [0u16, 0, 1000, 800, 0, 8, 2, 0, 0] {
        be16(&mut head, x); // bbox, macStyle, lowestRecPPEM, direction hint, short loca, glyf format
    }

    let mut hhea = Vec::new();
    be32(&mut hhea, 0x0001_0000);
    for x in [880i16, -120, 0] {
        be16(&mut hhea, x as u16);
    }
    be16(&mut hhea, 1000); // advanceWidthMax
    for x in [0u16, 0, 1000, 1, 0, 0, 0, 0, 0, 0, 0, 2] {
        be16(&mut hhea, x); // minLSB, minRSB, xMaxExtent, caret, reserved, metricDataFormat, numberOfHMetrics
    }

    let mut maxp = Vec::new();
    be32(&mut maxp, 0x0001_0000);
    for x in [2u16, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
        be16(&mut maxp, x);
    }

    let mut hmtx = Vec::new();
    for _ in 0..2 {
        be16(&mut hmtx, 1000);
        be16(&mut hmtx, 0);
    }

    // Glyph 1: one contour, four on-curve points (100,0) (900,0) (900,800) (100,800).
    let mut glyf = Vec::new();
    for x in [1i16, 100, 0, 900, 800, 3, 0] {
        be16(&mut glyf, x as u16); // contours, bbox, endPts, instructionLength
    }
    glyf.extend_from_slice(&[1, 1, 1, 1]); // on curve, 16-bit deltas
    for x in [100i16, 800, 0, -800, 0, 0, 800, 0] {
        be16(&mut glyf, x as u16);
    }
    glyf.resize(36, 0);
    let mut loca = Vec::new();
    for x in [0u16, 0, 18] {
        be16(&mut loca, x);
    }

    // cmap: format 4, one segment per mapped character plus the 0xFFFF end segment.
    let mut sub = Vec::new();
    let seg = (MAPPED.len() + 1) as u16;
    for x in [4u16, 16 + seg * 8, 0, seg * 2, 4, 1, seg * 2 - 4] {
        be16(&mut sub, x);
    }
    for c in MAPPED.iter().chain([&0xFFFF]) {
        be16(&mut sub, *c); // endCode
    }
    be16(&mut sub, 0);
    for c in MAPPED.iter().chain([&0xFFFF]) {
        be16(&mut sub, *c); // startCode
    }
    for c in MAPPED {
        be16(&mut sub, 1u16.wrapping_sub(c)); // idDelta: glyph 1
    }
    be16(&mut sub, 1);
    for _ in 0..seg {
        be16(&mut sub, 0); // idRangeOffset
    }
    let mut cmap = Vec::new();
    for x in [0u16, 1, 3, 1] {
        be16(&mut cmap, x);
    }
    be32(&mut cmap, 12);
    cmap.extend(sub);

    let names = [(1u16, family.to_string()), (2, "Regular".into()), (4, format!("{family} Regular")), (6, family.replace(' ', "") + "-Regular")];
    let mut name = Vec::new();
    be16(&mut name, 0);
    be16(&mut name, names.len() as u16);
    be16(&mut name, 6 + 12 * names.len() as u16);
    let mut strings = Vec::new();
    for (id, s) in &names {
        let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for x in [3u16, 1, 0x409, *id, utf16.len() as u16, strings.len() as u16] {
            be16(&mut name, x);
        }
        strings.extend(utf16);
    }
    name.extend(strings);

    let mut os2 = Vec::new();
    for x in [4u16, 1000, 400, 5, 0] {
        be16(&mut os2, x);
    }
    os2.extend_from_slice(&[0; 20 + 2 + 10 + 16]); // sub/superscript, strikeout, family class, panose, unicode ranges
    os2.extend_from_slice(b"NONE");
    for x in [0x40u16, MAPPED[0], MAPPED[1], 880, (-120i16) as u16, 0, 880, 120] {
        be16(&mut os2, x);
    }
    os2.extend_from_slice(&[0; 8]); // code page ranges
    for x in [500u16, 700, 0, 32, 1] {
        be16(&mut os2, x);
    }

    let mut post = Vec::new();
    be32(&mut post, 0x0003_0000);
    be32(&mut post, 0);
    be16(&mut post, (-100i16) as u16);
    be16(&mut post, 50);
    post.extend_from_slice(&[0; 20]);

    let tables: [(&[u8; 4], Vec<u8>); 10] = [
        (b"OS/2", os2),
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"name", name),
        (b"post", post),
    ];
    let mut out = Vec::new();
    be32(&mut out, 0x0001_0000);
    for x in [tables.len() as u16, 128, 3, tables.len() as u16 * 16 - 128] {
        be16(&mut out, x);
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        be32(&mut out, checksum(data));
        be32(&mut out, offset as u32);
        be32(&mut out, data.len() as u32);
        let mut padded = data.clone();
        padded.resize(data.len().div_ceil(4) * 4, 0);
        offset += padded.len();
        body.extend(padded);
    }
    out.extend(body);
    out
}

fn point(text: &str) -> TextLayer {
    TextLayer { text: text.into(), font_family: "Inter".into(), size_pt: 24.0, ..Default::default() }
}

#[test]
fn a_registered_fallback_family_fills_in_cjk() {
    let mut e = TextEngine::new();
    let before = e.layout(&point("測試"), 72.0);
    assert!(before.glyphs.iter().all(|g| g.id == 0), "the bundled fonts have no CJK (.notdef expected)");
    // "Noto Sans TC" is in the Traditional Chinese fallback list: Inter text picks it up.
    assert_eq!(e.fonts.register_font_data(tiny_font("Noto Sans TC")), ["Noto Sans TC"]);
    let after = e.layout(&point("A測試"), 72.0);
    assert_eq!(after.glyphs.len(), 3);
    assert!(after.glyphs.iter().all(|g| g.id != 0), "no .notdef after registering");
    assert_eq!(after.glyphs[1].id, 1, "測 comes from the registered font");
    let (_, r) = e.render(&point("測試"), 72.0, photocraft_color::PixelFormat::RGBA8);
    assert!(!r.rect.is_empty(), "the glyphs draw");
}

#[test]
fn add_runtime_font_registers_with_the_shared_engine() {
    // A family name outside the fallback lists, so other tests using the shared engine are unaffected.
    assert_eq!(fonts::add_runtime_font(tiny_font("AB Runtime Test")).unwrap(), ["AB Runtime Test"]);
    assert!(crate::shared().lock().unwrap().fonts.has_family("AB Runtime Test"));
    assert!(fonts::add_runtime_font(b"not a font".to_vec()).is_err());
}
