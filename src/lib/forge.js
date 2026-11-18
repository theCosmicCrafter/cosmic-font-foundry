/**
 * Forge Studio — Modular 7×9 Matrix Engine & Pure-JS TrueType (.ttf) SFNT Compiler.
 * Generates valid binary TrueType fonts in real time with clockwise outer contours
 * and optional counter-clockwise inner cutouts.
 */

export const MATRIX_COLS = 7;
export const MATRIX_ROWS = 9;

export const PRESETS = [
  { id: "cosmic", name: "Cosmic Geometric", shape: "chamfer", weight: 88, contrast: 96, slant: 0, tracking: 80, corner: 26, lowercaseMode: "lower" },
  { id: "stencil", name: "Nebula Stencil", shape: "slit", weight: 92, contrast: 88, slant: 0, tracking: 90, corner: 16, lowercaseMode: "lower" },
  { id: "brutalist", name: "Cyber Brutalist", shape: "square", weight: 100, contrast: 100, slant: 0, tracking: 65, corner: 0, lowercaseMode: "smallcaps" },
  { id: "pixel", name: "Orbital Pixel", shape: "hollow", weight: 90, contrast: 100, slant: 0, tracking: 85, corner: 0, lowercaseMode: "lower" },
  { id: "minimal", name: "Starlight Minimal", shape: "rounded", weight: 72, contrast: 85, slant: 8, tracking: 105, corner: 42, lowercaseMode: "lower" },
];

export const SHAPES = [
  { id: "square", label: "Block" },
  { id: "chamfer", label: "Chamfer" },
  { id: "rounded", label: "Capsule" },
  { id: "diamond", label: "Diamond" },
  { id: "slit", label: "Stencil" },
  { id: "hollow", label: "Orbit" },
];

