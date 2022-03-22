// Copied from `poem` source and modified to inject script tags into the files.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use poem::{
    error::StaticFileError,
    http::{header, Method},
    web::{StaticFileRequest, StaticFileResponse},
    Body, Endpoint, FromRequest, IntoResponse, Request, Response, Result,
};

struct DirectoryTemplate<'a> {
    path: &'a str,
    files: Vec<FileRef>,
}

impl<'a> DirectoryTemplate<'a> {
    fn render(&self) -> String {
        let mut s = format!(
            r#"
        <html>
            <head>
            <title>Index of {}</title>
        </head>
        <body>
        <h1>Index of /{}</h1>
        <ul>"#,
            self.path, self.path
        );

        for file in &self.files {
            if file.is_dir {
                s.push_str(&format!(
                    r#"<li><a href="{}">{}/</a></li>"#,
                    file.url, file.filename
                ));
            } else {
                s.push_str(&format!(
                    r#"<li><a href="{}">{}</a></li>"#,
                    file.url, file.filename
                ));
            }
        }

        s.push_str(
            r#"</ul>
        </body>
        </html>"#,
        );

        s
    }
}

struct FileRef {
    url: String,
    filename: String,
    is_dir: bool,
}

/// Static files handling service.
///
/// # Errors
///
/// - [`StaticFileError`]
#[cfg_attr(docsrs, doc(cfg(feature = "static-files")))]
pub struct StaticFilesEndpoint {
    path: PathBuf,
    show_files_listing: bool,
    index_file: Option<String>,
    prefer_utf8: bool,
    inject_script: Option<String>,
    load_file_on_slash: Option<String>,
}

impl StaticFilesEndpoint {
    /// Create new static files service for a specified base directory.
    ///
    /// # Example
    ///
    /// ```
    /// use poem::{endpoint::StaticFilesEndpoint, Route};
    ///
    /// let app = Route::new().nest(
    ///     "/files",
    ///     StaticFilesEndpoint::new("/etc/www")
    ///         .show_files_listing()
    ///         .index_file("index.html"),
    /// );
    /// ```
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            show_files_listing: false,
            index_file: None,
            prefer_utf8: true,
            inject_script: None,
            load_file_on_slash: None,
        }
    }

    #[must_use]
    pub fn inject_script<S: Into<String>>(self, script: S) -> Self {
        Self {
            inject_script: Some(script.into()),
            ..self
        }
    }

    #[must_use]
    pub fn load_file_on_slash(self, file: impl Into<String>) -> Self {
        Self {
            load_file_on_slash: Some(file.into()),
            ..self
        }
    }

    /// Show files listing for directories.
    ///
    /// By default show files listing is disabled.
    #[must_use]
    pub fn show_files_listing(self) -> Self {
        Self {
            show_files_listing: true,
            ..self
        }
    }

    /// Set index file
    ///
    /// Shows specific index file for directories instead of showing files
    /// listing.
    ///
    /// If the index file is not found, files listing is shown as a fallback if
    /// Files::show_files_listing() is set.
    #[must_use]
    pub fn index_file(self, index: impl Into<String>) -> Self {
        Self {
            index_file: Some(index.into()),
            ..self
        }
    }

    /// Specifies whether text responses should signal a UTF-8 encoding.
    ///
    /// Default is `true`.
    #[must_use]
    pub fn prefer_utf8(self, value: bool) -> Self {
        Self {
            prefer_utf8: value,
            ..self
        }
    }
}

async fn make_injected_response<P: AsRef<Path>>(
    req: &Request,
    inject: &Option<String>,
    file_path: P,
    prefer_utf8: bool,
) -> Result<Response> {
    let file_path = file_path.as_ref();
    let response = StaticFileRequest::from_request_without_body(&req)
        .await?
        .create_response(&file_path, prefer_utf8)?;
    if file_path.extension() != Some(OsStr::new("html")) {
        return Ok(response.into_response());
    }
    let response = {
        if let StaticFileResponse::Ok {
            body,
            content_type,
            etag,
            last_modified,
            content_range,
        } = response
        {
            let string_body = body.into_string().await?;
            let new_body = match inject {
                Some(script) => Body::from_string(format!("{}{}", string_body, script)),
                None => Body::from_string(string_body.clone()),
            };
            StaticFileResponse::Ok {
                body: new_body,
                content_type,
                etag,
                last_modified,
                content_range,
            }
        } else {
            StaticFileResponse::NotModified
        }
    };
    return Ok(response.into_response());
}

#[async_trait::async_trait]
impl Endpoint for StaticFilesEndpoint {
    type Output = Response;

    async fn call(&self, req: Request) -> Result<Self::Output> {
        if req.method() != Method::GET {
            return Err(StaticFileError::MethodNotAllowed(req.method().clone()).into());
        }

        let path = {
            let path = req.uri().path().trim_start_matches('/');
            if path.ends_with('/') {
                if let Some(file) = &self.load_file_on_slash {
                    std::borrow::Cow::Owned(format!("{path}{file}"))
                } else {
                    std::borrow::Cow::Borrowed(path)
                }
            } else {
                std::borrow::Cow::Borrowed(path)
            }
        };

        let path = percent_encoding::percent_decode_str(&path)
            .decode_utf8()
            .map_err(|_| StaticFileError::InvalidPath)?;

        let mut file_path = self.path.clone();
        for p in Path::new(&*path) {
            if p == OsStr::new(".") {
                continue;
            } else if p == OsStr::new("..") {
                file_path.pop();
            } else {
                file_path.push(&p);
            }
        }

        if !file_path.starts_with(&self.path) {
            return Err(StaticFileError::Forbidden(file_path.display().to_string()).into());
        }

        if !file_path.exists() {
            return Err(StaticFileError::NotFound.into());
        }

        if file_path.is_file() {
            return Ok(make_injected_response(
                &req,
                &self.inject_script,
                file_path,
                self.prefer_utf8,
            )
            .await?
            .into_response());
        } else {
            if let Some(index_file) = &self.index_file {
                let index_path = file_path.join(index_file);
                if index_path.is_file() {
                    return Ok(make_injected_response(
                        &req,
                        &self.inject_script,
                        file_path,
                        self.prefer_utf8,
                    )
                    .await?
                    .into_response());
                }
            }

            if self.show_files_listing {
                let read_dir = file_path.read_dir().map_err(StaticFileError::Io)?;
                let mut template = DirectoryTemplate {
                    path: &*path,
                    files: Vec::new(),
                };

                for res in read_dir {
                    let entry = res.map_err(StaticFileError::Io)?;

                    if let Some(filename) = entry.file_name().to_str() {
                        let mut base_url = req.original_uri().path().to_string();
                        if !base_url.ends_with('/') {
                            base_url.push('/');
                        }
                        template.files.push(FileRef {
                            url: format!("{}{}", base_url, filename),
                            filename: filename.to_string(),
                            is_dir: entry.path().is_dir(),
                        });
                    }
                }

                let html = template.render();
                Ok(Response::builder()
                    .header(header::CONTENT_TYPE, mime::TEXT_HTML_UTF_8.as_ref())
                    .body(Body::from_string(html)))
            } else {
                Err(StaticFileError::NotFound.into())
            }
        }
    }
}
