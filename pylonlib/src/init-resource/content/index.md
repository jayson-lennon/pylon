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
# Keywords aren't yet used by Pylon, but can be used by custom
# scripts when exporting the frontmatter.
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

# This is a sample document.

{{ big( text="shortcode text from markdown doc" )}}