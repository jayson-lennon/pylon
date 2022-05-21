use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct ShortCode {
    name: String,
    range: Range<usize>,
    context: Vec<(String, String)>,
}

impl ShortCode {
    pub fn new<S: Into<String>>(name: S, range: (usize, usize)) -> Self {
        Self {
            name: name.into(),
            range: Range {
                start: range.0,
                end: range.1,
            },
            context: vec![],
        }
    }

    pub fn add_context<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.context.push((key.into(), value.into()));
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn contexts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.context.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

pub fn get_shortcodes<S: AsRef<str>>(input: S) -> Vec<ShortCode> {
    use crate::util::static_regex;

    let mut shortcodes: Vec<ShortCode> = vec![];

    /***********************************************
    * Finds all instances of shortcode invocations and then splits them into two
    * groups: the name of the shortcode (name) and all key/value pairs (pairs) that correspond
    * to the context that should be generated. Whitespace is ignored.
    * Sample inputs:
    *
        blah {{  one  () }}
    *        ^^^^^^^^^^^^^
    *            | |  ||
    *         (name)  (pairs)
    *
        blah {{ one  ( a="b" ) }}
    *        ^^^^^^^^^^^^^^^^^^^^
    *           | |  |       |
    *        (name)  (pairs)
    */
    let re_shortcode =
        static_regex!(r#"\{\{\s*(?P<name>[a-zA-Z0-9_]+)\s*\((?P<pairs>.*)\)\s*\}\}"#);

    // contains the complete shortcode (including curly braces), and the start and end
    // byte positions of the entire shortcode
    let raw_shortcodes = re_shortcode
        .find_iter(input.as_ref())
        .map(|mat| (mat.as_str(), (mat.start(), mat.end())))
        .collect::<Vec<(&str, (usize, usize))>>();

    // contains the name of a shortcode, all key/value pairs, and the start and end
    // byte positions of the entire shortcode
    let mut data: Vec<(&str, &str, (usize, usize))> = vec![];
    for shortcode in raw_shortcodes {
        for mat in re_shortcode.captures_iter(shortcode.0) {
            dbg!(&mat);
            let name = mat.name("name").unwrap().as_str();
            let context = mat.name("pairs").unwrap().as_str();
            data.push((name, context, shortcode.1));
        }
    }

    /***********************************************
    * Finds all instances of a key value pair. Only supports strings for now.
    * Each pair must be comma-delimited, and whitespace is allowed between pairs
    * and the `=`.
    * Sample input:
    *
        key = "whatever", two = "three"
    *   ^^^   ^^^^^^^^^^  ^^^   ^^^^^^^
    *   key   value       key   value
    */
    let re_kv_pairs = static_regex!(r#"(?P<key>[a-zA-Z0-9_]+)\s*=\s*"(?P<value>[^"]*)""#);
    for (name, context, position) in data {
        let mut shortcode = ShortCode::new(name, position);
        for mat in re_kv_pairs.captures_iter(context) {
            shortcode.add_context(
                mat.name("key").unwrap().as_str(),
                mat.name("value").unwrap().as_str(),
            );
        }
        shortcodes.push(shortcode);
    }

    shortcodes
}

#[cfg(test)]
mod test {
    #![allow(clippy::all)]
    #![allow(warnings, unused)]

    use super::*;

    #[test]
    fn new() {
        let shortcode = ShortCode::new("test", (0, 0));
        assert_eq!(shortcode.name(), String::from("test"));
        assert!(shortcode.context.is_empty());
    }

    #[test]
    fn add_context() {
        let mut shortcode = ShortCode::new("test", (0, 0));
        shortcode.add_context("test_key", "test_value");
        shortcode.add_context("test_key2", "test_value2");
        assert_eq!(shortcode.context.len(), 2)
    }

    #[test]
    fn contexts() {
        let mut shortcode = ShortCode::new("test", (0, 0));
        shortcode.add_context("test_key", "test_value");
        shortcode.add_context("test_key2", "test_value2");
        assert_eq!(shortcode.contexts().count(), 2);
    }

    #[test]
    fn gets_shortcodes_on_multiple_lines() {
        let raw_md = r#"test {{ one  () }}
            test {{ two(test="test") }}
            another {{ three(a="1", b="2") }}"#;

        let shortcodes = super::get_shortcodes(raw_md);

        assert_eq!(shortcodes.len(), 3);
        assert_eq!(
            shortcodes[0],
            ShortCode {
                name: "one".into(),
                range: Range { start: 5, end: 18 },
                context: vec![]
            }
        );
        assert_eq!(&raw_md[5..18], r#"{{ one  () }}"#);

        assert_eq!(
            shortcodes[1],
            ShortCode {
                name: "two".into(),
                range: Range { start: 36, end: 58 },
                context: vec![("test".into(), "test".into())]
            }
        );
        assert_eq!(&raw_md[36..58], r#"{{ two(test="test") }}"#);

        assert_eq!(
            shortcodes[2],
            ShortCode {
                name: "three".into(),
                range: Range {
                    start: 79,
                    end: 104
                },
                context: vec![("a".into(), "1".into()), ("b".into(), "2".into())]
            }
        );
        assert_eq!(&raw_md[79..104], r#"{{ three(a="1", b="2") }}"#);

        assert_eq!(
            shortcodes[2].range(),
            Range {
                start: 79,
                end: 104
            }
        );
    }

    #[test]
    fn handles_whitespace() {
        let raw_md = r#"test {{ two( key = " value ") }}"#;

        let shortcodes = super::get_shortcodes(raw_md);

        assert_eq!(
            shortcodes[0],
            ShortCode {
                name: "two".into(),
                range: Range { start: 5, end: 32 },
                context: vec![("key".into(), " value ".into())]
            }
        );
    }
}
