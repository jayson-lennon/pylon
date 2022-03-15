use pulldown_cmark::{html, Options, Parser};

pub fn render<M: AsRef<str>>(raw_markdown: M) -> String {
    let raw_markdown = raw_markdown.as_ref();
    let options = Options::all();
    let parser = Parser::new_ext(raw_markdown, options);
    let mut buf = String::new();
    html::push_html(&mut buf, parser);
    buf
}
