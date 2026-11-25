/**
 * Cosmic Smart Search — Fuzzy, Style, Theme, Metadata & Geometric-Vibe font matcher.
 * Works seamlessly across Local Library, Discover (all registries & presets), and QuickSwitcher.
 */

const CATEGORY_ALIASES = {
  sans: ["sans", "sans-serif", "sansserif", "grotesk", "grotesque", "neogrotesque", "gothic", "humanist", "geometric", "clean", "modern", "ui"],
  serif: ["serif", "roman", "editorial", "book", "classic", "didone", "slab", "oldstyle", "transitional", "newspaper", "academic", "formal"],
  mono: ["mono", "monospace", "code", "coding", "terminal", "dev", "developer", "programming", "fixed", "console", "typewriter", "cli", "nerd"],
  script: ["script", "handwriting", "handwritten", "cursive", "calligraphy", "brush", "signature", "lettering", "pen", "marker", "chalk"],
  display: ["display", "headline", "poster", "title", "decorative", "fancy", "novelty", "logo", "branding", "banner", "art", "expressive"],
  other: ["other", "symbol", "symbols", "icon", "icons", "emoji", "dingbat", "nonlatin", "cjk", "arabic", "indic"],
};

/**
 * Curated Theme / Mood / Style lexicon.
 * Each theme defines:
 *   triggers: search words that activate this theme
 *   keywords: substrings/families strongly associated with this theme
 *   categories: categories that fit this theme (soft boost)
 */
