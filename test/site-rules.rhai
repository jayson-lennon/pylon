let rules = new_rules();

let sample_ctx = #{
  sup: "hi"
};

rules.add_page_context("/index.md", |db| {
  new_context(#{
    a: "b",
    c: "d",
    e: sample_ctx,
  })
});
rules