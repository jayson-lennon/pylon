ssb - Static Site Builder

TODO: how to handle links?
TODO: should pipelines get ran on all HTML files or just crawl from index.html?
TODO (later): caching artifacts
TODO (later): store everything in sqlite database for time travel

# Overall build

1. Iterate through files all files to build up a directory tree
2. Render each file with appropriate context data with a 1:1 file tree to URL mapping. Context data includes:
  * Frontmatter
  * slug
  * URL
3. Parse HTML of generated `index.html` file
  1. Discover all external assets
  2. Query each asset in the asset database
  3. Run the pipeline for each asset
  4. Abort if asset does not have at least one pipeline

## Asset Query
Database contains regex for every asset. When asset is discovered, it gets ran through all regexes in database and must match at least one regex. Once matched, pick the highest priority regex and use the associated pipeline regex to generate the asset and save it at the correct location.

Potential issues with this system: target regexes can have more than one matching pipeline. Pipelines need a priority number and should be sorted from least greedy to most greedy. For example, `**/*.jpg` matches all `jpg` files anywhere so should be a lower priority than `**/vacation/*.jpg` which only matches `jpg` files in a `vacation` directory.

All assets must have a link in a rendered HTML file.

## Asset transformations

### one-to-one
One to one asset transformations are simple. Given a target asset, run the pipeline on the source file.

### many-to-one
Many-to-one asset transformations are similar to one-to-one. Given a target asset, run the pipeline on all source files (ie: `sass *.scss > output`).

### many-to-many
Many-to-many asset transformations are just one-to-one transformations performed in a batch.

## Pipelines

Pipelines consist of 2 parts:
  * A glob expression that matches a _target_ asset requested in an HTML page
  * An operation to perform

### Glob expander
Need to write a library to expand globs. _IMPORTANT NOTE_: all file paths must be absolute starting with a `/`. They can begin from project root or output directory. This is required in order to properly match the regexes used in the database.

Examples
```
match all png files in the blog folder, recursive
/blog/**/*.png -> \/blog/\.*[^\/]*.png
        /blog/    \/blog\/
        **/       .*
        *.png     [^\/]*.png

match all png files directly in blog folder
/blog/*.png -> \/blog\/[^\/]*.png
        /blog/    \/blog\/
        *.png     [^\/]*.png

match all png files in root directory
*.png -> ^[^\/]*.png

match all png files in all directories
**/*.png -> .*[^\/]*.png
```

Implementation

| Glob   | Regex       | Notes                                                  |
|--------|-------------|--------------------------------------------------------|
| `**`   | `.*`        |                                                        |
| `*`    | `[^\/]*`    |                                                        |
| `?`    | `.`         |                                                        | 


### Post Processors

Post processors can apply transformations automatically at the end of a pipeline. This is for global changes. In order to allow composition with shell commands, transformers can only be applied on single files.

*Post Processor variables*
```
{INPUT}   input file
{OUTPUT}  output file
```

INPUT and OUTPUT files will ge temp generated names for the command to utilize. The system will use these files to push data through multiple commands.

*Watermark images*
_Perspective from user_
post_processor_name: watermark images
target_asset: `**/*.png`
processor: SHELL
|- `convert {INPUT} watermark.png -o {OUTPUT}`

_Perspective from system implementation_
Asset requested: `hello.png`                 discovered in HTML file
From: `/blog/greetings`                      derived from HTML file location
Target: `/blog/greetings/hello.png`          derived from parent directory and asset
Target glob: [`**/*.png`]                    queried from database
Picked glob: `**/*.png`                      hooks are processed first
hook: `watermark images`                     queried from database
apply processor                              queried from database


### Pipeline Examples

The `autorun` field in the pipeline is utilized for source file monitoring. It can either be `<GLOB>` to use the `target_asset` glob, or it can be a different glob. When using the `target_asset` glob, the system will simply search the source directory with the glob (since the source and target directory structures are the same).

*Pipeline variables*

```
{SOURCE}      full path to source file
{SRC_PARENT}  full path to source file parent directory
{TARGET}      full path to target file (includes ouput directory)
{TMPDIR}      a temporary directory to use for building artifacts
              - This is a randomly generated directory for each
                request and will be deleted automatically once
                the operation completes.
```

*Copy image*

_Perspective from user_
pipeline_name: copy blog images
target_asset : `/blog/**/*.png`
operation: COPY (builtin)
autorun: `<GLOB>`

_Perspective from system implementation_
Asset requested: `hello.png`                 discovered in HTML file
From: `/blog/greetings`                      derived from HTML file location
Target: `/blog/greetings/hello.png`          derived from parent directory and asset
Target glob: [`/blog/**/*.png`, `*.png`]     queried from database
Picked glob: `/blog/**/*.png`                highest priority chosen from database
pipeline: `copy blog images`                 queried from database
autorun: `<GLOB>`                            queried from database
Operation: COPY                              queried from database (builtin operation)
|- Source path: `/blog/greetings/hello.png`  calculated from `asset requested` and `from`
|- Do: copy `{SOURCE}` to `{TARGET}`


*Custom pipeline*

_Perspective from user_
pipeline_name: build diagram scripts
target_asset : `**/diagram.js`
operation: SHELL
autorun: `**/_src/*.ts`

_Perspective from system_
Autorun: `**/_src/*.ts`                            monitor these files for changes
Asset requested: `diagram.js`                      discovered in HTML file
From: `/blog/algo/bubblesort`                      derived from HTML file location
Target: `/blog/algo/bubblesort/diagram.js`         derived from previous info
Target glob: [`**/diagram.js`]                     queried from database
Picked glob: `**/diagram.js`                       highest priority chosen from database
pipeline: `build diagram scripts`                  queried from database
Operation: SHELL                                   queried from database
Do:
  ```sh
  cat {SRC_PARENT}/diag-*.ts {TMPDIR}/diagram.ts
  esbuild {TMPDIR}/diagram.ts -o {TARGET}
  ```
  {TMPDIR} automatically deleted after execution of shell script


## Asset Control

Assets can be "expanded" and "integrated" (terms TBD) from and to the database. Expanding assets from the database will write them to disk for editing. Integrating will push copies of the files to the database for later querying. This might take a lot of space, need to investigate differential storage. Initial implementation can just use versioned blobs.