export const THEME_LEXICON = [
  {
    id: "halloween",
    label: "Halloween",
    triggers: ["halloween", "spooky", "horror", "creepy", "scary", "ghost", "ghosts", "spider", "spiderweb", "haunted", "monster", "vampire", "zombie", "witch", "october", "fright", "macabre", "eerie", "dark", "skull", "crypt"],
    keywords: ["phantom", "creep", "nosifer", "eater", "butcher", "chiller", "jokerman", "blood", "skull", "crypt", "grave", "witch", "fraktur", "unifraktur", "pirata", "metal", "gothic", "blackletter", "medieval", "grim", "fright", "nightmare", "spooky", "halloween", "vamp", "lacquer", "special elite", "rye", "syne", "rubik wet", "rubik glitch", "nosifer", "creepster", "butcherman", "metal mania", "grenze", "almendra", "nosferatu", "dracula", "zombie", "boo", "web"],
    categories: ["display"],
  },
  {
    id: "cyberpunk",
    label: "Cyberpunk & Sci-Fi",
    triggers: ["cyberpunk", "cyber", "scifi", "sci-fi", "futuristic", "future", "space", "cosmic", "neon", "tech", "techno", "robot", "ai", "hud", "mecha", "vaporwave", "synthwave"],
    keywords: ["orbitron", "audiowide", "exo", "rajdhani", "oxanium", "chakra", "michroma", "syncopate", "electrolize", "share tech", "space", "nebula", "cosmic", "cyber", "tech", "quantum", "zen dots", "bruno ace", "silkscreen", "monaspace", "geist", "turret", "saira", "teko", "iceland", "jura", "quantico", "aldrich"],
    categories: ["display", "mono"],
  },
  {
    id: "retro",
    label: "Retro & Pixel",
    triggers: ["retro", "vintage", "80s", "90s", "70s", "arcade", "pixel", "8bit", "8-bit", "16bit", "bitmap", "game", "gaming", "crt", "dos", "oldschool", "nostalgic", "groovy"],
    keywords: ["press start", "vt323", "silkscreen", "pixel", "jersey", "tiny5", "bungee", "monoton", "righteous", "shrikhand", "pacifico", "lobster", "cooper", "abril", "limelight", "poiret", "bangers", "arcade", "bitmap", "retro", "dotgothic", "handjet", "ibm plex mono", "cousine", "share tech", "vcr", "atari", "commodore", "syne"],
    categories: ["display", "mono"],
  },
  {
    id: "coding",
    label: "Coding & Terminal",
    triggers: ["coding", "code", "programming", "programmer", "developer", "dev", "terminal", "ide", "cli", "nerd", "ligature", "ligatures", "shell", "console"],
    keywords: ["jetbrains", "fira code", "fira mono", "cascadia", "monaspace", "iosevka", "victor mono", "commit mono", "hack", "geist mono", "intel one", "maple", "0xproto", "ibm plex mono", "source code", "consolas", "inconsolata", "space mono", "roboto mono", "ubuntu mono", "ubuntu sans mono", "meslo", "nerd", "fantasque", "league mono", "anonymous", "cousine", "red hat mono", "adwaita mono", "recursive"],
    categories: ["mono"],
  },
  {
    id: "luxury",
    label: "Luxury & Wedding",
    triggers: ["luxury", "elegant", "wedding", "fashion", "vogue", "classy", "formal", "royal", "invitation", "bridal", "editorial", "magazine", "luxe", "sophisticated", "boutique"],
    keywords: ["playfair", "cormorant", "bodoni", "cinzel", "didot", "prata", "italiana", "poiret", "sacramento", "great vibes", "alex brush", "allura", "tangerine", "parisienne", "fraunces", "garamond", "baskerville", "instrument serif", "newsreader", "spectral", "caslon", "marcellus", "yeseva", "dm serif", "libre bodoni", "abril", "young serif"],
    categories: ["serif", "script"],
  },
  {
    id: "modern",
    label: "Modern UI",
    triggers: ["modern", "minimal", "minimalist", "clean", "ui", "ux", "app", "web", "startup", "saas", "corporate", "business", "professional", "swiss", "neo", "neutral"],
    keywords: ["inter", "geist", "plus jakarta", "dm sans", "manrope", "outfit", "figtree", "onest", "satoshi", "general sans", "switzer", "work sans", "ibm plex", "helvetica", "segoe", "roboto", "open sans", "lato", "montserrat", "poppins", "urbanist", "albert sans", "public sans", "archivo", "space grotesk", "jost", "sora", "epilogue", "commissioner", "instrument sans", "bricolage", "cabinet", "clash", "red hat", "adwaita"],
    categories: ["sans"],
  },
  {
    id: "comic",
    label: "Comic & Playful",
    triggers: ["comic", "cartoon", "fun", "playful", "kids", "cute", "bubbly", "bouncy", "friendly", "whimsical", "toy", "meme", "manga"],
    keywords: ["comic", "bangers", "fredoka", "baloo", "chewy", "luckiest", "patrick", "gochi", "bubble", "boogaloo", "lilita", "titan", "shantell", "sniglet", "nunito", "quicksand", "dyna", "mochiy", "amatic", "gloria", "indie flower", "kalam", "architects daughter", "coming soon", "schoolbell", "short stack"],
    categories: ["display", "script"],
  },
  {
    id: "handwriting",
    label: "Handwriting & Brush",
    triggers: ["handwriting", "handwritten", "brush", "marker", "cursive", "calligraphy", "signature", "ink", "pen", "note", "notes", "journal", "diary", "sketch"],
    keywords: ["caveat", "pacifico", "permanent marker", "rock salt", "satisfy", "dancing script", "kaushan", "sacramento", "great vibes", "allura", "alex brush", "amatic", "kalam", "gochi", "shadows into", "homemade apple", "reenie", "indie flower", "patrick hand", "gloria", "nothing you", "la belle", "kristi", "marck", "yellowtail", "lobster", "cookie", "courgette", "damion", "mr dafoe"],
    categories: ["script"],
  },
  {
    id: "medieval",
    label: "Gothic & Medieval",
    triggers: ["medieval", "fantasy", "gothic", "blackletter", "tattoo", "metal", "heavy metal", "old english", "knight", "castle", "dnd", "rpg", "viking", "rune", "ancient"],
    keywords: ["unifraktur", "pirata", "germania", "grenze", "almendra", "cinzel", "medieval", "im fell", "eagle lake", "uncial", "old english", "fraktur", "blackletter", "chancery", "metal mania", "metamorphous", "macondo", "fondamento", "berkshire", "texturina", "gothic", "phantom"],
    categories: ["display", "serif"],
  },
  {
    id: "poster",
    label: "Bold Poster & Headline",
    triggers: ["poster", "headline", "impact", "loud", "sports", "youtube", "thumbnail", "title", "bold", "heavy", "chunky", "condensed", "tall", "block", "banner", "art deco", "deco"],
    keywords: ["anton", "bebas", "oswald", "impact", "archivo black", "bungee", "black ops", "russo", "teko", "barlow condensed", "league gothic", "league spartan", "staatliches", "syne", "unbounded", "climate crisis", "rubik mono", "alfa slab", "ultra", "bowlby", "passion one", "righteous", "monoton", "poiret", "limelight", "cinzel", "big shoulders", "chunk"],
    categories: ["display"],
  },
  {
    id: "western",
    label: "Western & Slab",
    triggers: ["western", "cowboy", "saloon", "americana", "woodblock", "slab", "rodeo", "country", "rustic", "farmhouse"],
    keywords: ["rye", "sancreek", "smokum", "ewert", "alfa slab", "arvo", "bitter", "ultra", "rokkitt", "zilla slab", "chunk", "josefin slab", "besley", "clarendon", "rockwell", "courier", "kreon", "patua", "crete", "enriqueta"],
    categories: ["serif", "display"],
  },
  {
    id: "graffiti",
    label: "Street & Grunge",
    triggers: ["graffiti", "street", "grunge", "punk", "skate", "urban", "distressed", "rough", "spray", "rebel", "hiphop", "hip-hop", "glitch"],
    keywords: ["permanent marker", "rock salt", "rubik glitch", "rubik wet", "sedgwick", "lacquer", "bangers", "finger paint", "gloria", "knewave", "marker", "brush", "glitch", "special elite", "covered by", "bungee shade", "faster one", "rampart", "nosifer"],
    categories: ["display", "script"],
  },
  {
    id: "accessible",
    label: "Accessible & Reading",
    triggers: ["accessible", "accessibility", "dyslexia", "dyslexic", "readable", "legible", "hyperlegible", "low vision", "clear", "reading", "book", "body", "longform"],
    keywords: ["atkinson", "lexend", "inclusive", "andika", "opendyslexic", "public sans", "readex", "verdana", "tahoma", "open sans", "source sans", "source serif", "gentium", "charis", "merriweather", "lora", "literata", "bitter", "ibm plex", "inter", "noto"],
    categories: ["sans", "serif"],
  },
  {
    id: "typewriter",
    label: "Typewriter & Noir",
    triggers: ["typewriter", "noir", "detective", "newspaper", "press", "journal", "vintage", "classified", "memo", "scriptwriting", "screenplay"],
    keywords: ["courier", "special elite", "typewriter", "cutive", "ibm plex mono", "libre baskerville", "newsreader", "merriweather", "crimson", "im fell", "anonymous", "xanh mono", "space mono", "jetbrains"],
    categories: ["mono", "serif"],
  },
  {
    id: "christmas",
    label: "Holiday & Festive",
    triggers: ["christmas", "holiday", "festive", "winter", "santa", "xmas", "snow", "celebration", "party"],
    keywords: ["mountains of christmas", "berkshire", "great vibes", "cinzel", "lobster", "sacramento", "satisfy", "tangerine", "parisienne", "allura", "playfair", "pacifico", "alex brush"],
    categories: ["script", "display"],
  },
];

