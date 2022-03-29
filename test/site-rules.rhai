let rules = new_rules();

let sample_ctx = #{
  sup: "hi"
};

print(1);
print(PAGES);
print(2);
print(type_of(PAGES.get("/index.md")));
print(3);
print(PAGES.get("/index.md").meta("sample"));
print(4);
print("OMG");

let my_global = 10;

rules.add_page_context("/index.md", |page| {
  print(page.frontmatter.meta);
  print(page.meta);
  let hm = page.frontmatter.meta("sampl");
  print(type_of(hm));
  print(type_of("test"));

  print(hm == "test");
  if page.frontmatter.meta("sample")  == "test" {
    print("getgot");
    new_context(#{
      script_context: my_global,
    })
  } else {
    print("nope");
    new_context(#{
      nop: "yup",
    })
  }
});
rules