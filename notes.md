path rewrite:

SystemPath - maybe don't allow creation except from a page?. this will prevent system paths from being generated that point to non-existent items. the only exception is transforming URIs into an asset path (because the web page may not use a properly formed URI).
 - base
 - absolute
 - file

Uri ( newtype)
- linked asset uri as seen on the document

Page
 - source_path() -> SystemPath: file path residing on system
 - target_path() -> SystemPath: if (use index == true && this page name != index) -> add parent folder name and return an index.html path
 - asset_path(uri) -> SystemPath: file path residing on system
 - asset_target(uri) -> SystemPath : if (use index == true && this page name != index) -> prepend ../


when discovering assets, we have access to the page Uri and the asset uri

use the page uri to query the Page from the PageStore and then
determine the system path of the asset using the page

