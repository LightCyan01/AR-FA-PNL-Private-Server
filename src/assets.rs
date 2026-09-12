use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use bytes::Bytes;
use hyper::{
    body::Incoming,
    http::{header, Method, Request, Response, StatusCode},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio_util::io::ReaderStream;

use crate::{
    config::LoadedConfig,
    transport::{full, stream, AppBody},
};

#[derive(Clone)]
pub struct AssetService {
    config: Arc<LoadedConfig>,
    files: Arc<HashMap<String, PathBuf>>,
}

impl AssetService {
    pub async fn load(config: Arc<LoadedConfig>) -> Result<Self, std::io::Error> {
        let catalog: serde_json::Value =
            serde_json::from_slice(&fs::read(&config.config.paths.catalog).await?)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let mut files = HashMap::from([
            ("manifest.json".into(), config.config.paths.manifest.clone()),
            ("catalog.json".into(), config.config.paths.catalog.clone()),
        ]);
        if let Some(rows) = catalog
            .get("_fileCatalog")
            .and_then(|value| value.get("_bundles"))
            .and_then(serde_json::Value::as_array)
        {
            for row in rows {
                let Some(relative) = row.get("_relativePath").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(bundle_name) = row.get("_bundleName").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(hash) = row.get("_hash").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let filename = Path::new(relative)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(relative);
                let candidates = [
                    Some(config.config.paths.remote_bundle_root.join(filename)),
                    Some(
                        config
                            .config
                            .paths
                            .asset_root
                            .join(format!("{bundle_name}_{hash}")),
                    ),
                    config
                        .config
                        .paths
                        .embedded_bundle_root
                        .as_ref()
                        .map(|root| root.join(filename)),
                ];
                if let Some(path) = candidates.into_iter().flatten().find(|path| path.is_file()) {
                    files.insert(filename.to_owned(), path);
                }
            }
        }
        Ok(Self {
            config,
            files: Arc::new(files),
        })
    }

    pub async fn serve(&self, request: Request<Incoming>) -> Response<AppBody> {
        let method = request.method().clone();
        let route = request.uri().path().to_owned();
        let result = self.serve_inner(request).await;
        let response_bytes = result
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        tracing::info!(event = "asset_request", method = %method, route = %route, status = result.status().as_u16(), response_bytes, redacted = true);
        result
    }

    async fn serve_inner(&self, request: Request<Incoming>) -> Response<AppBody> {
        if request.method() != Method::GET && request.method() != Method::HEAD {
            return simple_response(
                StatusCode::METHOD_NOT_ALLOWED,
                b"asset method not allowed\n",
                Some("GET, HEAD"),
            );
        }
        let Some(path) = self.resolve(request.uri().path()) else {
            return simple_response(StatusCode::NOT_FOUND, b"asset not found\n", None);
        };
        let metadata = match fs::metadata(&path).await {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => return simple_response(StatusCode::NOT_FOUND, b"asset not found\n", None),
        };
        let Some((start, end, status)) =
            requested_range(request.headers().get(header::RANGE), metadata.len())
        else {
            let mut response = Response::new(full(Bytes::new()));
            *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                header::HeaderValue::from_str(&format!("bytes */{}", metadata.len())).unwrap(),
            );
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("0"),
            );
            return response;
        };
        let length = if metadata.len() == 0 {
            0
        } else {
            end - start + 1
        };
        let mut response = if request.method() == Method::HEAD {
            Response::new(full(Bytes::new()))
        } else {
            let file = match fs::File::open(&path).await {
                Ok(mut file) => {
                    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                        return simple_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            b"asset read failed\n",
                            None,
                        );
                    }
                    file
                }
                Err(_) => {
                    return simple_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        b"asset read failed\n",
                        None,
                    )
                }
            };
            Response::new(stream(ReaderStream::new(file.take(length))))
        };
        *response.status_mut() = status;
        response.headers_mut().insert(
            header::ACCEPT_RANGES,
            header::HeaderValue::from_static("bytes"),
        );
        response.headers_mut().insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from_str(&length.to_string()).unwrap(),
        );
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                header::HeaderValue::from_static("application/json")
            } else {
                header::HeaderValue::from_static("application/octet-stream")
            },
        );
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let modified_seconds = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        response.headers_mut().insert(
            header::ETAG,
            header::HeaderValue::from_str(&format!(
                "W/\"{:x}-{:x}\"",
                metadata.len(),
                modified_seconds
            ))
            .unwrap(),
        );
        response.headers_mut().insert(
            header::LAST_MODIFIED,
            header::HeaderValue::from_str(&httpdate::fmt_http_date(modified)).unwrap(),
        );
        if status == StatusCode::PARTIAL_CONTENT {
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                header::HeaderValue::from_str(&format!("bytes {start}-{end}/{}", metadata.len()))
                    .unwrap(),
            );
        }
        response
    }

    fn resolve(&self, uri: &str) -> Option<PathBuf> {
        let prefix = &self.config.config.assets.url_prefix;
        if !uri.starts_with(prefix) {
            return None;
        }
        let relative = percent_decode(&uri[prefix.len()..])?;
        if relative.is_empty() || relative.contains("..") || relative.contains('\\') {
            return None;
        }
        self.files.get(relative.rsplit('/').next()?).cloned()
    }
}

fn requested_range(
    value: Option<&header::HeaderValue>,
    size: u64,
) -> Option<(u64, u64, StatusCode)> {
    let Some(value) = value else {
        return Some((0, size.saturating_sub(1), StatusCode::OK));
    };
    let text = value.to_str().ok()?;
    let pair = text.strip_prefix("bytes=")?.split_once('-')?;
    let start = if pair.0.is_empty() {
        size.checked_sub(pair.1.parse::<u64>().ok()?)?
    } else {
        pair.0.parse().ok()?
    };
    let requested_end = if pair.1.is_empty() {
        size.saturating_sub(1)
    } else {
        pair.1.parse().ok()?
    };
    if size == 0 || start >= size || start > requested_end {
        return None;
    }
    Some((
        start,
        requested_end.min(size - 1),
        StatusCode::PARTIAL_CONTENT,
    ))
}

fn percent_decode(value: &str) -> Option<String> {
    let mut output = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            output.push((hex(bytes[index + 1])? << 4 | hex(bytes[index + 2])?) as char);
            index += 3;
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    Some(output)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn simple_response(
    status: StatusCode,
    body: &'static [u8],
    allow: Option<&'static str>,
) -> Response<AppBody> {
    let length = body.len();
    let mut response = Response::new(full(Bytes::from_static(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    if let Some(allow) = allow {
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static(allow));
    }
    response
}
