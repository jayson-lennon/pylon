let rules = new_rules();

let sample_ctx = #{
  sup: "hi"
};

rules.add_page_context("/index.md", |db, page| {
  print(page.frontmatter.meta);
  print(page.meta);
  let hm = page.frontmatter.meta("sampl");
  print(type_of(hm));
  print(type_of("test"));

  print("test".len());
  print(hm.len());
  debug("test");
  debug(hm);

  print(hm == "test");
  if page.frontmatter.meta("sample")  == "test" {
    print("getgot");
    new_context(#{
      a: "b",
      c: "d",
      e: sample_ctx,
    })
  } else {
    print("nope");
    new_context(#{
      nop: "yup",
    })
  }
});
rules