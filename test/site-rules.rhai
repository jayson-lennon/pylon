let rules = new_rules();

let copy_op = "[COPY]";

rules.add_pipeline("**/*.png", [copy_op]);
rules.add_pipeline("*.txt", [
  "sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
  copy_op
]);

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
      script_context: "sample == test",
    })
  } else {
    print("nope");
    new_context(#{
      script_context: "sample is something else (" + page.frontmatter.meta("sample") + ")"
    })
  }
});

rules.add_page_context("/index.md", |page| {
  new_context(#{
    global_: "crash"
  })
});
rules