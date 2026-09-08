//! APL character database: every glyph the palette can insert.
//!
//! Covers GNU APL 2.0 primitives, derived operators, quad names,
//! Greek/alpha identifiers, box-drawing output glyphs, and the newer
//! Dyalog extensions (insertable even where the interpreter still stubs them).

/// Category of an APL character (one palette row per category).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharCategory {
    Primitive,
    Operator,
    Quad,
    Greek,
    BoxDraw,
    Dyalog,
}

impl CharCategory {
    pub fn title(self) -> &'static str {
        match self {
            CharCategory::Primitive => "Primitives",
            CharCategory::Operator => "Operators",
            CharCategory::Quad => "Quad names",
            CharCategory::Greek => "Greek / ids",
            CharCategory::BoxDraw => "Box drawing",
            CharCategory::Dyalog => "Dyalog ext",
        }
    }
}

/// One palette entry: the text to insert plus a short human name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Text inserted into the buffer (usually one char, sometimes a ⎕ name).
    pub glyph: &'static str,
    /// Short description shown in the status bar.
    pub name: &'static str,
}

/// One palette row: a category plus its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteRow {
    pub category: CharCategory,
    pub entries: &'static [PaletteEntry],
}

macro_rules! row {
    ($cat:expr, $( ($g:expr, $n:expr) ),* $(,)?) => {
        PaletteRow {
            category: $cat,
            entries: &[ $( PaletteEntry { glyph: $g, name: $n } ),* ],
        }
    };
}

/// All palette rows in TAB-cycle order.
pub static PALETTE_ROWS: &[PaletteRow] = &[
    row!(
        CharCategory::Primitive,
        ("+", "plus"),
        ("−", "minus (high)"),
        ("-", "minus (ascii)"),
        ("×", "times"),
        ("÷", "divide"),
        ("⌈", "ceiling/max"),
        ("⌊", "floor/min"),
        ("∣", "absolute/residue"),
        ("⍳", "index gen/of"),
        ("⍸", "where"),
        ("?", "roll/deal"),
        ("⋆", "power (alt)"),
        ("*", "power/exp"),
        ("⍟", "log"),
        ("○", "circle/pi-times"),
        ("!", "binomial/factorial"),
        ("∧", "and/lcm"),
        ("∨", "or/gcd"),
        ("⍲", "nand"),
        ("⍱", "nor"),
        ("∼", "not/without"),
        ("≠", "not-equal/unique"),
        ("≤", "less-or-equal"),
        ("<", "less"),
        ("=", "equal"),
        (">", "greater"),
        ("≥", "greater-or-equal"),
        ("≡", "match/depth"),
        ("≢", "not-match/tally"),
        ("∊", "enlist/membership"),
        ("⍷", "find"),
        ("∪", "unique/union"),
        ("∩", "intersection"),
        ("⊃", "pick/disclose"),
        ("⊂", "partition/enclose"),
        ("↑", "take/mix"),
        ("↓", "drop/split"),
        ("⍪", "catenate-first/ravel"),
        (",", "catenate/ravel"),
        ("⍴", "reshape/shape"),
        ("⌽", "reverse/rotate"),
        ("⊖", "reverse-first"),
        ("⍉", "transpose"),
        ("⍋", "grade up"),
        ("⍒", "grade down"),
        ("⍎", "execute"),
        ("⍕", "format"),
        ("⊤", "encode"),
        ("⊥", "decode"),
        ("⊣", "left"),
        ("⊢", "right"),
        ("⍺", "left arg"),
        ("⍵", "right arg"),
        ("⍝", "comment"),
        ("⋄", "statement sep"),
        ("←", "assignment"),
    ),
    row!(
        CharCategory::Operator,
        ("¨", "each"),
        ("⍨", "commute/swap"),
        ("˙", "jot (dop)"),
        ("∘", "ring/product (GNU: +.× equiv)"),
        (".", "inner/outer product"),
        ("⍣", "power operator"),
        ("/", "reduce/compress"),
        ("⌿", "reduce-first"),
        ("\\", "scan/expand"),
        ("⍀", "scan-first"),
        ("⍤", "atop (dop)"),
        ("⍥", "over (dop)"),
        ("@", "at (dop)"),
        ("⌸", "key"),
        ("⍩", "quad-diamond (dfn guard)"),
        ("⍤", "bind (dop)"),
    ),
    row!(
        CharCategory::Quad,
        ("⎕IO", "index origin"),
        ("⎕PP", "print precision"),
        ("⎕A", "uppercase alphabet"),
        ("⎕D", "digits"),
        ("⎕PW", "page width"),
        ("⎕LX", "latent expression"),
        ("⎕EM", "event message"),
        ("⎕EC", "event code"),
        ("⎕WI", "window interface"),
        ("⎕SEC", "security level"),
        ("⎕FIO", "file I/O"),
        ("⎕SV", "shared variables"),
        ("⎕EA", "execute alternate"),
        ("⎕ES", "event signal"),
        ("⎕TS", "timestamp"),
        ("⎕TV", "token vector"),
        ("⎕CR", "canonical rep"),
        ("⎕FX", "fix function"),
        ("⎕EX", "expunge"),
        ("⎕NC", "name class"),
        ("⎕NL", "name list"),
        ("⎕NS", "namespace"),
        ("⎕CS", "current space"),
        ("⎕PLOT", "plot (ext)"),
        ("⎕PNG", "png (ext)"),
        ("⎕FFT", "fft (ext)"),
        ("⎕SQL", "sql (ext)"),
        ("⎕RE", "regex (ext)"),
        ("⎕CDR", "cdr (ext)"),
        ("⎕INP", "input"),
        ("⎕OUT", "output"),
    ),
    row!(
        CharCategory::Greek,
        ("⍺", "alpha"),
        ("⍵", "omega"),
        ("⍺⍺", "left operand"),
        ("⍵⍵", "right operand"),
        ("∆", "delta (in names)"),
        ("⍙", "delta-underbar"),
        ("_", "underscore"),
    ),
    row!(
        CharCategory::BoxDraw,
        ("─", "h-line"),
        ("│", "v-line"),
        ("┌", "down-right"),
        ("┐", "down-left"),
        ("└", "up-right"),
        ("┘", "up-left"),
        ("├", "v-right"),
        ("┤", "v-left"),
        ("┬", "h-down"),
        ("┴", "h-up"),
        ("┼", "cross"),
        ("═", "h-double"),
        ("║", "v-double"),
        ("◇", "diamond"),
        ("○", "circle"),
    ),
    row!(
        CharCategory::Dyalog,
        ("⍥", "over"),
        ("⌸", "key"),
        ("⊥", "decode"),
        ("⊤", "encode"),
        ("⊆", "nest/partitioned-enclose"),
        ("⊇", "first-pick (quad)"),
        ("⍷", "find"),
        ("⍸", "where/interval-index"),
        ("⍉", "transpose"),
        ("⌺", "stencil"),
        ("⍠", "variant"),
        ("⌾", "atop-under (under)"),
        ("⍤", "atop/bind"),
        ("⍣", "power"),
        ("⍢", "under (dop)"),
        ("⍤", "j-diamond"),
        ("∩", "intersection"),
        ("∪", "union"),
    ),
];

