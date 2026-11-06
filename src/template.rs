//! Tiny template language for presence text.
//!
//! * `{var}`            → value of `var` (empty if unknown/unset)
//! * `{var|fallback}`   → value, or the literal fallback text when empty
//! * `[ ... ]`          → optional section: rendered only if every `{var}` directly inside
//!   it is non-empty (after fallbacks). Sections can nest.
//! * `\{ \} \[ \] \\`   → literal characters

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    Var {
        name: String,
        fallback: Option<String>,
    },
    Section(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    nodes: Vec<Node>,
}

impl Template {
    pub fn parse(src: &str) -> Result<Template, String> {
        let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
        let mut text = String::new();
        let mut chars = src.chars();

        fn flush(text: &mut String, stack: &mut [Vec<Node>]) {
            if !text.is_empty() {
                stack
                    .last_mut()
                    .unwrap()
                    .push(Node::Text(std::mem::take(text)));
            }
        }

        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some(n) => text.push(n),
                    None => text.push('\\'),
                },
                '{' => {
                    flush(&mut text, &mut stack);
                    let mut inner = String::new();
                    let mut closed = false;
                    for n in chars.by_ref() {
                        if n == '}' {
                            closed = true;
                            break;
                        }
                        inner.push(n);
                    }
                    if !closed {
                        return Err(format!("unclosed '{{' in template {src:?}"));
                    }
                    let (name, fallback) = match inner.split_once('|') {
                        Some((n, f)) => (n.trim(), Some(f.to_owned())),
                        None => (inner.trim(), None),
                    };
                    if name.is_empty()
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return Err(format!(
                            "invalid variable name {name:?} in template {src:?}"
                        ));
                    }
                    stack.last_mut().unwrap().push(Node::Var {
                        name: name.to_owned(),
                        fallback,
                    });
                }
                '}' => {
                    return Err(format!(
                        "unmatched '}}' in template {src:?} (use \\}} for a literal)"
                    ));
                }
                '[' => {
                    flush(&mut text, &mut stack);
                    stack.push(Vec::new());
                }
                ']' => {
                    flush(&mut text, &mut stack);
                    if stack.len() == 1 {
                        return Err(format!(
                            "unmatched ']' in template {src:?} (use \\] for a literal)"
                        ));
                    }
                    let section = stack.pop().unwrap();
                    stack.last_mut().unwrap().push(Node::Section(section));
                }
                _ => text.push(c),
            }
        }
        flush(&mut text, &mut stack);
        if stack.len() != 1 {
            return Err(format!("unclosed '[' in template {src:?}"));
        }
        Ok(Template {
            nodes: stack.pop().unwrap(),
        })
    }

    /// Renders with `lookup(name)`; the result is trimmed.
    ///
    /// `{file}` and `{command}` are left out when that same text is already on
    /// the line, so `{activity}[ {file}]` does not print a name the activity
    /// sentence already contains. The optional section around it drops too.
    pub fn render(&self, lookup: &dyn Fn(&str) -> Option<String>) -> String {
        let mut out = String::new();
        render_nodes(&self.nodes, lookup, &mut out, "");
        out.trim().to_owned()
    }

    /// Every variable name referenced (for validation).
    pub fn vars(&self) -> Vec<&str> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a str>) {
            for n in nodes {
                match n {
                    Node::Var { name, .. } => out.push(name),
                    Node::Section(inner) => walk(inner, out),
                    Node::Text(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.nodes, &mut out);
        out
    }
}

fn resolve(
    name: &str,
    fallback: &Option<String>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> String {
    lookup(name)
        .filter(|v| !v.is_empty())
        .or_else(|| fallback.clone())
        .unwrap_or_default()
}

/// `{file}` / `{command}` repeat the name when a card line is `{activity}` plus
/// the same variable, because the activity sentence already substituted it.
fn skip_if_shown(name: &str) -> bool {
    name == "file" || name == "command"
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

/// `needle` occurs in `haystack` as its own token, not as part of a longer name.
fn token_in(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let h: Vec<char> = haystack.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    if n.len() > h.len() {
        return false;
    }
    for i in 0..=h.len() - n.len() {
        if h[i..i + n.len()] != n[..] {
            continue;
        }
        let before_ok = i == 0 || !is_token_char(h[i - 1]);
        let after = i + n.len();
        let after_ok = after == h.len() || !is_token_char(h[after]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Returns false if a variable directly in `nodes` resolved empty.
/// `prior` is text already rendered outside this node list (the parent line).
fn render_nodes(
    nodes: &[Node],
    lookup: &dyn Fn(&str) -> Option<String>,
    out: &mut String,
    prior: &str,
) -> bool {
    let mut complete = true;
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Var { name, fallback } => {
                let v = resolve(name, fallback, lookup);
                let repeated = skip_if_shown(name)
                    && !v.is_empty()
                    && (token_in(prior, &v) || token_in(out, &v));
                if repeated {
                    complete = false;
                } else {
                    complete &= !v.is_empty();
                    out.push_str(&v);
                }
            }
            Node::Section(inner) => {
                let mut buf = String::new();
                let seen = format!("{prior}{out}");
                if render_nodes(inner, lookup, &mut buf, &seen) {
                    out.push_str(&buf);
                }
            }
        }
    }
    complete
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(t: &str, vars: &[(&str, &str)]) -> String {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        Template::parse(t)
            .unwrap()
            .render(&|n| vars.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()))
    }

    #[test]
    fn variables_and_fallbacks() {
        assert_eq!(r("{a} and {b}", &[("a", "x"), ("b", "y")]), "x and y");
        assert_eq!(r("Editing {file|code}", &[]), "Editing code");
        assert_eq!(
            r("Editing {file|code}", &[("file", "main.rs")]),
            "Editing main.rs"
        );
        assert_eq!(r("{ a }", &[("a", "x")]), "x");
    }

    #[test]
    fn optional_sections_collapse() {
        let t = "{project}[ · {branch}]";
        assert_eq!(r(t, &[("project", "p"), ("branch", "main")]), "p · main");
        assert_eq!(r(t, &[("project", "p")]), "p");
        assert_eq!(r("[a {x}[ b {y}]]", &[("x", "1")]), "a 1");
        assert_eq!(r("[a {x}[ b {y}]]", &[("y", "2")]), "");
        // fallback satisfies a section
        assert_eq!(r("[{x|none}]", &[]), "none");
    }

    #[test]
    fn escapes() {
        assert_eq!(r(r"\{lit\} \[x\] \\", &[]), r"{lit} [x] \");
    }

    #[test]
    fn parse_errors() {
        for bad in ["{oops", "a}", "[x", "x]", "{}", "{a b}"] {
            assert!(Template::parse(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn lists_vars() {
        let t = Template::parse("{a}[{b|x}[{c}]]").unwrap();
        assert_eq!(t.vars(), ["a", "b", "c"]);
    }

    #[test]
    fn file_and_command_are_not_repeated_on_the_same_line() {
        let vars = [
            ("activity", "Fapping to questions.txt"),
            ("file", "questions.txt"),
            ("command", "cargo"),
        ];
        assert_eq!(r("{activity}[{ file}]", &vars), "Fapping to questions.txt");
        assert_eq!(
            r(
                "{activity}[ {command}]",
                &[
                    ("activity", "Cumming all over cargo"),
                    ("command", "cargo"),
                    ("file", ""),
                ]
            ),
            "Cumming all over cargo"
        );
        // Still shown when the sentence does not already contain them.
        assert_eq!(
            r(
                "{activity}[ {file}][ {command}]",
                &[
                    ("activity", "Working"),
                    ("file", "questions.txt"),
                    ("command", "cargo")
                ]
            ),
            "Working questions.txt cargo"
        );
        assert_eq!(
            r("Fapping to {file}", &[("file", "questions.txt")]),
            "Fapping to questions.txt"
        );
        assert_eq!(r("{file}", &[("file", "questions.txt")]), "questions.txt");
    }
}
