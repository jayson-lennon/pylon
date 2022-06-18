+++
#
# (OPTIONAL) template to use for rendering this document
#
# If not provided, Pylon will search for `page.tera` in the `templates`
# directory and each parent directory. If no `page.tera` is found, then
# the build will fail.
#
# template_name = "templates/content/default.tera"

#
# (OPTIONAL) keywords to associate with this page
#
# Keywords aren't used directly by Pylon, but can be used with custom
# search engines when exporting the frontmatter
#
# keywords = ["sample 1", "sample 2"]


#
# (OPTIONAL) custom data to provide to the rendering context
#
# Any data you want available when the page is rendered goes under
# the [meta] section and can be accessed with {{ meta.custom_key }}
#
# [meta]
# custom_key = "sample"
+++


# This is a sample document.

{{ big( text="shortcode text from markdown doc" )}}