/// Number of TAB stops (palette rows).
pub fn row_count() -> usize {
    PALETTE_ROWS.len()
}

/// Row by index, wrapping around (TAB cycles forever).
pub fn row(index: usize) -> &'static PaletteRow {
    &PALETTE_ROWS[index % PALETTE_ROWS.len()]
}

/// Total number of insertable glyphs across all rows.
pub fn total_glyphs() -> usize {
    PALETTE_ROWS.iter().map(|r| r.entries.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rows_are_nonempty() {
        assert!(!PALETTE_ROWS.is_empty());
        for r in PALETTE_ROWS {
            assert!(!r.entries.is_empty(), "empty row: {:?}", r.category);
        }
    }

    #[test]
    fn row_wraps() {
        assert_eq!(row(0).category, row(row_count()).category);
    }

    #[test]
    fn core_primitives_present() {
        let prims: HashSet<&str> = PALETTE_ROWS[0].entries.iter().map(|e| e.glyph).collect();
        for g in ["⍳", "⍴", "←", "⍝", "⋄", "≡", "≢", "∊", "⍋", "⍒"] {
            assert!(prims.contains(g), "missing primitive {g}");
        }
    }

    #[test]
    fn quad_row_has_apl_names() {
        let quads: HashSet<&str> = PALETTE_ROWS[2].entries.iter().map(|e| e.glyph).collect();
        for g in ["⎕IO", "⎕PP", "⎕SEC", "⎕FIO", "⎕A", "⎕D"] {
            assert!(quads.contains(g), "missing quad {g}");
        }
    }

    #[test]
    fn no_empty_glyph_or_name() {
        for r in PALETTE_ROWS {
            for e in r.entries {
                assert!(!e.glyph.is_empty());
                assert!(!e.name.is_empty());
            }
        }
    }
}
