//! APL character database: every glyph the palette can insert.
//!
//! The palette is organized into 11 rows by function. Each row is a
//! logical group (arithmetic, comparison, logical, etc.) that the user
//! cycles through with TAB.

/// Category of an APL character (one palette row per category).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharCategory {
    Assign,
    Arithmetic,
    Magnitude,
    Compare,
    Logical,
    Structural,
    Membership,
    Catenate,
    Operators,
    Punctuation,
    Misc,
}

impl CharCategory {
    pub fn title(self) -> &'static str {
        match self {
            CharCategory::Assign => "Assign / Struct",
            CharCategory::Arithmetic => "Arithmetic",
            CharCategory::Magnitude => "Magnitude / Encode",
            CharCategory::Compare => "Compare",
            CharCategory::Logical => "Logical",
            CharCategory::Structural => "Structural",
            CharCategory::Membership => "Membership / Index",
            CharCategory::Catenate => "Catenate / Reshape",
            CharCategory::Operators => "Operators",
            CharCategory::Punctuation => "Punctuation / Greek",
            CharCategory::Misc => "Misc",
        }
    }
}

/// One palette entry: the text to insert plus a short human name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Text inserted into the buffer (usually one char).
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
        CharCategory::Assign,
        ("←", "assignment"),
        ("⇐", "global assign"),
        ("⟦", "dfn open"),
        ("⟧", "dfn close"),
    ),
    row!(
        CharCategory::Arithmetic,
        ("+", "plus / conjugate"),
        ("-", "minus / negate"),
        ("×", "times / sign"),
        ("÷", "divide / reciprocal"),
        ("*", "power / exp"),
        ("⍟", "log / log-base"),
        ("√", "sqrt"),
        ("⌹", "matrix inv / divide"),
        ("○", "circle / pi-times"),
        ("!", "factorial / binomial"),
        ("?", "roll / deal"),
    ),
    row!(
        CharCategory::Magnitude,
        ("|", "abs / residue"),
        ("⌈", "ceiling / max"),
        ("⌊", "floor / min"),
        ("⊥", "decode"),
        ("⊤", "encode"),
        ("⊣", "left / same"),
        ("⊢", "right / same"),
        ("⌸", "key"),
    ),
    row!(
        CharCategory::Compare,
        ("=", "equal"),
        ("≠", "not-equal / unique mask"),
        ("≤", "less-or-equal"),
        ("<", "less"),
        (">", "greater"),
        ("≥", "greater-or-equal"),
        ("≡", "match / depth"),
        ("≢", "not-match / tally"),
    ),
    row!(
        CharCategory::Logical,
        ("∨", "or / gcd"),
        ("∧", "and / lcm"),
        ("⍲", "nand"),
        ("⍱", "nor"),
        ("&", "and / spawn"),
    ),
    row!(
        CharCategory::Structural,
        ("↑", "take / mix"),
        ("↓", "drop / split"),
        ("⊂", "enclose / partition"),
        ("⊃", "disclose / pick"),
        ("⊆", "nest / partition-enclose"),
        ("⊇", "first-pick (quad)"),
        ("⌷", "index"),
        ("⍋", "grade up"),
        ("⍒", "grade down"),
        ("≬", "grade up variant"),
        ("⫇", "grade down variant"),
    ),
    row!(
        CharCategory::Membership,
        ("⍳", "index gen / of"),
        ("⍸", "where / interval-index"),
        ("∊", "membership / enlist"),
        ("⍷", "find"),
        ("∪", "unique / union"),
        ("∩", "intersection"),
        ("~", "not / without"),
        ("/", "reduce / compress"),
        ("\\", "scan / expand"),
        ("⌿", "reduce-first"),
        ("⍀", "scan-first"),
        ("…", "ellipsis"),
    ),
    row!(
        CharCategory::Catenate,
        (",", "catenate / ravel"),
        ("⍪", "catenate-first"),
        ("⍮", "pair / 2-elem"),
        ("⍴", "reshape / shape"),
        ("⌽", "reverse / rotate"),
        ("⊖", "reverse-first"),
        ("⍉", "transpose"),
    ),
    row!(
        CharCategory::Operators,
        ("¨", "each"),
        ("⍨", "commute / swap"),
        ("⍣", "power operator"),
        ("∙", "inner product (alt)"),
        ("⌻", "inner product variant"),
        ("˝", "variant"),
        ("∘", "compose / ring"),
        ("⍛", "compose-back"),
        ("⍤", "atop / bind"),
        ("⍥", "over"),
        ("⍢", "under (alt)"),
        (".", "compose dot"),
        ("@", "at / each-element"),
        ("⌺", "stencil"),
        ("⌶", "I-beam"),
        ("⍫", "lock"),
        ("∵", "because"),
        ("∥", "parallel"),
        ("λ", "lambda (dfn)"),
        ("⍞", "char input"),
        ("√", "square root"),
        ("⍎", "execute"),
        ("⍕", "format"),
        ("⍰", "null / placeholder"),
        ("⍠", "quad variant"),
    ),
    row!(
        CharCategory::Punctuation,
        ("«", "digraph open"),
        ("»", "digraph close"),
        ("⋄", "statement separator"),
        ("⍝", "comment"),
        ("→", "branch"),
        ("⍵", "omega / right arg"),
        ("⍺", "alpha / left arg"),
        ("∇", "dfn editor"),
        ("⍓", "quad input"),
        ("⎕", "quad"),
    ),
    row!(
        CharCategory::Misc,
        ("¯", "high minus"),
        ("⍬", "zilde / empty"),
        ("∆", "delta (in names)"),
        ("⍙", "delta-underbar"),
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
    fn all_user_glyphs_present() {
        // Every glyph the user specified must appear somewhere in the palette.
        let wanted = [
            "←", "⇐", "⟦", "⟧", "+", "-", "×", "÷", "*", "⍟", "√", "⌹", "○", "!", "?", "|", "⌈",
            "⌊", "⊥", "⊤", "⊣", "⊢", "⌸", "=", "≠", "≤", "<", ">", "≥", "≡", "≢", "∨", "∧", "⍲",
            "⍱", "↑", "↓", "⊂", "⊃", "⊆", "⊇", "⌷", "⍋", "⍒", "≬", "⫇", "⍳", "⍸", "∊", "⍷", "∪",
            "∩", "~", "/", "\\", "⌿", "⍀", "…", ",", "⍪", "⍮", "⍴", "⌽", "⊖", "⍉", "¨", "⍨", "⍣",
            "∙", "⌻", "˝", "∘", "⍛", "⍤", "⍥", "⍢", "⍫", "∵", "∥", "λ", "⍞", "⍎", "⍕", "⍰", "«",
            "»", "⋄", "⍝", "→", "⍵", "⍺", "∇", "⍓", "¯", "⍬", "∆", "⍙",
        ];
        let mut present = HashSet::new();
        for r in PALETTE_ROWS {
            for e in r.entries {
                present.insert(e.glyph);
            }
        }
        for g in wanted {
            assert!(present.contains(g), "missing glyph: {g}");
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

    #[test]
    fn eleven_rows() {
        assert_eq!(PALETTE_ROWS.len(), 11);
    }
}
