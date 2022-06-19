Pylon
=====

Pylon is a static site generator focused on customization that uses Markdown documents and Tera templates.

## Features
* Arbitrary shell commands can be used to build resources
* Verifies that all resources linked within HTML files exist (CSS is on the roadmap)
* Customizable metadata linting
* Dev server w/live reload and configurable watch directories/files
* Mount (copy) directories into your site
* Add global metadata to the rendering context, accessible in all pages
* Inline files directly in a template
* Export document metadata for use in indexing
* Asset colocation
* Customizable shortcodes can be used in Markdown
* Generate Table of Contents
* Add anchors to headers
* Syntax highlighting

# Note

Pylon is in early development and unstable. Major changes are planned while still on version `0`.

# Getting Started

Pylon must be built from source for now. Packages will be created once the project experiences fewer breaking changes.

```sh
git clone https://github.com/jayson-lennon/pylon
cd pylon
cargo build --release
```

Create a new Pylon site and launch the development server:

```sh
pylon init .
pylon serve
```

# Documentation

Configuration of Pylon is done through a [Rhai](https://rhai.rs/) script. This allows fine-grained control over different aspects of Pylon. Only a small amount of Pylon functionality is currently scriptable, but expansion is planned as more features are implemented. Check out the Rhai [language reference](https://rhai.rs/book/language/) for details on how to write Rhai scripts.

## Documents

Pylon pages are called "documents" which are modified [Markdown](https://www.markdownguide.org/) files that are split into two parts: frontmatter in [TOML format](https://toml.io/en/), and the Markdown content. Three pluses (`+++`) are used to delimit the frontmatter from the Markdown content.

Pylon will preserve the directory structure you provide in the `content` directory when rendering the documents to the `output` directory.

### Frontmatter

The frontmatter is used to associate some metadata with the page so Pylon knows how to render it properly. It can also be used to provide page-specific information for rendering.

All frontmatter keys are optional, and the default values are listed below:

```
+++
#
# template to use for rendering this document
#
# If not provided, Pylon will search for `default.tera` in the `templates`
# directory using the same directory structure as the source Markdown file.
# If no `default.tera` is found, then each parent directory is checked as
# well. If still no `default.tera` is found in any parent directories, then
# the build will fail.
#
template_name = "default.tera"

#
# keywords to associate with this page
#
# Keywords aren't yet used by Pylon, but they will be exported when
# running `pylon build --frontmatter`.
#
keywords = []

#
# custom data to provide to the rendering context
#
# Any data you want available when the page is rendered goes under
# the [meta] section, and can be accessed with {{ meta.keyname }}.
#
# [meta]
# example = "example"
+++

This is now the [Markdown](https://www.markdownguide.org/) section of the document.
```

### Internal Links

Linking to other documents can be accomplished prefixing a path to a Markdown file with `@/`. The path always starts from the _project root_ and will be automatically expanded to the appropriate URI when rendered:

```[my favorite post](@/blog/favorite/post.md)```


## Templates

Pylon uses [Tera](https://tera.netlify.app/) for it's template engine and provides a few extra builtin functions on top of what Tera already provides. These functions are available in `Tera` templates and within Markdown documents:

### include_file

Inlines the content of an entire file. The path must start with a slash (`/`) and is always relative from the project root.

```
{{ include_file( path = "/dir/file.ext" ) }}
```

### include_cmd
Inlines the output of a shell command (`cmd`). By default, `stdout` will be captured and used as the inlined data. This can be changed by including `$SCRATCH` somewhere in the shell command, which causes Pylon to generate a temporary file to be read from and then inlined. `cwd` is the current working directory to use for shell execution, must start with a slash (`/`), and is always relative from the project root.

```
{{ include_cmd( cwd = "/", cmd = "echo inline from stdout" ) }}
{{ include_cmd( cwd = "/some/dir", cmd = "echo inline from file > $SCRATCH" ) }}
```

## Shortcodes

Shortcodes are small template functions that can be used to generate HTML code directly from your Markdown documents. They exist as `.tera` files in the `templates/shortcodes` directory. If you are looking for reusable chunks to use in template files (_not_ Markdown files), check out the [partials](https://tera.netlify.app/docs/#include) and [macros](https://tera.netlify.app/docs/#include) docs for Tera.

There are two types of shortcodes:

* `inline`: similar to a function call and only allows strings as arguments
* `body`: allows arguments just like an `inline` shortcode, but it also allows multiple lines of Markdown to be included as an argument

### Example: Inline shortcode

**templates/shortcodes/custom_heading.tera**:

```tera
<h1 class="{{ class }}">{{ title }}</h1>
```

Usage in Markdown file:
```
{{ custom_heading(class = "bright-red", title = "My bright red heading!") }}
```

### Example: Body shortcode

The provided `body` will be rendered as Markdown and is accessible with `{{ body }}` in the shortcode source.

**templates/shortcodes/dialog.tera**:

```tera
<div>
  <h1>{{ heading }}</h1>
  <p>{{ body }}</p>
</div>
```

Usage in Markdown file:
```
{% dialog(heading = "Notice") %}

## Instructions for Windows users
...

## Instructions for Linux users
...

{% end %}
```

## Pipelines

When Pylon builds your site, it checks all the HTML tags for linked files (`href`, `src`, etc). If the linked file is not found, then an associated `pipeline` will be ran to generate this file. The pipeline can be simple, such as copying a file from some directory. It can also be complex and progressively build the file from a series of shell commands. Pipelines only operate on a single file at a time, and only on files that are linked directly in an HTML file. To copy batches of files without running a pipeline, use a [mount](#mounts) instead.

Pipelines are the last step in the build process, so all mounted directories have been copied, and all HTML files have been generated when the pipelines are ran. This allows other applications to parse the content as part of their build process (`tailwind` checks the `class` attributes on HTML tags to generate CSS, for example).

**Create a pipeline**:

```rhai
rules.add_pipeline(
  "",     // working directory
  "",     // glob to match linked files (in href, src, etc. attributes)
  []      // commands to run
);   
```

`working directory` can be either:

* Relative (from the Markdown parent directory) using `.` (dot). Subdirectories can be accessed using `./dir_name`.
* Absolute (from project root) using `/` (slash). Subdirectories can be accessed using `/dir_name`.

When using a _relative_ `working directory`, Pylon will lookup the Markdown file that the HTML file was generated from, and use the Markdown file parent directory. If the HTML file was mounted (as in, not generated from a Markdown file), then using a relative `working directory` will fail.

### Builtin Commands

Pipelines offer builtin commands for common tasks:

| Command | Description                                                          |
|---------|----------------------------------------------------------------------|
| `OP_COPY`  | Copies the file from some source location to the target location. |

### Shell Commands

To offer maximum customization, shell commands can be ran to generate files. Pylon provides tokens to use with your commands, which are replaced with appropriate information:

| Token      | Description                                                                                                                                                                                                                        |
|------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `$SOURCE`  | Absolute path to the source file being requested. Only applicable when using globs (`*`) in the pipeline.                                                                                                                          |
| `$TARGET`  | Absolute path to the target file in the `output` directory, that is: `$TARGET` will be reachable by the URI indicated in an HTML tag.                                                                                              |
| `$SCRATCH` | Absolute path to a temporary file that can be used as an intermediary when redirecting the output of multiple commands. Persists across the entire pipeline run, allowing multiple shell commands to access the same scratch file. |

### Example: Copy all colocated files of a single type

Colocation allows document-specific files to be stored alongside the Markdown document, making it easy to keep your files organized. To access colocated assets, we need to create a pipeline that uses a path relative to the Markdown file.

This example uses the builtin `OP_COPY` command to copy files from the source directory to the output directory.

Given this directory structure:

```
/content/
|-- blog/
    |-- page.md
    |-- assets/
        |-- sample.png
```

and a desired output directory of

```
/output/
|-- blog/
    |-- page.html     (containing <img src="assets/sample.png">)
    |-- assets/
        |-- sample.png
```

we can use this pipeline to copy the colocated files:

```rhai
rules.add_pipeline(
  ".",          // use the Markdown directory as the working directory
  "**/*.png",   // apply this pipeline to all .png files
  [
    OP_COPY     // run the OP_COPY builtin to copy the png file
  ]
);
```

This will result in `content/blog/assets/sample.png` being copied to `output/blog/assets/sample.png`.


### Example: Generate a CSS file using `Sass`

Pipelines were designed to allow integration of any tool that can be ran from CLI, making it easy to use whichever tooling you need to generate your site.

This example uses shell redirection and the `$TARGET` token to generate a site's CSS using the [Sass](https://sass-lang.com/) preprocessor.

Given this directory structure:

```
/web/
|-- styles/
    |-- a.scss
    |-- b.scss
    |-- c.scss
    |-- main.scss   (we'll assume `main.scss` imports `a` `b` and `c`)
```

and a desired output directory of

```
/output/
|-- index.html     (containing <link href="/style.css" rel="stylesheet">)
|-- style.css
```

we can use this pipeline to generate the `style.css` file:

```rhai
rules.add_pipeline(
  "/web/styles",                // working directory is <project root>/web/styles
  "/style.css",                 // only run this pipeline when this exact file is linked in the HTML
  [
    "sass main.scss > $TARGET"  // run `sass` on the `main.scss` file, and output the resulting
  ]                             // CSS code to the target file (<output root>/style.css)
);
```

This will result in the CSS being generated by Sass and saved to `/output/style.css`.

### Example: Modify SVG files and then minify the result

Instead of using an exported version of some file as a colocated asset, we can use the source file and then compile it on demand. This removes the need to have separate "exported" and "source" versions of files, making it easier to manage content that changes frequently.

This example modifies SVG files by setting a custom "brand" color, and then minifying the files with [usvg](https://github.com/RazrFalcon/resvg/tree/master/usvg).

Given this directory structure:

```
/img/
|-- logo.svg       (containing the color #AABBCC)
|-- popup.svg      (containing the color #AABBCC)
```

and a desired output directory of

```
/output/
|-- index.html         (containing <img src="/static/img/logo.svg"> <img src="/static/img/popup.svg">)
|-- static/
    |-- img/
        |-- logo.svg       (containing the color #123456)
        |-- popup.svg      (containing the color #123456)
```

we can use this pipeline to modify and generate the files:

```rhai
rules.add_pipeline(
  "/img",                  // working directory is <project root>/img
  "/static/img/*.svg",     // only run this pipeline on SVG files requested from `/static/img`
  [
    "sed 's/#AABBCC/#123456/g' $SOURCE > $SCRATCH",  // run `sed` to replace the color in the SVG file,
                                                     // and redirect to a scratch file

    "usvg $SCRATCH $TARGET"    // minify the scratch file (which now has color #123456)
                               // with `usvg` and output to target
  ]
);
```

This will result in minified `/output/logo.svg` and `/output/popup.svg`, both having color `#AABBCC` replaced with `#DDEEFF`.

## Mounts

Mounts allow you to "mount", or copy, the contents of an entire directory into your output directory. Mounts are convenient for copying `static` resources that rarely (if ever) change. All directores mounted directories are relative to the project root.

**Example**:

We want to mount `wwwroot` directly to the output directory

```
/web/
|-- wwwroot/
    |-- logo.png
    |-- extra/
        |-- data.txt
```

so we can use `.mount`

```rhai
rules.mount("web/wwwroot");
```

and we will have the following output directory when the site builds:

```
/output/
|-- logo.png
|-- extra/
    |-- data.txt
```

## Watches

When running the development server, Pylon will watch the `output`, `content`, and `template` directories, and the `site-rules.rhai` script. Whenever a watch target is updated, the server will rebuild the necessary assets and refresh the page. Additional watch targets can be added with the `rules.watch` function:

```rhai
// watch a file
rules.watch("package.json");

// watch a directory
rules.watch("static");
```

## Lints

Prior to building the site, Pylon can check the Markdown documents for issues that you specify. There are two lint modes available:

* `WARN` will log a warning during the build
* `DENY` will log an error and cancel the build

Lints are defined with a closure that has access to the current document being processed. In addition to the fields available in the [frontmatter](#frontmatter), lints can also use these document functions:

| Function         | Description                                                    |
|------------------|----------------------------------------------------------------|
| `doc.uri()`      | Returns the URI of the generated page (`/some/path/page.html`) |

**Add a lint**:

```rhai
rules.add_lint(
  MODE,       // either WARN or DENY
  ""          // the message to be displayed if this lint is triggered
  "",         // a file glob for matching Markdown documents 
  |doc| {}    // a closure that returns `true` when the lint fails, and `false` if it passes
);
```

### Example: Emit a warning if blog posts do not have an author

```rhai
rules.add_lint(WARN, "Missing author", "/blog/**/*.md", |doc| {
  // We check the `author` field in the metadata and ensure it is not blank,
  // and we also check if the `author` field exists at all. If the `author` field
  // is missing, it's type will be a unit `()`.
  doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
});
```

## Global Context

Site-wide data can be set for all documents via the global context. When used, the data will be available under the `global` key in the templates:

```rhai
rules.set_global_context(
  #{
    nav_items: [
      #{
        url: "/",
        title: "Home"
      },
      #{
        url: "/blog/",
        title: "Blog"
      },
      #{
        url: "/about/",
        title: "About"
      }
    ],
  }
);
```

## Document Context

Per-document data can be set for specific documents based on a glob pattern. The context will be available in templates using the identifier provided in the closure. However, using the same name as a [builtin context identifier](#context-builtins) is an error and the build will be aborted.

**Add context**:

```rhai
rules.add_doc_context(
  "",             // file glob
  |doc| {         // closure to generate the context
    new_context(  // use the `new_context` function to create a new context
      #{}         // object containing custom context data
    )
});
```

### Example: Add an alert message to all blog pages

You might have an alert system built into your templates that can display information such as an upcoming event. It may not be appropriate to display it on all pages of the site using a [global context](#global-context), and you only want it displayed on blog posts:

```rhai
rules.add_doc_context("/blog/**/*.md", |doc| {
  new_context(#{
    alert: "Don't forget to join the live stream happening this Friday!"
  })
});
```

The `alert` message can now be accessed within templates as `{{ alert }}`, but only for the documents that exist in the `/blog` directory.

### Context Builtins

Pylon provides some basic information to each page when rendering:

| Identifier  | Description                                                                       |
|-------------|-----------------------------------------------------------------------------------|
| `content`   | The rendered Markdown for the page                                                |
| `global`    | [Global context](#global-context) provided via script                             |
| `library`   | All documents in the site                                                         |
| `doc`       | Container for document related information                                        |
| `doc.path`  | On-disk path to the Markdown file for the document                                |
| `doc.uri`   | The URI to access the generated page (`/example/index.html`)                      |
| `doc.meta`  | Any metadata added using the `[meta]` section in the [frontmatter](#frontmatter)  |
| `doc.toc`   | Rendered table of contents                                                        |

## Syntax Highlighting

Syntax highlighting is themed with Sublime text `tmTheme` files. Pylon can convert a `tmTheme` file to CSS using `pylon build-syntax-theme`. Currently, only class-based syntax highlighting is supported, so the generated CSS file will need to be manually included in your site.

# Roadmap

You can check the detailed status of all planned features for the next release using the `milestones` in the issue tracker. Important features that are currently planned:

- [ ] Pagination
- [ ] Launch external source watchers
- [ ] Scan CSS files for linked files
- [ ] Integrated Preprocessors
- [ ] Integrated Postprocessor
- [ ] Link checker
- [ ] Generate RSS feeds
- [ ] Generate sitemap
- [ ] Proper logging
