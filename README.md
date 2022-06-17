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

# Documentation

Configuration of Pylon is done through a [rhai](https://rhai.rs/) script.

## Documents

Pylon pages are called "documents" and are modified [Markdown](https://www.markdownguide.org/) files that are split into two parts: frontmatter in [TOML format](https://toml.io/en/), and the Markdown content. Three pluses (`+++`) are used to delimit the frontmatter from the markdown content. Pylon will preserve the directory structure you provide in the `content` directory when rendering the documents.

### Frontmatter

Pylon currently has very few frontmatter keys, and it is expected that this list will grow as more features are added.

Here is a document showing all possible frontmatter keys:

```
+++
#
# (OPTIONAL) template to use for rendering this document
#
# If not provided, Pylon will search for `page.tera` in the `templates`
# directory and each parent directory. If no `page.tera` is found, then
# the build will fail.
#
template_name = "templates/sample/landing.tera"

#
# (OPTIONAL) keywords to associate with this page
#
# Keywords aren't used directly by Pylon, but can be used with custom
# search engines when exporting the frontmatter
#
keywords = ["sample", "data"]


#
# (OPTIONAL) custom data to provide to the rendering context
#
# Any data you want available when the page is rendered goes under
# the [meta] section and can be accessed with {{ meta.custom_key }}
#
[meta]
custom_key = "ok"
+++

This is now the [Markdown](https://www.markdownguide.org/) section of the document.

```

## Pipelines

When Pylon builds your site, it checks all the HTML tags for linked files. If the linked file is not found, then an associated `pipeline` will be ran to generate this file. The pipeline can be as simple as copying a file from some directory, or it can progressively build the file from a series of shell commands. Pipelines only operate on a single file at a time and only on files that are linked directly in an HTML file. To copy batches of files without running a pipeline, use a [mount](#mounts) instead.

Pipelines are the last step in the build process, so all mounted directories have been copied and all HTML files have been generated when the pipelines are ran. This allows other applications to parse the content as part of their build process (`tailwind` checks the `class` attributes on tags to generate minimal CSS, for example).

**Syntax**:

```rhai
rules.add_pipeline(
  "",     // working directory
  "",     // glob
  []      // commands
);   
```

`working directory` can be either:

* Relative (from the Markdown parent directory) using `.` (dot) or `./dir_name`
* Absolute (from project root) using `/` (slash) or `/dir_name`

### Builtin Commands

Pipelines offer builtin commands for common tasks:

| Command | Description                                                       |
|---------|-------------------------------------------------------------------|
| `COPY`  | Copies the file from some source location to the target location. |

### Shell Commands

To offer maximum customization, shell commands can be ran to generate linked files. Special tokens may be used within your commands and they will be replaced with appropriate information:

| Token      | Description                                                                                                                                                                                                     |
|------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `$SOURCE`  | Absolute path to the source file being requested. Only applicable when using glob patterns (`*`). `$SOURCE` is the concatenation of `working directory` and the URI supplied in the HTML tag.                   |
| `$TARGET`  | Absolute path to the target file in the output site directory. `$TARGET` is the path where the file should reside in order to be reachable by according to the URI in the HTML tag.                             |
| `$SCRATCH` | Temporary file that can be used as an intermediary when redirecting the output of multiple commands. Persists across the entire pipeline run, allowing multiple shell commands to access the same scratch file. |


### Example: Copy all colocated files of a single type

Colocation allows page-specific files to be stored alongside the Markdown document. This makes it easy to organize files. To access colocated assets, we need to create a pipeline that uses a path relative to the Markdown file.

This example uses the builtin `COPY` command to copy files from the source directory to the output directory.

Given this directory structure:

```
/content/
|-- blog/
    |-- page.md
    |-- assets/
        |-- sample.png
```

and a desired output directory of:

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
  ".",          // use the Markdown source directory for the working directory
  "**/*.png",   // apply this pipeline to all .png files
  [
    COPY        // run the COPY builtin to copy the png file
  ]
);
```

which results in `content/blog/assets/sample.png` being copied to `output/blog/assets/sample.png`.


### Example: Generate a CSS file using `sass`

There are a _lot_ of web development tools, and pipelines can be used to execute whichever tools make sense for your project.

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

and a desired output directory of:
```
/output/
|-- index.html     (containing <link href="/style.css">)
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

which results in `/output/style.css` being generated by `sass`.

### Example: Modify SVG files and then minify the result

Instead of using an exported version of some file as a colocated asset, we can use the source file and compile it on demand. This removes the need to have separate "exported" and "source" versions of files.

This example modifies SVG files by setting a custom brand color, and then minifying the file with [usvg](https://github.com/RazrFalcon/resvg/tree/master/usvg).

Given this directory structure:

```
/img/
|-- logo.svg       (containing the color #AABBCC)
|-- popup.svg      (containing the color #AABBCC)
```

and a desired output directory of:
```
/output/
|-- index.html     (containing <img src="/img/logo.svg"> <img src="/img/popup.svg">)
|-- logo.svg       (containing the color #123456)
|-- popup.svg      (containing the color #123456)
```

we can use this pipeline to modify and generate the files:

```rhai
rules.add_pipeline(
  "/img",                       // working directory is <project root>
  "/img/*.svg",                 // only run this pipeline on svg files requested from the `img` directory
  [
    "sed 's/#AABBCC/#123456/g' $SOURCE > $SCRATCH",  // run `sed` to replace the color in the svg file,
                                                     // and redirect to a scratch file

    "usvg $SCRATCH $TARGET"     // minify the scratch file (which now has color #123456)
                                // with `usvg` and output to target
  ]
);
```

which results in minified `/output/logo.svg` and `/output/popup.svg` being generated with `#AABBCC` replaced with `#DDEEFF`.

## Mounts

Mounts allow you to "mount", or copy, the contents of an entire directory into your output directory. Mounts are convenient for copying `static` resources that rarely (if ever) change. All directores used with `add_mount` are relative to the project root.

**Example**:

If we have this in our project directory:

```
/web/
|-- wwwroot/
    |-- logo.png
    |-- extra/
        |-- data.txt
```

and this mount:

```rhai
rules.mount("web/wwwroot");
```

then we will have the following output directory when the site builds:

```
/output/
|-- logo.png
|-- extra/
    |-- data.txt
```

## Watches

When running the development server, Pylon will watch the `output`, `content`, and `template` directories, and the `site-rule.rhai` script. Whenever a watch target is updated, the server will rebuild the necessary assets and refresh the page. Additional watch targets can be added to the `site-rules.rhai` script:

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

Lints are defined with a closure that has access to the current page being linted. See the [Frontmatter](#frontmatter) section for details on which fields are available.

**Syntax**:

```rhai
rules.add_lint(
  MODE,       // either WARN or DENY
  ""          // the message to be displayed if this lint is triggered
  "",         // a file glob for matching Markdown documents 
  |page| {}   // a closure that returns `true` when the lint fails, and `false` if it passes
);
```

### Example: Emit a warning if blog posts do not have an author

```rhai
rules.add_lint(WARN, "Missing author", "/blog/**/*.md", |page| {
  // We check the `author` field in the metadata and ensure it is not blank,
  // and we also check if the `author` field exists at all. If the `author` field
  // is missing, it's type will be a unit `()`.
  page.meta("author") == "" || type_of(page.meta("author")) == "()"
});
```

## Global Context

Site-wide data can be set for all pages via the global context. When used, the data will be available under the `global` key in the templates:

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

## Page Context

Per-page data can be set for specific pages based on a glob pattern. The context will be available in templates using the identifier given in the closure. However, using the same name as a [builtin context identifier](#context-builtins) is an error and the build will be aborted.

**Syntax**

```rhai
rules.add_page_context(
  "",           // file glob
  |page| {      // closure to generate the context
    new_context(  // use the `new_context` function to create a new context
      #{}         // object containing custom context data
    )
});
```

### Example: Add an alert message to all blog pages

You might have an alert system built into your templates that can display information such as an upcoming event. It may not be appropriate to display it on all pages of the site using a [global context](#global-context), and you only want it displayed on blog posts:

```rhai
rules.add_page_context("/blog/**/*.md", |page| {
  new_context(#{
    alert: "Don't forget to join the live stream happening this Friday!"
  })
});
```

The `alert` message can now be accessed within templates as `{{ alert }}`, but only for the pages that exist in the `/blog` directory.

### Context Builtins

Pylon provides some basic information to each page when rendering:

| Identifier  | Description                                                                       |
|-------------|-----------------------------------------------------------------------------------|
| `content`   | The rendered Markdown for the page                                                |
| `global`    | [Global context](#global-context) provided via script                             |
| `library`   | All pages in the site                                                             |
| `page`      | Container for page related information                                            |
| `page.path` | On-disk path to the Markdown file for the page                                    |
| `page.uri`  | The URI to access the generated page (`/example/index.html`)                      |
| `page.meta` | Any metadata added using the `[meta]` section in the [frontmatter](#frontmatter)  |
| `page.toc`  | Rendered table of contents                                                        |

## Templates

Pylon uses [Tera](https://tera.netlify.app/) for it's template engine and provides a few extra builtin functions on top of what Tera already provides. These functions are available in `Tera` templates, and within Markdown documents.

### include_file

Includes a file directly in the template. The path must start with a slash (`/`) and is always relative from the project root.

```
include_file( path = "/dir/file.ext" )
```

### include_cmd
Run a `cmd` in a shell, capture the output, and include it in the template. The `$SCRATCH` token can be used to create a temporary file, which will then be used as the output instead of `stdout`. `cwd` is the current working directory to use for shell execution, must start with a slash (`/`), and is always relative from the project root.

```
include_cmd( cwd = "/some/dir", cmd = "some shell command" )
```

## Shortcodes

Shortcodes are small template functions that can be used to generate HTML code directly from your Markdown documents. They exist as `.tera` files in the `templates/shortcodes` directory. If you are looking for reusable chunks for usage in template files (_not_ Markdown files), check out [partials](https://tera.netlify.app/docs/#include) and [macros](https://tera.netlify.app/docs/#include).

There are two types of shortcodes:

* `inline`: similar to a function call and only allows strings as arguments
* `body`: allows arguments just like an `inline` shortcode, but it also allows additional lines within the "body" of the shortcode

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

The provided `body` will be rendered as Markdown and is accessible as `{{ body }}` within the shortcode.

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
