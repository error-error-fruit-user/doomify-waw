//! A builder for generated code (ZScript, SNDINFO, MAPINFO...) that reads like the control flow
//! deciding it. Text comes from formatted multiline strings (`indoc::formatdoc!`), and the builder
//! says when it goes in:
//!
//! ```
//! # use doomify::code::Code;
//! # let (zoom, auto) = (true, false);
//! let fire = Code::new()
//!     .always("Fire:")
//!     .when(zoom, |code| code.always(r#"TNT1 A 0 WhenZoomed("ZoomFire");"#))
//!     .when(auto, |code| code.always("TNT1 A 0 A_ReFire;"))
//!     .otherwise(|code| code.always("TNT1 A 0;"))
//!     .build();
//! assert_eq!(fire, "Fire:\nTNT1 A 0 WhenZoomed(\"ZoomFire\");\nTNT1 A 0;\n");
//! ```
//!
//! `when` starts a chain, like an `if`: `otherwise_when` adds its branch only when no earlier
//! branch of the chain did (an `else if`), and `otherwise` ends it (an `else`). `always`, `block`
//! and `each` end any chain too, so an `otherwise` always belongs to the nearest `when` before it.

/// Text built line by line, indented by its blocks.
#[derive(Debug, Default, Clone)]
pub struct Code {
    lines: Vec<String>,
    indent: usize,
    /// Whether a branch of the current `when` chain was taken; None outside a chain.
    chain: Option<bool>,
}

const INDENT: &str = "    ";

impl Code {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `text`, every line at the current indentation (blank lines stay blank).
    pub fn always(mut self, text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        let text = text.strip_suffix('\n').unwrap_or(text);
        for line in text.split('\n') {
            if line.trim().is_empty() {
                self.lines.push(String::new());
            } else {
                self.lines
                    .push(format!("{}{}", INDENT.repeat(self.indent), line.trim_end()));
            }
        }
        self.chain = None;
        self
    }

    /// Adds what `then` builds if `condition` holds, and starts a chain.
    pub fn when(mut self, condition: bool, then: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            self = then(self);
        }
        self.chain = Some(condition);
        self
    }

    /// Adds what `then` builds if no earlier branch of the chain did and `condition` holds.
    pub fn otherwise_when(mut self, condition: bool, then: impl FnOnce(Self) -> Self) -> Self {
        let taken = self.chain.expect("otherwise_when needs a when before it");
        if !taken && condition {
            self = then(self);
        }
        self.chain = Some(taken || condition);
        self
    }

    /// Adds what `then` builds if no branch of the chain did, and ends the chain.
    pub fn otherwise(mut self, then: impl FnOnce(Self) -> Self) -> Self {
        let taken = self.chain.expect("otherwise needs a when before it");
        if !taken {
            self = then(self);
        }
        self.chain = None;
        self
    }

    /// Adds what `then` builds, as a statement, if `value` is there.
    pub fn when_some<T>(self, value: Option<T>, then: impl FnOnce(Self, T) -> Self) -> Self {
        let present = value.is_some();
        let mut code = match value {
            Some(value) => then(self, value),
            None => self,
        };
        code.chain = Some(present);
        code
    }

    /// Adds what `then` builds for each item, in order.
    pub fn each<T>(
        mut self,
        items: impl IntoIterator<Item = T>,
        mut then: impl FnMut(Self, T) -> Self,
    ) -> Self {
        for item in items {
            self = then(self, item);
        }
        self.chain = None;
        self
    }

    /// `header {`, then what `body` builds one level in, then `}`.
    pub fn block(self, header: impl AsRef<str>, body: impl FnOnce(Self) -> Self) -> Self {
        self.wrap(format!("{} {{", header.as_ref()), body, "}")
    }

    /// `open`, then what `body` builds one level in, then `close`.
    pub fn wrap(
        mut self,
        open: impl AsRef<str>,
        body: impl FnOnce(Self) -> Self,
        close: impl AsRef<str>,
    ) -> Self {
        self = self.always(open);
        self.indent += 1;
        self = body(self);
        self.indent -= 1;
        self.always(close)
    }

    /// What `body` builds one level in, without anything around it.
    pub fn indented(mut self, body: impl FnOnce(Self) -> Self) -> Self {
        self.indent += 1;
        self = body(self);
        self.indent -= 1;
        self.chain = None;
        self
    }

    /// An empty line, unless the text already ends with one (or is empty).
    pub fn gap(mut self) -> Self {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
        self.chain = None;
        self
    }

    /// Adds code built elsewhere, at this indentation.
    pub fn append(self, other: Code) -> Self {
        let text = other.build();
        if text.is_empty() {
            return self;
        }
        self.always(text)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The text, ending with a newline (empty if nothing was added).
    pub fn build(self) -> String {
        let mut text = self.lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(a: bool, b: bool) -> String {
        Code::new()
            .when(a, |c| c.always("a"))
            .otherwise_when(b, |c| c.always("b"))
            .otherwise(|c| c.always("neither"))
            .build()
    }

    #[test]
    fn chains_take_one_branch() {
        assert_eq!(chain(true, true), "a\n");
        assert_eq!(chain(false, true), "b\n");
        assert_eq!(chain(false, false), "neither\n");
    }

    #[test]
    fn nested_chains_keep_their_own_state() {
        let text = Code::new()
            .when(true, |c| {
                c.when(false, |c| c.always("inner"))
                    .otherwise(|c| c.always("inner else"))
            })
            .otherwise(|c| c.always("outer else"))
            .build();
        assert_eq!(text, "inner else\n");
    }

    #[test]
    fn blocks_indent_multiline_text() {
        let text = Code::new()
            .block("class A", |c| c.always("int a;\n\nint b;"))
            .build();
        assert_eq!(text, "class A {\n    int a;\n\n    int b;\n}\n");
    }

    #[test]
    #[should_panic]
    fn otherwise_without_when_is_a_bug() {
        let _ = Code::new().always("a").otherwise(|c| c);
    }
}
