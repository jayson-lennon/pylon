let rules = new_rules();

let sample_ctx = #{
  sup: "hi"
};

rules.add_page_context("/index.md", |db, page| {
  if page.canonical_path()[0] == "/" {
    new_context(#{
      a: "b",
      c: "d",
      e: sample_ctx,
    })
  } else {
    new_context(#{
      nop: "yup",
    })
  }
});
rules