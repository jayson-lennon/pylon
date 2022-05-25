use eyre::eyre;

use std::ops::Range;

#[macro_export]
macro_rules! static_regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<fancy_regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| {
            fancy_regex::Regex::new($re)
                .expect(&format!("Malformed regex '{}'. This is a bug.", $re))
        })
    }};
}

pub type Result<T> = eyre::Result<T>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcodeKind {
    Inline,
    WithBody,
}

#[derive(Clone, Debug)]
pub struct Shortcode<'a> {
    name: &'a str,
    range: Range<usize>,
    context: Vec<(&'a str, serde_json::Value)>,
    raw: &'a str,
}

impl<'a> Shortcode<'a> {
    #[must_use]
    pub fn name(&self) -> &str {
        self.name
    }

    #[must_use]
    pub fn range(&self) -> &Range<usize> {
        &self.range
    }

    #[must_use]
    pub fn context(&self) -> &[(&str, serde_json::Value)] {
        self.context.as_ref()
    }

    #[must_use]
    pub fn raw(&self) -> &str {
        self.raw
    }
}

pub fn find_next<'a>(in_text: &'a str) -> Result<Option<Shortcode>> {
    let inline = find_one(ShortcodeKind::Inline, in_text)?;
    if inline.is_some() {
        Ok(inline)
    } else {
        find_one(ShortcodeKind::WithBody, in_text)
    }
}