export const QUICK_SEARCH_THEMES = [
  { label: "Halloween", query: "halloween" },
  { label: "Modern UI", query: "modern ui" },
  { label: "Coding", query: "coding" },
  { label: "Retro Pixel", query: "retro pixel" },
  { label: "Handwriting", query: "handwriting" },
  { label: "Luxury Serif", query: "luxury serif" },
  { label: "Bold Poster", query: "bold poster" },
  { label: "Cyberpunk", query: "cyberpunk" },
];

const WEIGHT_KEYWORDS = {
  thin: [100],
  hairline: [100],
  extralight: [200],
  ultralight: [200],
  light: [300],
  regular: [400],
  normal: [400],
  book: [400],
  medium: [500],
  semibold: [600],
  demibold: [600],
  bold: [700],
  extrabold: [800],
  ultrabold: [800],
  black: [900],
  heavy: [800, 900],
};

function normalizeText(s) {
  return String(s || "")
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9\s-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Bounded Damerau-Levenshtein edit distance (supports transpositions like "hlaloween"). */
function editDistance(a, b, maxDist = 2) {
  const la = a.length;
  const lb = b.length;
  if (Math.abs(la - lb) > maxDist) return maxDist + 1;
  if (la === 0) return lb;
  if (lb === 0) return la;

  const prev2 = new Int32Array(lb + 1);
  const prev1 = new Int32Array(lb + 1);
  const curr = new Int32Array(lb + 1);
  for (let j = 0; j <= lb; j++) prev1[j] = j;

  for (let i = 1; i <= la; i++) {
    curr[0] = i;
    let rowMin = curr[0];
    const ca = a.charCodeAt(i - 1);
    for (let j = 1; j <= lb; j++) {
      const cb = b.charCodeAt(j - 1);
      const cost = ca === cb ? 0 : 1;
      let val = Math.min(prev1[j] + 1, curr[j - 1] + 1, prev1[j - 1] + cost);
      if (i > 1 && j > 1 && ca === b.charCodeAt(j - 2) && a.charCodeAt(i - 2) === cb) {
        val = Math.min(val, prev2[j - 2] + 1);
      }
      curr[j] = val;
      if (val < rowMin) rowMin = val;
    }
    if (rowMin > maxDist) return maxDist + 1;
    prev2.set(prev1);
    prev1.set(curr);
  }
  return prev1[lb];
}

/** Subsequence match for abbreviations like "jbm" -> "jetbrains mono", "pw" -> "phantom web". */
function subsequenceScore(query, target) {
  if (query.length < 2 || query.length > target.length) return 0;
  let qi = 0;
  let consecutive = 0;
  let maxConsec = 0;
  let boundaryHits = 0;
  for (let ti = 0; ti < target.length && qi < query.length; ti++) {
    if (target[ti] === query[qi]) {
      if (ti === 0 || target[ti - 1] === " " || target[ti - 1] === "-") boundaryHits++;
      consecutive++;
      if (consecutive > maxConsec) maxConsec = consecutive;
      qi++;
    } else {
      consecutive = 0;
    }
  }
  if (qi < query.length) return 0;
  // Require strong boundary alignment or compactness so random scattered letters don't false-match
  const compactness = query.length / target.length;
  if (boundaryHits >= 2 || maxConsec >= Math.min(3, query.length) || compactness >= 0.45) {
    return 24 + boundaryHits * 6 + maxConsec * 3;
  }
  return 0;
}

/** Pre-parses a user query once per keystroke into tokens, active themes, categories, and traits. */
export function parseSearchQuery(rawQuery) {
  const norm = normalizeText(rawQuery);
  if (!norm) {
    return { raw: "", norm: "", tokens: [], themes: [], categories: [], weights: [], flags: {} };
  }
  const tokens = norm.split(" ").filter(Boolean);
  const themes = [];
  for (const theme of THEME_LEXICON) {
    const matched = theme.triggers.some(
      (trig) =>
        (trig.includes(" ") ? norm.includes(trig) : tokens.includes(trig)) ||
        tokens.some((tok) => tok.length >= 6 && trig.length >= 6 && editDistance(tok, trig, 1) <= 1),
    );
    if (matched) themes.push(theme);
  }

  const categories = [];
  for (const [catKey, aliases] of Object.entries(CATEGORY_ALIASES)) {
    if (tokens.some((tok) => aliases.includes(tok) || (tok.length >= 5 && aliases.some((a) => a.length >= 5 && editDistance(tok, a, 1) <= 1)))) {
      categories.push(catKey);
    }
  }

  const weights = [];
  for (const tok of tokens) {
    if (WEIGHT_KEYWORDS[tok]) weights.push(...WEIGHT_KEYWORDS[tok]);
  }

  const flags = {
    variable: tokens.some((tok) => tok === "variable" || tok === "var" || tok === "axes"),
    italic: tokens.some((tok) => tok === "italic" || tok === "italics" || tok === "oblique" || tok === "slanted"),
    commercial: tokens.some((tok) => tok === "commercial" || tok === "ofl" || tok === "free"),
    vault: tokens.some((tok) => tok === "vault" || tok === "forged" || tok === "export" || tok === "exports"),
  };

  return { raw: rawQuery.trim(), norm, tokens, themes, categories, weights, flags };
}

function matchNameToken(tok, nameNorm, nameWords) {
  if (!tok) return 0;
  if (nameNorm === tok) return 110;
  if (nameNorm.startsWith(tok)) return 88;
  for (const w of nameWords) {
    if (w === tok) return 84;
    if (w.startsWith(tok)) return 70;
  }
  if (nameNorm.includes(tok)) return 56;

  // Typo-tolerant fuzzy match on individual words in the font name
  if (tok.length >= 3) {
    const maxDist = tok.length >= 7 ? 2 : 1;
    let bestDist = maxDist + 1;
    for (const w of nameWords) {
      // Compare full word or prefix of same length for partial typing
      const dFull = editDistance(tok, w, maxDist);
      if (dFull < bestDist) bestDist = dFull;
      if (w.length > tok.length + 1) {
        const dPref = editDistance(tok, w.slice(0, tok.length + 1), maxDist);
        if (dPref < bestDist) bestDist = dPref;
      }
    }
    if (bestDist <= maxDist) {
      return bestDist === 1 ? 42 : 30;
    }
  }

  // Acronym / subsequence match (e.g. "jbm", "pweb")
  if (tok.length >= 3) {
    const sub = subsequenceScore(tok, nameNorm);
    if (sub > 0) return sub;
  }
  return 0;
}

function extractDomainTokens(rawUrl) {
  if (!rawUrl) return "";
  try {
    const u = new URL(String(rawUrl));
    const host = u.hostname.replace(/^www\./, "");
    const path = u.pathname.replace(/\/+/g, " ");
    return normalizeText(`${host} ${host.split(".").join(" ")} ${path}`);
  } catch {
    return normalizeText(rawUrl);
  }
}

/**
 * Scores a local or Discover font item against a parsed query.
 * Returns 0 if no match; > 0 ranked score (higher = stronger relevance).
 */
export function scoreFontMatch(item, parsed, opts = {}) {
  if (!parsed || !parsed.norm) return 1;

  const nameNorm = normalizeText(item.name);
  const nameCompact = nameNorm.replace(/[\s-]+/g, "");
  const nameWords = nameNorm.split(/[\s-]+/).filter(Boolean);
  const category = String(item.category || "").toLowerCase();
  const scripts = (item.scripts ?? []).map((s) => normalizeText(s));
  const designers = (item.designers ?? []).map((d) => normalizeText(d));
  const mergedTags = [
    ...(Array.isArray(item.tags) ? item.tags : []),
    ...(Array.isArray(opts.tags) ? opts.tags : []),
  ];
  const tags = mergedTags.map((tg) => normalizeText(tg)).filter(Boolean);
  const descNorm = normalizeText(item.description || "");
  const descWords = descNorm ? descNorm.split(" ") : [];
  const licenseNorm = normalizeText(item.license || "");
  const homepageNorm = extractDomainTokens(item.homepage);
  const sourceLabel = normalizeText(item.source_label || item.source || "");
  const filePaths = [
    item.file || "",
    ...(item.files ?? []),
  ]
    .filter(Boolean)
    .map((p) => normalizeText(p));

  const styles = item.styles ?? [];
  const styleNames = Array.isArray(styles)
    ? styles.map((s) => normalizeText(typeof s === "string" ? s : s.subfamily || s.full_name || ""))
    : [];
  const styleWeights = Array.isArray(styles)
    ? styles.map((s) => (typeof s === "object" && s ? Number(s.weight) || 400 : 400))
    : [];
  const variantWeights = (item.variants ?? []).map((v) => parseInt(String(v), 10)).filter((n) => !Number.isNaN(n));
  const allWeights = styleWeights.length ? styleWeights : variantWeights;
  const hasItalic =
    (Array.isArray(styles) && styles.some((s) => typeof s === "object" && s?.italic)) ||
    (item.variants ?? []).some((v) => String(v).endsWith("i"));

  let score = 0;

  // 1. Whole-phrase match against font name (strongest signal)
  if (nameNorm === parsed.norm || nameCompact === parsed.norm.replace(/\s+/g, "")) {
    score += 180;
  } else if (nameNorm.startsWith(parsed.norm)) {
    score += 130;
  } else if (nameNorm.includes(parsed.norm)) {
    score += 95;
  } else if (parsed.norm.length >= 3) {
    const wholeSub = subsequenceScore(parsed.norm.replace(/\s+/g, ""), nameCompact);
    if (wholeSub >= 32) score += wholeSub;
  }

  // 2. Per-token scoring across name, fuzzy words, tags, description, license, foundry, scripts, styles, folders
  let matchedTokens = 0;
  for (const tok of parsed.tokens) {
    let tokBest = matchNameToken(tok, nameNorm, nameWords);

    if (tags.some((tg) => tg === tok)) {
      tokBest = Math.max(tokBest, 76);
    } else if (tags.some((tg) => tg.includes(tok))) {
      tokBest = Math.max(tokBest, 68);
    }
    if (designers.some((d) => d.includes(tok))) {
      tokBest = Math.max(tokBest, 66);
    }
    if (scripts.some((sc) => sc === tok || sc.includes(tok))) {
      tokBest = Math.max(tokBest, 62);
    }
    if (sourceLabel && (sourceLabel === tok || sourceLabel.includes(tok))) {
      tokBest = Math.max(tokBest, 54);
    }
    if (licenseNorm && (licenseNorm.split(" ").includes(tok) || licenseNorm.includes(tok))) {
      tokBest = Math.max(tokBest, 54);
    }
    if (descNorm) {
      if (descWords.includes(tok)) {
        tokBest = Math.max(tokBest, 52);
      } else if (tok.length >= 4 && descNorm.includes(tok)) {
        tokBest = Math.max(tokBest, 42);
      }
    }
    if (homepageNorm && homepageNorm.includes(tok) && tok.length >= 3) {
      tokBest = Math.max(tokBest, 50);
    }
    if (styleNames.some((sn) => sn.includes(tok))) {
      tokBest = Math.max(tokBest, 52);
    }
    if (filePaths.some((fp) => fp.includes(tok))) {
      tokBest = Math.max(tokBest, 48);
    }
    if (CATEGORY_ALIASES[category]?.includes(tok)) {
      tokBest = Math.max(tokBest, 50);
    }
    if (WEIGHT_KEYWORDS[tok] && allWeights.some((w) => WEIGHT_KEYWORDS[tok].includes(w))) {
      tokBest = Math.max(tokBest, 48);
    }
    if ((tok === "variable" || tok === "var") && item.is_variable) {
      tokBest = Math.max(tokBest, 64);
    }
    if ((tok === "italic" || tok === "italics" || tok === "oblique") && hasItalic) {
      tokBest = Math.max(tokBest, 48);
    }
    if (
      tok === "commercial" &&
      (item.license_tier === "free_commercial" ||
        licenseNorm.includes("commercial") ||
        licenseNorm.includes("ofl") ||
        licenseNorm.includes("apache") ||
        licenseNorm.includes("mit") ||
        licenseNorm.includes("itf"))
    ) {
      tokBest = Math.max(tokBest, 48);
    }
    if ((tok === "vault" || tok === "forged" || tok === "exports" || tok === "export") && filePaths.some((fp) => fp.includes("vault") || fp.includes("exports") || fp.includes("cosmic font foundry"))) {
      tokBest = Math.max(tokBest, 68);
    }
    if ((tok === "system" || tok === "windows") && item.origin === "sys") {
      tokBest = Math.max(tokBest, 45);
    }

    // Theme keyword match across name, tags, and repository description
    for (const theme of parsed.themes) {
      const trigMatch =
        theme.triggers.includes(tok) ||
        (tok.length >= 6 && theme.triggers.some((t) => t.length >= 6 && editDistance(tok, t, 1) <= 1));
      if (trigMatch) {
        const hitKw = theme.keywords.some(
          (kw) =>
            nameNorm.includes(kw) ||
            tags.some((tg) => tg.includes(kw)) ||
            (descNorm && descNorm.includes(kw)),
        );
        if (hitKw) {
          tokBest = Math.max(tokBest, 92);
        } else if (
          (theme.id === "coding" || theme.id === "handwriting" || theme.id === "modern") &&
          theme.categories.includes(category)
        ) {
          tokBest = Math.max(tokBest, 34);
        }
      }
    }

    if (tokBest > 0) {
      matchedTokens++;
      score += tokBest;
    }
  }

  // 3. Geometric Typographic DNA Vibe Score (from Rust `foundry_vibe_search` for local fonts)
  const vibeBoost = opts.vibeScoreMap?.get(item.name.toLowerCase()) ?? 0;
  if (vibeBoost > 0) {
    score += Math.round(vibeBoost * 85);
    matchedTokens = Math.max(matchedTokens, parsed.tokens.length);
  }

  // Require at least all tokens to contribute when multi-word (unless a strong full-phrase or theme hit matched)
  if (parsed.tokens.length > 1 && matchedTokens < parsed.tokens.length && score < 95) {
    return 0;
  }

  // Subtle bonus for exact theme keyword hits so true theme fonts rank above generic category matches
  for (const theme of parsed.themes) {
    if (
      theme.keywords.some(
        (kw) =>
          nameNorm.includes(kw) ||
          tags.some((tg) => tg.includes(kw)) ||
          (descNorm && descNorm.includes(kw)),
      )
    ) {
      score += 45;
    }
  }

  // If only a weak generic theme-category match happened on a multi-thousand catalog, keep it below direct matches
  return score;
}
