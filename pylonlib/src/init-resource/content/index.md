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
template_name = "content/default.tera"

#
# (UNUSED) keywords to associate with this document
#
# Keywords aren't yet used by Pylon, but they will be exported when
# running `pylon build --frontmatter`.
#
keywords = []

#
# (UNUSED) whether this document should be index
#
# This value is not yet used by Pylon, but will be exported when
# running `pylon build --frontmatter`.
#
searchable = true

#
# whether to generate breadcrumbs for this document
#
# When `true`, breadcrumbs will be available as an array of documents,
# and can be accessed in the template with {{ breadcrumbs }}. The last
# entry in the array is always the current document. The remaining
# breadcrumbs will be `index.md` documents, starting from the directory of
# the current document, and traversing all directories until the root of
# the `src` directory is reached. Only `index.md` documents that actually
# exist will be present in the array.
#
use_breadcrumbs = false

#
# whether this document will be generated in build
#
# When `true`, this document will be rendered during a site build. When
# running the development server, this value is ignored and the document
# will always be generated (in order to preview work).
published = false

#
# custom data to provide to the rendering context
#
# Any data you want available when the document is rendered goes under
# the [meta] section, and can be accessed with {{ meta.keyname }}.
#
# [meta]
# example = "example"
+++

# This is a sample document.

{{ big( text="shortcode text from markdown doc" )}}