pub fn find_one<'a>(kind: ShortcodeKind, in_text: &'a str) -> Result<Option<Shortcode<'a>>> {
    let re = match kind {
        ShortcodeKind::Inline => static_regex!(r#"\{\{.*?(?=\}\})\}\}"#),
        ShortcodeKind::WithBody => {
            static_regex!(r#"(?s)\{%.*?(?=%\})%\}.*?(?=\{% end %\})\{% end %\}"#)
        }
    };
    if let Some(mat) = re.find(in_text)? {
        let parsed = match kind {
            ShortcodeKind::Inline => parse::inline_shortcode(mat.as_str()),
            ShortcodeKind::WithBody => parse::body_shortcode(mat.as_str()),
        }
        // map error because `nom` returns a borrow and we want to return owned values
        .map_err(|e| eyre!("{}", e.to_string()))?
        .1;
        Ok(Some(Shortcode {
            name: parsed.name(),
            range: mat.range(),
            context: parsed.args_clone(),
            raw: mat.as_str(),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn locate_body_finds_multiple() {
        let input = r#"
        start1 {% sample(arg1 = "1") %}
        test
        {% end %} end1
        start2 {% sample(arg2 = "2") %}two{% end %} end2
        "#;
        let result = find_one(ShortcodeKind::WithBody, input).unwrap().unwrap();

        let start = result.range.start;
        let end = result.range.end;

        assert_eq!(
            &input[start..end],
            r#"{% sample(arg1 = "1") %}
        test
        {% end %}"#
        );
    }

    #[test]
    fn locate_body_finds_multiline() {
        let input = r#"test {% sample(arg1 = "1") %}
        test
        {% end %} bye"#;
        let result = find_one(ShortcodeKind::WithBody, input).unwrap().unwrap();

        let start = result.range.start;
        let end = result.range.end;

        assert_eq!(
            &input[start..end],
            r#"{% sample(arg1 = "1") %}
        test
        {% end %}"#
        );
    }

    #[test]
    fn locate_inline_finds_basic() {
        let input = r#"{{ sample(arg1 = "1") }}"#;
        find_one(ShortcodeKind::Inline, input).unwrap().unwrap();
    }

    #[test]
    fn locate_inline_finds_basic_with_underscore() {
        let input = r#"{{ test_with_underscores(arg1 = "1") }}"#;
        find_one(ShortcodeKind::Inline, input).unwrap().unwrap();
    }

    #[test]
    fn locate_inline_report_correct_range() {
        let input = r#"test {{ sample(arg1 = "1") }}"#;
        let result = find_one(ShortcodeKind::Inline, input).unwrap().unwrap();

        let start = result.range.start;
        let end = result.range.end;

        assert_eq!(&input[start..end], r#"{{ sample(arg1 = "1") }}"#);
        assert_eq!(result.raw(), r#"{{ sample(arg1 = "1") }}"#);
    }
}

mod parse {
    use nom::{
        bytes::complete::{tag, take_until, take_while},
        character::complete::multispace0,
        combinator::{map, map_res},
        error::ParseError,
        multi::separated_list0,
        sequence::{delimited, separated_pair, tuple},
        IResult,
    };

    #[derive(Clone, Debug, PartialEq)]
    pub struct ParsedShortcode<'a> {
        name: &'a str,
        args: Vec<(&'a str, serde_json::Value)>,
    }

    impl<'a> ParsedShortcode<'a> {
        #[must_use]
        pub(super) fn name(&self) -> &'a str {
            self.name
        }

        #[must_use]
        pub(super) fn args(&self) -> &[(&str, serde_json::Value)] {
            self.args.as_ref()
        }

        #[must_use]
        pub(super) fn args_clone(&self) -> Vec<(&'a str, serde_json::Value)> {
            self.args.clone()
        }
    }

    /*
    {{ example(a = "b") }}
     */

    fn ws<'a, F: 'a, O, E: ParseError<&'a str>>(
        inner: F,
    ) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
    where
        F: Fn(&'a str) -> IResult<&'a str, O, E>,
    {
        delimited(multispace0, inner, multispace0)
    }

    fn key_char(ch: char) -> bool {
        ch == '_' || ch.is_alphanumeric()
    }

    fn shortcode_name_char(ch: char) -> bool {
        ch == '-' || ch == '_' || ch.is_alphanumeric()
    }

    fn shortcode_name(s: &str) -> IResult<&str, &str> {
        take_while(shortcode_name_char)(s)
    }

    fn key(s: &str) -> IResult<&str, &str> {
        take_while(key_char)(s)
    }
    fn value(s: &str) -> IResult<&str, serde_json::Value> {
        map_res(
            delimited(tag("\""), take_until("\""), tag("\"")),
            |s: &str| serde_json::to_value(s),
        )(s)
    }

    fn kv_pair(s: &str) -> IResult<&str, (&str, serde_json::Value)> {
        separated_pair(key, ws(tag("=")), value)(s)
    }

    fn multiple_kv_pairs(s: &str) -> IResult<&str, Vec<(&str, serde_json::Value)>> {
        separated_list0(ws(tag(",")), kv_pair)(s)
    }

    fn shortcode_args(s: &str) -> IResult<&str, ParsedShortcode> {
        map(
            tuple((
                shortcode_name,
                ws(tag("(")),
                multiple_kv_pairs,
                ws(tag(")")),
            )),
            |(a, _, pairs, _)| ParsedShortcode {
                name: a,
                args: pairs,
            },
        )(s)
    }

    fn end_body_shortcode(s: &str) -> IResult<&str, &str> {
        tag("{% end %}")(s)
    }

    fn body_shortcode_header(s: &str) -> IResult<&str, ParsedShortcode> {
        delimited(ws(tag("{%")), ws(shortcode_args), tag("%}"))(s)
    }

    fn inner_body(s: &str) -> IResult<&str, serde_json::Value> {
        map_res(take_until("{% end %}"), |s| serde_json::to_value(s))(s)
    }

    pub fn body_shortcode(s: &str) -> IResult<&str, ParsedShortcode> {
        map(
            tuple((body_shortcode_header, inner_body, end_body_shortcode)),
            |(mut shortcode, body, _)| {
                shortcode.args.push(("body", body));
                shortcode
            },
        )(s)
    }

    pub fn inline_shortcode(s: &str) -> IResult<&str, ParsedShortcode> {
        delimited(ws(tag("{{")), ws(shortcode_args), ws(tag("}}")))(s)
    }

    #[cfg(test)]
    mod test {
        use serde_json::json;

        use super::*;

        #[test]
        fn gets_shortcode_name() {
            assert_eq!(shortcode_name("sample"), Ok(("", "sample")));
            assert_eq!(
                shortcode_name("with_underscores"),
                Ok(("", "with_underscores"))
            );
            assert_eq!(shortcode_name("123"), Ok(("", "123")));
            assert_eq!(shortcode_name("dash-dash"), Ok(("", "dash-dash")));
        }

        #[test]
        fn get_body_shortcode() {
            let expected = ParsedShortcode {
                name: "test",
                args: vec![
                    ("key1", json!("value1")),
                    ("key2", json!("value2")),
                    ("body", json!(" test ")),
                ],
            };
            assert_eq!(
                body_shortcode(r#"{% test( key1 = "value1", key2="value2") %} test {% end %}"#),
                Ok(("", expected))
            );
        }

        #[test]
        fn get_body_shortcode_header() {
            let expected = ParsedShortcode {
                name: "test",
                args: vec![("key1", json!("value1")), ("key2", json!("value2"))],
            };
            assert_eq!(
                body_shortcode_header(r#"{% test( key1 = "value1", key2="value2") %}"#),
                Ok(("", expected))
            );
        }

        #[test]
        fn get_end_body_shortcode() {
            assert_eq!(end_body_shortcode(r#"{% end %}"#), Ok(("", "{% end %}")));
        }

        #[test]
        fn get_inline_shortcode() {
            let expected = ParsedShortcode {
                name: "test",
                args: vec![("key1", json!("value1")), ("key2", json!("value2"))],
            };
            assert_eq!(
                inline_shortcode(r#"{{ test( key1 = "value1", key2="value2") }} "#),
                Ok(("", expected))
            );
            let expected = ParsedShortcode {
                name: "test",
                args: vec![],
            };
            assert_eq!(inline_shortcode(r#"{{ test() }} "#), Ok(("", expected)));
        }

        #[test]
        fn get_shortcode_inner() {
            let expected = ParsedShortcode {
                name: "test",
                args: vec![("key1", json!("value1")), ("key2", json!("value2"))],
            };
            assert_eq!(
                shortcode_args(r#"test(key1="value1", key2="value2")"#),
                Ok(("", expected.clone()))
            );
            assert_eq!(
                shortcode_args(r#"test ( key1 = "value1", key2="value2")"#),
                Ok(("", expected))
            );
        }

        #[test]
        fn get_kv_pair() {
            assert_eq!(
                kv_pair(r#"test="hello""#),
                Ok(("", ("test", json!("hello"))))
            );
        }

        #[test]
        fn get_multiple_kv_pairs() {
            assert_eq!(
                multiple_kv_pairs(r#"a="b",c="d""#),
                Ok(("", vec![("a", json!("b")), ("c", json!("d"))]))
            );
            assert_eq!(
                multiple_kv_pairs(r#"a = "b", c="d""#),
                Ok(("", vec![("a", json!("b")), ("c", json!("d"))]))
            );
        }

        #[test]
        fn get_key() {
            assert_eq!(key("test"), Ok(("", "test")));
            assert_eq!(key("_test"), Ok(("", "_test")));
            assert_eq!(
                key("test_with_underscores"),
                Ok(("", "test_with_underscores"))
            );
            assert_eq!(key("!test"), Ok(("!test", "")));
            assert_eq!(key("-test"), Ok(("-test", "")));
        }

        #[test]
        fn get_value() {
            let s = r#""test""#;
            assert_eq!(value(s), Ok(("", json!("test"))));
        }
    }
}
