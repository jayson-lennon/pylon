rules.mount("web/wwwroot");

rules.watch("web");
rules.watch("content");

rules.add_pipeline(".", "**/*.png", [OP_COPY]);
rules.add_pipeline(".", "**/*.jpg", [OP_COPY]);
rules.add_pipeline(".", "**/*.jpeg", [OP_COPY]);
rules.add_pipeline(".", "**/*.webm", [OP_COPY]);
rules.add_pipeline(".", "**/*.gif", [OP_COPY]);