const UPPER_MATRICES = {
  A: ["..###..", ".#...#.", "#.....#", "#.....#", "#######", "#.....#", "#.....#", "#.....#", "#.....#"],
  B: ["######.", "#.....#", "#.....#", "#.....#", "######.", "#.....#", "#.....#", "#.....#", "######."],
  C: [".#####.", "#.....#", "#......", "#......", "#......", "#......", "#......", "#.....#", ".#####."],
  D: ["#####..", "#....#.", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#....#.", "#####.."],
  E: ["#######", "#......", "#......", "#......", "######.", "#......", "#......", "#......", "#######"],
  F: ["#######", "#......", "#......", "#......", "######.", "#......", "#......", "#......", "#......"],
  G: [".#####.", "#.....#", "#......", "#......", "#..####", "#.....#", "#.....#", "#.....#", ".#####."],
  H: ["#.....#", "#.....#", "#.....#", "#.....#", "#######", "#.....#", "#.....#", "#.....#", "#.....#"],
  I: [".#####.", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", ".#####."],
  J: ["..#####", ".....#.", ".....#.", ".....#.", ".....#.", ".....#.", "#....#.", "#....#.", ".####.."],
  K: ["#....#.", "#...#..", "#..#...", "#.#....", "##.....", "#.#....", "#..#...", "#...#..", "#....#."],
  L: ["#......", "#......", "#......", "#......", "#......", "#......", "#......", "#......", "#######"],
  M: ["#.....#", "##...##", "#.#.#.#", "#..#..#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#"],
  N: ["#.....#", "##....#", "#.#...#", "#..#..#", "#...#.#", "#....##", "#.....#", "#.....#", "#.....#"],
  O: [".#####.", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  P: ["######.", "#.....#", "#.....#", "#.....#", "######.", "#......", "#......", "#......", "#......"],
  Q: [".#####.", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#..#..#", "#...#.#", ".####.#"],
  R: ["######.", "#.....#", "#.....#", "#.....#", "######.", "#..#...", "#...#..", "#....#.", "#.....#"],
  S: [".#####.", "#.....#", "#......", ".#.....", "..###..", ".....#.", "......#", "#.....#", ".#####."],
  T: ["#######", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#..."],
  U: ["#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  V: ["#.....#", "#.....#", "#.....#", "#.....#", ".#...#.", ".#...#.", "..#.#..", "..#.#..", "...#..."],
  W: ["#.....#", "#.....#", "#.....#", "#.....#", "#..#..#", "#..#..#", "#.#.#.#", "##...##", "#.....#"],
  X: ["#.....#", "#.....#", ".#...#.", "..#.#..", "...#...", "..#.#..", ".#...#.", "#.....#", "#.....#"],
  Y: ["#.....#", "#.....#", ".#...#.", "..#.#..", "...#...", "...#...", "...#...", "...#...", "...#..."],
  Z: ["#######", "......#", ".....#.", "....#..", "...#...", "..#....", ".#.....", "#......", "#######"],
};

const LOWER_MATRICES = {
  a: [".......", ".......", ".#####.", "......#", ".######", "#.....#", "#.....#", "#....##", ".####.#"],
  b: ["#......", "#......", "#......", "#.####.", "##....#", "#.....#", "#.....#", "##....#", "#.####."],
  c: [".......", ".......", ".#####.", "#.....#", "#......", "#......", "#......", "#.....#", ".#####."],
  d: ["......#", "......#", "......#", ".####.#", "#....##", "#.....#", "#.....#", "#....##", ".####.#"],
  e: [".......", ".......", ".#####.", "#.....#", "#######", "#......", "#......", "#.....#", ".#####."],
  f: ["..###..", ".#...#.", ".#.....", "####...", ".#.....", ".#.....", ".#.....", ".#.....", ".#....."],
  g: [".......", ".......", ".####.#", "#....##", "#.....#", "#....##", ".####.#", "......#", ".#####."],
  h: ["#......", "#......", "#......", "#.####.", "##....#", "#.....#", "#.....#", "#.....#", "#.....#"],
  i: ["...#...", ".......", "..##...", "...#...", "...#...", "...#...", "...#...", "...#...", "..###.."],
  j: ["....#..", ".......", "...##..", "....#..", "....#..", "....#..", "#...#..", "#...#..", ".###..."],
  k: ["#......", "#......", "#...#..", "#..#...", "#.#....", "##.....", "#.#....", "#..#...", "#...#.."],
  l: ["..##...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", "...#...", "..###.."],
  m: [".......", ".......", "##.##..", "#.#..#.", "#.#..#.", "#.#..#.", "#.#..#.", "#.#..#.", "#.#..#."],
  n: [".......", ".......", "#.####.", "##....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#"],
  o: [".......", ".......", ".#####.", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  p: [".......", ".......", "#.####.", "##....#", "#.....#", "##....#", "#.####.", "#......", "#......"],
  q: [".......", ".......", ".####.#", "#....##", "#.....#", "#....##", ".####.#", "......#", "......#"],
  r: [".......", ".......", "#.###..", "##...#.", "#......", "#......", "#......", "#......", "#......"],
  s: [".......", ".......", ".#####.", "#......", ".####..", ".....#.", "......#", "#.....#", ".#####."],
  t: ["..#....", "..#....", "#####..", "..#....", "..#....", "..#....", "..#....", "..#..#.", "...##.."],
  u: [".......", ".......", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#....##", ".####.#"],
  v: [".......", ".......", "#.....#", "#.....#", "#.....#", ".#...#.", ".#...#.", "..#.#..", "...#..."],
  w: [".......", ".......", "#.....#", "#.....#", "#..#..#", "#..#..#", "#.#.#.#", "##...##", ".#...#."],
  x: [".......", ".......", "#.....#", ".#...#.", "..#.#..", "...#...", "..#.#..", ".#...#.", "#.....#"],
  y: [".......", ".......", "#.....#", "#.....#", "#....##", ".####.#", "......#", "#.....#", ".#####."],
  z: [".......", ".......", "#######", ".....#.", "....#..", "...#...", "..#....", ".#.....", "#######"],
};

const DIGIT_SYMBOL_MATRICES = {
  "0": [".#####.", "#....##", "#...#.#", "#..#..#", "#.#...#", "##....#", "#.....#", "#.....#", ".#####."],
  "1": ["...#...", "..##...", ".#.#...", "...#...", "...#...", "...#...", "...#...", "...#...", ".#####."],
  "2": [".#####.", "#.....#", "......#", ".....#.", "...##..", "..#....", ".#.....", "#......", "#######"],
  "3": [".#####.", "#.....#", "......#", ".....#.", "..####.", "......#", "......#", "#.....#", ".#####."],
  "4": ["....##.", "...#.#.", "..#..#.", ".#...#.", "#....#.", "#######", ".....#.", ".....#.", ".....#."],
  "5": ["#######", "#......", "#......", "######.", "......#", "......#", "......#", "#.....#", ".#####."],
  "6": [".#####.", "#.....#", "#......", "######.", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  "7": ["#######", "......#", ".....#.", "....#..", "...#...", "..#....", "..#....", "..#....", "..#...."],
  "8": [".#####.", "#.....#", "#.....#", "#.....#", ".#####.", "#.....#", "#.....#", "#.....#", ".#####."],
  "9": [".#####.", "#.....#", "#.....#", "#.....#", ".######", "......#", "......#", "#.....#", ".#####."],
  "!": ["...#...", "...#...", "...#...", "...#...", "...#...", "...#...", ".......", "...#...", "...#..."],
  "?": [".#####.", "#.....#", "......#", ".....#.", "...##..", "...#...", ".......", "...#...", "...#..."],
  ".": [".......", ".......", ".......", ".......", ".......", ".......", ".......", "..##...", "..##..."],
  ",": [".......", ".......", ".......", ".......", ".......", ".......", "...#...", "...#...", "..#...."],
  ":": [".......", ".......", "..##...", "..##...", ".......", ".......", "..##...", "..##...", "......."],
  ";": [".......", ".......", "..##...", "..##...", ".......", ".......", "...#...", "...#...", "..#...."],
  "-": [".......", ".......", ".......", ".......", ".#####.", ".......", ".......", ".......", "......."],
  "_": [".......", ".......", ".......", ".......", ".......", ".......", ".......", ".......", "#######"],
  "+": [".......", ".......", "...#...", "...#...", ".#####.", "...#...", "...#...", ".......", "......."],
  "=": [".......", ".......", ".......", ".#####.", ".......", ".#####.", ".......", ".......", "......."],
  "/": ["......#", ".....#.", "....#..", "....#..", "...#...", "..#....", "..#....", ".#.....", "#......"],
  "(": ["....#..", "...#...", "..#....", "..#....", "..#....", "..#....", "..#....", "...#...", "....#.."],
  ")": ["..#....", "...#...", "....#..", "....#..", "....#..", "....#..", "....#..", "...#...", "..#...."],
  "[": ["..###..", "..#....", "..#....", "..#....", "..#....", "..#....", "..#....", "..#....", "..###.."],
  "]": ["..###..", "....#..", "....#..", "....#..", "....#..", "....#..", "....#..", "....#..", "..###.."],
  "'": ["...#...", "...#...", "..#....", ".......", ".......", ".......", ".......", ".......", "......."],
  '"': [".#...#.", ".#...#.", ".#...#.", ".......", ".......", ".......", ".......", ".......", "......."],
};

const ACCENTED_MATRICES = {
  "Á": ["....#..", "...#...", "..###..", ".#...#.", "#.....#", "#######", "#.....#", "#.....#", "#.....#"],
  "É": ["....#..", "...#...", "#######", "#......", "######.", "#......", "#......", "#......", "#######"],
  "Í": ["....#..", "...#...", ".#####.", "...#...", "...#...", "...#...", "...#...", "...#...", ".#####."],
  "Ó": ["....#..", "...#...", ".#####.", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  "Ú": ["....#..", "...#...", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
  "Ñ": [".##.##.", "#..#...", "#.....#", "##....#", "#.#...#", "#..#..#", "#...#.#", "#....##", "#.....#"],
  "Ü": [".#...#.", ".......", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", ".#####."],
};

export const CHAR_GROUPS = [
  { id: "upper", label: "A–Z", chars: Object.keys(UPPER_MATRICES) },
  { id: "lower", label: "a–z", chars: Object.keys(LOWER_MATRICES) },
  { id: "accented", label: "ÁÉÑ", chars: Object.keys(ACCENTED_MATRICES) },
  { id: "digits", label: "0–9", chars: ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] },
  { id: "symbols", label: "Symbols", chars: Object.keys(DIGIT_SYMBOL_MATRICES).slice(10) },
];

function rowsToGrid(rows) {
  return rows.map((line) => [...line].map((ch) => (ch === "#" ? 1 : 0)));
}

function makeSmallCaps(rows) {
  // Compress 9-row uppercase into a crisp 7-row small-caps baseline-aligned grid.
  const picked = [0, 1, 2, 4, 6, 7, 8].map((r) => [...rows[r]].map((c) => (c === "#" ? 1 : 0)));
  const empty = new Array(MATRIX_COLS).fill(0);
  return [empty, empty, ...picked];
}

export function buildPresetGlyphs(presetId = "cosmic") {
  const preset = PRESETS.find((p) => p.id === presetId) ?? PRESETS[0];
  const glyphs = {};
  for (const [ch, rows] of Object.entries(UPPER_MATRICES)) {
    glyphs[ch] = rowsToGrid(rows);
  }
  for (const [ch, rows] of Object.entries(LOWER_MATRICES)) {
    glyphs[ch] =
      preset.lowercaseMode === "smallcaps"
        ? makeSmallCaps(UPPER_MATRICES[ch.toUpperCase()])
        : rowsToGrid(rows);
  }
  for (const [ch, rows] of Object.entries(DIGIT_SYMBOL_MATRICES)) {
    glyphs[ch] = rowsToGrid(rows);
  }
  for (const [ch, rows] of Object.entries(ACCENTED_MATRICES)) {
    glyphs[ch] = rowsToGrid(rows);
  }
  if (presetId === "stencil") {
    for (const ch of Object.keys(glyphs)) {
      const g = glyphs[ch];
      if (/[A-Z0-9]/.test(ch)) {
        g[4][3] = 0;
        g[3][3] = 0;
      }
    }
  }
  return glyphs;
}

export function cloneGrid(grid) {
  return grid.map((r) => [...r]);
}

export function resizeGrid(grid, targetCols, targetRows) {
  const curRows = grid.length || MATRIX_ROWS;
  const curCols = grid[0]?.length || MATRIX_COLS;
  if (curRows === targetRows && curCols === targetCols) return cloneGrid(grid);
  const out = Array.from({ length: targetRows }, () => new Array(targetCols).fill(0));
  for (let r = 0; r < targetRows; r++) {
    const srcR0 = Math.floor((r * curRows) / targetRows);
    const srcR1 = Math.max(srcR0 + 1, Math.ceil(((r + 1) * curRows) / targetRows));
    for (let c = 0; c < targetCols; c++) {
      const srcC0 = Math.floor((c * curCols) / targetCols);
      const srcC1 = Math.max(srcC0 + 1, Math.ceil(((c + 1) * curCols) / targetCols));
      let hits = 0;
      let total = 0;
      for (let sr = srcR0; sr < srcR1 && sr < curRows; sr++) {
        for (let sc = srcC0; sc < srcC1 && sc < curCols; sc++) {
          total++;
          if (grid[sr]?.[sc]) hits++;
        }
      }
      out[r][c] = hits > 0 && hits * 2 >= total ? 1 : 0;
    }
  }
  return out;
}

export function resizeGlyphsResolution(glyphs, targetCols, targetRows) {
  const next = {};
  for (const [ch, grid] of Object.entries(glyphs)) {
    next[ch] = resizeGrid(grid, targetCols, targetRows);
  }
  return next;
}

// ---- Binary SFNT Writer Helpers ----

class ByteWriter {
  constructor(initial = 4096) {
    this.buf = new Uint8Array(initial);
    this.len = 0;
  }
  ensure(extra) {
    if (this.len + extra <= this.buf.length) return;
    let next = this.buf.length * 2;
    while (next < this.len + extra) next *= 2;
    const grown = new Uint8Array(next);
    grown.set(this.buf);
    this.buf = grown;
  }
  u8(v) {
    this.ensure(1);
    this.buf[this.len++] = v & 0xff;
  }
  u16(v) {
    this.ensure(2);
    this.buf[this.len++] = (v >>> 8) & 0xff;
    this.buf[this.len++] = v & 0xff;
  }
  i16(v) {
    this.u16(v < 0 ? 0x10000 + v : v);
  }
  u32(v) {
    this.ensure(4);
    this.buf[this.len++] = (v >>> 24) & 0xff;
    this.buf[this.len++] = (v >>> 16) & 0xff;
    this.buf[this.len++] = (v >>> 8) & 0xff;
    this.buf[this.len++] = v & 0xff;
  }
  bytes(arr) {
    this.ensure(arr.length);
    this.buf.set(arr, this.len);
    this.len += arr.length;
  }
  tag(str) {
    for (let i = 0; i < 4; i++) this.u8(str.charCodeAt(i));
  }
  pad4() {
    while (this.len % 4 !== 0) this.u8(0);
  }
  toUint8Array() {
    return this.buf.slice(0, this.len);
  }
}

function calcChecksum(bytes) {
  let sum = 0;
  const paddedLen = Math.ceil(bytes.length / 4) * 4;
  for (let i = 0; i < paddedLen; i += 4) {
    const b0 = bytes[i] ?? 0;
    const b1 = bytes[i + 1] ?? 0;
    const b2 = bytes[i + 2] ?? 0;
    const b3 = bytes[i + 3] ?? 0;
    sum = (sum + (((b0 << 24) | (b1 << 16) | (b2 << 8) | b3) >>> 0)) >>> 0;
  }
  return sum >>> 0;
}

/**
 * Generates clockwise outer contours (and optional CCW inner contours) for a single cell.
 * All points are on-curve polygon vertices in Y-up TrueType font units.
 */
function buildModuleContours(cx, cy, w, h, shape, cornerPct, slantFactor) {
  const shear = (y) => Math.round(cx + y * slantFactor);
  const hw = Math.max(8, Math.round(w / 2));
  const hh = Math.max(8, Math.round(h / 2));

  const skewPt = (dx, dy) => [Math.round(shear(cy + dy) + dx), Math.round(cy + dy)];

  const makeBoxCW = (rx, ry) => [
    skewPt(-rx, -ry),
    skewPt(-rx, ry),
    skewPt(rx, ry),
    skewPt(rx, -ry),
  ];

  const makeBoxCCW = (rx, ry) => [
    skewPt(-rx, -ry),
    skewPt(rx, -ry),
    skewPt(rx, ry),
    skewPt(-rx, ry),
  ];

  if (shape === "diamond") {
    return [
      [
        skewPt(0, hh),
        skewPt(hw, 0),
        skewPt(0, -hh),
        skewPt(-hw, 0),
      ],
    ];
  }

  if (shape === "slit") {
    const sh = Math.max(6, Math.round(hh * 0.68));
    return [makeBoxCW(hw, sh)];
  }

  if (shape === "hollow") {
    const irx = Math.max(4, Math.round(hw * 0.42));
    const iry = Math.max(4, Math.round(hh * 0.42));
    return [makeBoxCW(hw, hh), makeBoxCCW(irx, iry)];
  }

  if (shape === "chamfer" || shape === "rounded") {
    const cutFrac = shape === "rounded" ? Math.max(0.28, cornerPct / 100) : Math.max(0.12, cornerPct / 100);
    const c = Math.max(4, Math.round(Math.min(hw, hh) * Math.min(0.48, cutFrac)));
    return [
      [
        skewPt(-hw + c, -hh),
        skewPt(-hw, -hh + c),
        skewPt(-hw, hh - c),
        skewPt(-hw + c, hh),
        skewPt(hw - c, hh),
        skewPt(hw, hh - c),
        skewPt(hw, -hh + c),
        skewPt(hw - c, -hh),
      ],
    ];
  }

  return [makeBoxCW(hw, hh)];
}

function encodeSimpleGlyph(contours) {
  if (!contours.length) return { bytes: new Uint8Array(0), xMin: 0, yMin: 0, xMax: 0, yMax: 0, points: 0 };
  let xMin = Infinity;
  let yMin = Infinity;
  let xMax = -Infinity;
  let yMax = -Infinity;
  let totalPts = 0;
  for (const c of contours) {
    for (const [x, y] of c) {
      if (x < xMin) xMin = x;
      if (y < yMin) yMin = y;
      if (x > xMax) xMax = x;
      if (y > yMax) yMax = y;
      totalPts++;
    }
  }
  const gw = new ByteWriter(256);
  gw.i16(contours.length);
  gw.i16(xMin);
  gw.i16(yMin);
  gw.i16(xMax);
  gw.i16(yMax);

  let ptEnd = -1;
  for (const c of contours) {
    ptEnd += c.length;
    gw.u16(ptEnd);
  }
  gw.u16(0); // instructionLength = 0

  // Flags: 0x01 (on-curve, 16-bit x & y deltas) for every point.
  for (let i = 0; i < totalPts; i++) gw.u8(0x01);

  let prevX = 0;
  for (const c of contours) {
    for (const [x] of c) {
      gw.i16(x - prevX);
      prevX = x;
    }
  }
  let prevY = 0;
  for (const c of contours) {
    for (const [, y] of c) {
      gw.i16(y - prevY);
      prevY = y;
    }
  }
  gw.pad4();
  return { bytes: gw.toUint8Array(), xMin, yMin, xMax, yMax, points: totalPts };
}

function buildNameTable(entries) {
  const records = [];
  const strWriter = new ByteWriter(1024);

  for (const [nameId, text] of entries) {
    // Mac Roman (platform 1, encoding 0, language 0)
    const macOffset = strWriter.len;
    const macBytes = [...text].map((c) => {
      const code = c.charCodeAt(0);
      return code < 128 ? code : 0x3f;
    });
    for (const b of macBytes) strWriter.u8(b);
    records.push({ platformId: 1, encodingId: 0, languageId: 0, nameId, length: macBytes.length, offset: macOffset });

    // Windows Unicode BMP (platform 3, encoding 1, language 0x0409)
    const winOffset = strWriter.len;
    for (let i = 0; i < text.length; i++) strWriter.u16(text.charCodeAt(i));
    records.push({
      platformId: 3,
      encodingId: 1,
      languageId: 0x0409,
      nameId,
      length: text.length * 2,
      offset: winOffset,
    });
  }

  records.sort(
    (a, b) =>
      a.platformId - b.platformId ||
      a.encodingId - b.encodingId ||
      a.languageId - b.languageId ||
      a.nameId - b.nameId,
  );

  const w = new ByteWriter(1280);
  w.u16(0); // format
  w.u16(records.length);
  w.u16(6 + records.length * 12); // stringOffset
  for (const r of records) {
    w.u16(r.platformId);
    w.u16(r.encodingId);
    w.u16(r.languageId);
    w.u16(r.nameId);
    w.u16(r.length);
    w.u16(r.offset);
  }
  w.bytes(strWriter.toUint8Array());
  return w.toUint8Array();
}

function buildCmapTable(mappings) {
  // mappings: sorted [{ code, gid }]
  const segments = [];
  for (const { code, gid } of mappings) {
    const prev = segments[segments.length - 1];
    if (prev && code === prev.end + 1 && gid - prev.startGid === code - prev.start) {
      prev.end = code;
    } else {
      segments.push({ start: code, end: code, startGid: gid });
    }
  }
  // Required sentinel segment
  segments.push({ start: 0xffff, end: 0xffff, startGid: 1, isSentinel: true });

  const segCount = segments.length;
  const segCountX2 = segCount * 2;
  const searchRange = 2 * Math.pow(2, Math.floor(Math.log2(segCount)));
  const entrySelector = Math.floor(Math.log2(searchRange / 2));
  const rangeShift = segCountX2 - searchRange;
  const subtableLen = 16 + segCount * 8;

  const sub = new ByteWriter(subtableLen);
  sub.u16(4); // format
  sub.u16(subtableLen);
  sub.u16(0); // language
  sub.u16(segCountX2);
  sub.u16(searchRange);
  sub.u16(entrySelector);
  sub.u16(rangeShift);

  for (const s of segments) sub.u16(s.end);
  sub.u16(0); // reservedPad
  for (const s of segments) sub.u16(s.start);
  for (const s of segments) {
    const delta = s.isSentinel ? 1 : (s.startGid - s.start) & 0xffff;
    sub.u16(delta);
  }
  for (let i = 0; i < segCount; i++) sub.u16(0); // idRangeOffset

  const subBytes = sub.toUint8Array();
  const cmap = new ByteWriter(20 + subBytes.length);
  cmap.u16(0); // version
  cmap.u16(2); // numTables: Platform 0/3 and Platform 3/1 pointing to same subtable
  // Platform 0 (Unicode), Encoding 3 (BMP)
  cmap.u16(0);
  cmap.u16(3);
  cmap.u32(20);
  // Platform 3 (Windows), Encoding 1 (Unicode BMP)
  cmap.u16(3);
  cmap.u16(1);
  cmap.u32(20);
  cmap.bytes(subBytes);
  return cmap.toUint8Array();
}

export function compileTrueTypeFont({
  familyName = "Cosmic Forge",
  styleName = "Regular",
  weight = 88,
  contrast = 96,
  slant = 0,
  tracking = 80,
  corner = 26,
  shape = "chamfer",
  glyphs = {},
  cols = MATRIX_COLS,
  rows = MATRIX_ROWS,
}) {
  const cleanFamily = String(familyName || "Cosmic Forge").trim().slice(0, 48) || "Cosmic Forge";
  const cleanStyle = String(styleName || "Regular").trim().slice(0, 24) || "Regular";
  const psName = `${cleanFamily}-${cleanStyle}`.replace(/[^A-Za-z0-9-]/g, "") || "CosmicForge-Regular";

  const cellSize = Math.max(18, Math.round(76 * (MATRIX_COLS / cols)));
  const originX = 48;
  const topY = 740;
  const modW = Math.max(10, Math.round(cellSize * (weight / 100)));
  const modH = Math.max(10, Math.round(modW * (contrast / 100)));
  const slantFactor = Math.tan((slant * Math.PI) / 180);
  const sidePad = Math.max(20, Math.round(tracking));

  const glyphRecords = [];
  const cmapPairs = [];

  // Glyph 0: .notdef hollow rectangle
  const notdef = encodeSimpleGlyph([
    [
      [60, 0],
      [60, 720],
      [460, 720],
      [460, 0],
    ],
    [
      [120, 60],
      [400, 60],
      [400, 660],
      [120, 660],
    ],
  ]);
  glyphRecords.push({ ...notdef, advanceWidth: 520, lsb: 60 });

  // Glyph 1: space (0x20)
  const spaceAdv = Math.round(cellSize * (cols * 0.54) + sidePad);
  glyphRecords.push({ bytes: new Uint8Array(0), xMin: 0, yMin: 0, xMax: 0, yMax: 0, points: 0, advanceWidth: spaceAdv, lsb: 0 });
  cmapPairs.push({ code: 0x20, gid: 1 });

  const sortedChars = Object.keys(glyphs).sort((a, b) => a.charCodeAt(0) - b.charCodeAt(0));
  for (const ch of sortedChars) {
    const code = ch.charCodeAt(0);
    if (code <= 0x20) continue;
    const grid = glyphs[ch];
    if (!Array.isArray(grid) || grid.length !== rows) continue;

    let minCol = cols;
    let maxCol = -1;
    const contours = [];
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        if (!grid[r][c]) continue;
        if (c < minCol) minCol = c;
        if (c > maxCol) maxCol = c;
        const cx = originX + c * cellSize + cellSize / 2;
        const cy = topY - r * cellSize - cellSize / 2;
        const cellContours = buildModuleContours(cx, cy, modW, modH, shape, corner, slantFactor);
        for (const poly of cellContours) contours.push(poly);
      }
    }

    const encoded = encodeSimpleGlyph(contours);
    const activeCols = maxCol >= minCol ? maxCol - minCol + 1 : 3;
    const lsb = encoded.points ? encoded.xMin : 0;
    const adv = encoded.points
      ? Math.max(encoded.xMax + sidePad, originX + activeCols * cellSize + sidePad)
      : spaceAdv;

    const gid = glyphRecords.length;
    glyphRecords.push({ ...encoded, advanceWidth: adv, lsb });
    cmapPairs.push({ code, gid });
  }

  // Metrics aggregation
  let fontXMin = 0;
  let fontYMin = -180;
  let fontXMax = 500;
  let fontYMax = 800;
  let maxAdv = 0;
  let minLsb = 0;
  let minRsb = 0;
  let maxExtent = 0;
  let maxPoints = 0;
  let maxContours = 0;
  let advSum = 0;
  let advCount = 0;

  let firstNonEmpty = true;
  for (const g of glyphRecords) {
    if (g.advanceWidth > maxAdv) maxAdv = g.advanceWidth;
    if (g.advanceWidth > 0) {
      advSum += g.advanceWidth;
      advCount++;
    }
    if (g.points > 0) {
      const rsb = g.advanceWidth - g.xMax;
      const ext = g.lsb + (g.xMax - g.xMin);
      if (firstNonEmpty) {
        fontXMin = g.xMin;
        fontYMin = g.yMin;
        fontXMax = g.xMax;
        fontYMax = g.yMax;
        minLsb = g.lsb;
        minRsb = rsb;
        maxExtent = ext;
        firstNonEmpty = false;
      } else {
        if (g.xMin < fontXMin) fontXMin = g.xMin;
        if (g.yMin < fontYMin) fontYMin = g.yMin;
        if (g.xMax > fontXMax) fontXMax = g.xMax;
        if (g.yMax > fontYMax) fontYMax = g.yMax;
        if (g.lsb < minLsb) minLsb = g.lsb;
        if (rsb < minRsb) minRsb = rsb;
        if (ext > maxExtent) maxExtent = ext;
      }
      if (g.points > maxPoints) maxPoints = g.points;
    }
    // Count contours: numberOfContours is first i16 of glyf bytes
    if (g.bytes.length >= 2) {
      const nc = (g.bytes[0] << 8) | g.bytes[1];
      if (nc > maxContours) maxContours = nc;
    }
  }

  // Build glyf & loca (32-bit offsets)
  const glyfWriter = new ByteWriter(glyphRecords.length * 160);
  const locaWriter = new ByteWriter((glyphRecords.length + 1) * 4);
  for (const g of glyphRecords) {
    locaWriter.u32(glyfWriter.len);
    if (g.bytes.length) glyfWriter.bytes(g.bytes);
  }
  locaWriter.u32(glyfWriter.len);
  const glyfBytes = glyfWriter.toUint8Array();
  const locaBytes = locaWriter.toUint8Array();

  // Build head
  const isBold = cleanStyle.toLowerCase().includes("bold") || weight >= 95;
  const isItalic = cleanStyle.toLowerCase().includes("oblique") || cleanStyle.toLowerCase().includes("italic") || slant >= 4;
  const macStyle = (isBold ? 1 : 0) | (isItalic ? 2 : 0);

  const head = new ByteWriter(54);
  head.u16(1);
  head.u16(0);
  head.u32(0x00010000); // fontRevision 1.0
  head.u32(0); // checkSumAdjustment placeholder
  head.u32(0x5f0f3cf5); // magicNumber
  head.u16(0x000b); // flags
  head.u16(1000); // unitsPerEm
  head.u32(0);
  head.u32(3800000000); // created
  head.u32(0);
  head.u32(3800000000); // modified
  head.i16(fontXMin);
  head.i16(fontYMin);
  head.i16(fontXMax);
  head.i16(fontYMax);
  head.u16(macStyle);
  head.u16(8); // lowestRecPPEM
  head.i16(2); // fontDirectionHint
  head.i16(1); // indexToLocFormat = 1 (long 32-bit offsets)
  head.i16(0); // glyphDataFormat
  const headBytes = head.toUint8Array();

  // Build hhea
  const ascender = Math.max(800, fontYMax + 40);
  const descender = Math.min(-200, fontYMin - 20);
  const hhea = new ByteWriter(36);
  hhea.u16(1);
  hhea.u16(0);
  hhea.i16(ascender);
  hhea.i16(descender);
  hhea.i16(100); // lineGap
  hhea.u16(maxAdv);
  hhea.i16(minLsb);
  hhea.i16(minRsb);
  hhea.i16(maxExtent);
  hhea.i16(1); // caretSlopeRise
  hhea.i16(0); // caretSlopeRun
  hhea.i16(0); // caretOffset
  hhea.i16(0);
  hhea.i16(0);
  hhea.i16(0);
  hhea.i16(0);
  hhea.i16(0); // metricDataFormat
  hhea.u16(glyphRecords.length); // numberOfHMetrics
  const hheaBytes = hhea.toUint8Array();

  // Build hmtx
  const hmtx = new ByteWriter(glyphRecords.length * 4);
  for (const g of glyphRecords) {
    hmtx.u16(g.advanceWidth);
    hmtx.i16(g.lsb);
  }
  const hmtxBytes = hmtx.toUint8Array();

  // Build maxp v1.0
  const maxp = new ByteWriter(32);
  maxp.u32(0x00010000);
  maxp.u16(glyphRecords.length);
  maxp.u16(maxPoints);
  maxp.u16(maxContours);
  maxp.u16(0); // maxCompositePoints
  maxp.u16(0); // maxCompositeContours
  maxp.u16(2); // maxZones
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  maxp.u16(0);
  const maxpBytes = maxp.toUint8Array();

  // Build OS/2 v4 (96 bytes)
  const usWeight = Math.min(900, Math.max(100, Math.round((weight / 100) * 600 + 200)));
  const fsSelection = (isBold ? 0x0020 : isItalic ? 0x0001 : 0x0040) | 0x0080;
  const maxCharCode = cmapPairs.length ? cmapPairs[cmapPairs.length - 1].code : 0x20;
  const os2 = new ByteWriter(96);
  os2.u16(4); // version
  os2.i16(advCount ? Math.round(advSum / advCount) : 500);
  os2.u16(usWeight);
  os2.u16(5); // usWidthClass
  os2.u16(0); // fsType (Installable)
  os2.i16(650);
  os2.i16(699);
  os2.i16(0);
  os2.i16(140);
  os2.i16(650);
  os2.i16(699);
  os2.i16(0);
  os2.i16(479);
  os2.i16(50);
  os2.i16(300);
  os2.i16(0); // sFamilyClass
  for (const p of [2, 0, 5, 3, 0, 0, 0, 0, 0, 0]) os2.u8(p); // PANOSE
  os2.u32(1); // ulUnicodeRange1: Basic Latin
  os2.u32(0);
  os2.u32(0);
  os2.u32(0);
  os2.tag("CSMC");
  os2.u16(fsSelection);
  os2.u16(0x0020);
  os2.u16(maxCharCode);
  os2.i16(ascender);
  os2.i16(descender);
  os2.i16(100);
  os2.u16(Math.max(850, fontYMax + 40));
  os2.u16(Math.max(250, -fontYMin + 20));
  os2.u32(1); // Latin 1 codepage
  os2.u32(0);
  os2.i16(520);
  os2.i16(720);
  os2.u16(0);
  os2.u16(32);
  os2.u16(0);
  const os2Bytes = os2.toUint8Array();

  // Build post v3.0 (32 bytes)
  const post = new ByteWriter(32);
  post.u32(0x00030000);
  post.u32(((-slant) << 16) >>> 0); // italicAngle
  post.i16(-100);
  post.i16(50);
  post.u32(0);
  post.u32(0);
  post.u32(0);
  post.u32(0);
  post.u32(0);
  const postBytes = post.toUint8Array();

  // Build name
  const nameBytes = buildNameTable([
    [0, `Forged with Cosmic Font Foundry — ${cleanFamily}`],
    [1, cleanFamily],
    [2, cleanStyle],
    [3, `CosmicForge-${psName}-${Date.now()}`],
    [4, `${cleanFamily} ${cleanStyle}`],
    [5, "Version 1.000"],
    [6, psName],
    [8, "Cosmic Font Foundry"],
    [9, "Forge Studio"],
    [13, "CC0 1.0 Universal (Public Domain)"],
    [16, cleanFamily],
    [17, cleanStyle],
  ]);

  // Build cmap
  const cmapBytes = buildCmapTable(cmapPairs);

  // Assemble SFNT tables sorted alphabetically by 4-char ASCII tag
  const tables = [
    ["OS/2", os2Bytes],
    ["cmap", cmapBytes],
    ["glyf", glyfBytes],
    ["head", headBytes],
    ["hhea", hheaBytes],
    ["hmtx", hmtxBytes],
    ["loca", locaBytes],
    ["maxp", maxpBytes],
    ["name", nameBytes],
    ["post", postBytes],
  ].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));

  const numTables = tables.length;
  const maxPow2 = Math.pow(2, Math.floor(Math.log2(numTables)));
  const searchRange = maxPow2 * 16;
  const entrySelector = Math.floor(Math.log2(maxPow2));
  const rangeShift = numTables * 16 - searchRange;

  const headerSize = 12 + numTables * 16;
  let curOffset = headerSize;
  const records = [];
  for (const [tag, raw] of tables) {
    const paddedLen = Math.ceil(raw.length / 4) * 4;
    records.push({ tag, raw, offset: curOffset, length: raw.length, checksum: calcChecksum(raw) });
    curOffset += paddedLen;
  }

  const out = new ByteWriter(curOffset);
  out.u32(0x00010000); // TrueType sfntVersion
  out.u16(numTables);
  out.u16(searchRange);
  out.u16(entrySelector);
  out.u16(rangeShift);

  let headOffset = 0;
  for (const r of records) {
    if (r.tag === "head") headOffset = r.offset;
    out.tag(r.tag);
    out.u32(r.checksum);
    out.u32(r.offset);
    out.u32(r.length);
  }
  for (const r of records) {
    out.bytes(r.raw);
    out.pad4();
  }

  const finalBytes = out.toUint8Array();
  const fullSum = calcChecksum(finalBytes);
  const adj = (0xb1b0afba - fullSum) >>> 0;
  finalBytes[headOffset + 8] = (adj >>> 24) & 0xff;
  finalBytes[headOffset + 9] = (adj >>> 16) & 0xff;
  finalBytes[headOffset + 10] = (adj >>> 8) & 0xff;
  finalBytes[headOffset + 11] = adj & 0xff;
  return finalBytes;
}
