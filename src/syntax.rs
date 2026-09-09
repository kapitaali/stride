//! Syntax highlighting: classify each line into coloured tokens.
//!
//! The classifier is UI-agnostic (returns [`Token`]s); `ui.rs` maps
//! [`TokenKind`] to ratatui styles. Operates per line — APL comments
//! (`⍝`) and strings never span lines in the editors we target.

/// Highlight category for a slice of source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Comment,
    String,
    Number,
    Primitive,
    Operator,
    QuadName,
    SysCmd,
    FnMarker,
    Identifier,
    Whitespace,
    Other,
}

/// A classified slice of a source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

impl Token {
    fn new(kind: TokenKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// APL primitive glyphs (single chars, GNU APL 2.0 set).
pub const PRIMITIVES: &str = "+−×÷⌈⌊∣⍳⍸?⋆*⍟○!∧∨⍲⍱∼≠≤<=>≥≡≢∊⍷∪∩⊃⊂↑↓⍪,⍴⌽⊖⍉⍋⍒⍎⍕⊤⊥⊣⊢←⋄⍝⌷⊆⍺⍵";

/// Operator glyphs (monadic/dyadic operators, incl. Dyalog extensions).
pub const OPERATORS: &str = "¨⍨˙∘.⍣/⌿\\⍀⍤⍥@⌸⍩⍠⌾⌺⍢";

pub fn is_primitive(c: char) -> bool {
    PRIMITIVES.contains(c)
}

pub fn is_operator(c: char) -> bool {
    OPERATORS.contains(c)
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '∆' || c == '⍙' || c == '⍺' || c == '⍵'
}

fn is_number_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '.' | '¯' | 'e' | 'E' | 'j' | 'J' | '∞')
}

/// Classify one line of APL source into tokens.
pub fn highlight_line(line: &str) -> Vec<Token> {
    let mut out = Vec::new();
    // System command: `)CLEAR`, `)LOAD foo`, ... — whole line.
    if line.starts_with(')') {
        out.push(Token::new(TokenKind::SysCmd, line));
        return out;
    }

    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '⍝' {
            // Comment runs to end of line.
            out.push(Token::new(
                TokenKind::Comment,
                chars[i..].iter().collect::<String>(),
            ));
            break;
        } else if c == '\'' {
            // String literal; '' is an escaped quote.
            let mut s = String::from("'");
            i += 1;
            while i < chars.len() {
                s.push(chars[i]);
                if chars[i] == '\'' {
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        s.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(Token::new(TokenKind::String, s));
        } else if c.is_whitespace() {
            let mut s = String::new();
            while i < chars.len() && chars[i].is_whitespace() {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token::new(TokenKind::Whitespace, s));
            continue;
        } else if is_number_char(c) {
            let mut s = String::new();
            while i < chars.len() && is_number_char(chars[i]) {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token::new(TokenKind::Number, s));
            continue;
        } else if is_primitive(c) {
            out.push(Token::new(TokenKind::Primitive, c.to_string()));
        } else if is_operator(c) {
            out.push(Token::new(TokenKind::Operator, c.to_string()));
        } else if c == '⎕' {
            // Quad name: ⎕ + following name chars.
            let mut s = String::from("⎕");
            i += 1;
            while i < chars.len() && is_name_char(chars[i]) {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token::new(TokenKind::QuadName, s));
            continue;
        } else if c == '∇' {
            out.push(Token::new(TokenKind::FnMarker, c.to_string()));
        } else if is_name_char(c) {
            let mut s = String::new();
            while i < chars.len() && is_name_char(chars[i]) {
                s.push(chars[i]);
                i += 1;
            }
            out.push(Token::new(TokenKind::Identifier, s));
            continue;
        } else {
            out.push(Token::new(TokenKind::Other, c.to_string()));
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_comment() {
        let toks = highlight_line("A←5 ⍝ set A to 5");
        assert_eq!(toks.last().unwrap().kind, TokenKind::Comment);
    }

    #[test]
    fn highlights_string() {
        let toks = highlight_line("⍞←'hello world'");
        assert!(toks.iter().any(|t| t.kind == TokenKind::String));
    }

    #[test]
    fn highlights_syscmd() {
        let toks = highlight_line(")CLEAR");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::SysCmd);
    }

    #[test]
    fn highlights_primitive() {
        let toks = highlight_line("A←⍳5");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Primitive));
    }

    #[test]
    fn highlights_quad() {
        let toks = highlight_line("⎕IO←1");
        assert!(toks.iter().any(|t| t.kind == TokenKind::QuadName));
    }
}
