rules.mount("web/wwwroot");

rules.watch("web");
rules.watch("content");

// rules.add_pipeline(".", "**/*.png", [OP_COPY]);