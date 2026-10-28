//! Micro-backend HTTP server for DesignCraft Web:
//! - Serves static WebAssembly bundle from `--dist` (`./dist/web`)
//! - Serves `/api/templates`: lists and delivers `.designcraft` native templates
//! - Serves `/api/documents`: lists, delivers and persists working `.designcraft` drafts
//! - CORS headers enabled for multi-origin access

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde_json::json;

const MAX_BODY_SIZE: usize = 300 * 1024 * 1024; // 300 MB document size cap (supports large ads/editions)

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

type DocLocks = Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>;

#[derive(Clone, Debug)]
pub struct DocLease {
    pub holder: String,
    pub holder_name: String,
    pub acquired_at: u64,
    pub last_heartbeat: u64,
    pub mode: String,
}

pub type DocLeases = Arc<Mutex<HashMap<String, DocLease>>>;
pub const LEASE_TIMEOUT_SECS: u64 = 90;

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn get_doc_lock(locks: &DocLocks, id: &str) -> Arc<Mutex<()>> {
    let mut map = locks.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(id.to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
}

fn load_env_var(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key)
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Ok(content) = fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=')
                && k.trim() == key
            {
                let mut val = v.trim();
                if (val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')) {
                    val = val.get(1..val.len().saturating_sub(1)).unwrap_or("");
                }
                return Some(val.to_string());
            }
        }
    }
    None
}

#[derive(Clone)]
struct CachedFont {
    filename: String,
    bytes: Vec<u8>,
    mime: &'static str,
}

type FontCache = Arc<std::collections::HashMap<String, CachedFont>>;

fn font_mime(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "ttf" => Some("font/ttf"),
        "otf" => Some("font/otf"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

fn load_fonts_into_cache(dir: &Path, cache: &mut std::collections::HashMap<String, CachedFont>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if let Some(mime) = font_mime(ext) {
                let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if let Ok(bytes) = fs::read(&p) {
                    cache.insert(filename.clone(), CachedFont { filename, bytes, mime });
                }
            }
        }
    }
}

pub fn serve(args: &[String]) -> Result<(), String> {
    let mut port: u16 = 5173;
    let mut bind_addr = "0.0.0.0".to_string();
    let mut templates_dir = PathBuf::from("templates");
    let mut docs_dir = PathBuf::from("documents");
    let mut fonts_dir = PathBuf::from("fonts");
    let mut dist_dir = PathBuf::from("dist/web");

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                let p = it.next().ok_or("--port needs a port number")?;
                port = p.parse().map_err(|e| format!("invalid port `{p}`: {e}"))?;
            }
            "--bind" | "-b" => {
                bind_addr = it.next().ok_or("--bind needs an address")?.clone();
            }
            "--templates" | "-t" => {
                templates_dir = PathBuf::from(it.next().ok_or("--templates needs a directory")?);
            }
            "--docs" | "-d" => {
                docs_dir = PathBuf::from(it.next().ok_or("--docs needs a directory")?);
            }
            "--fonts" | "-f" => {
                fonts_dir = PathBuf::from(it.next().ok_or("--fonts needs a directory")?);
            }
            "--dist" => {
                dist_dir = PathBuf::from(it.next().ok_or("--dist needs a directory")?);
            }
            "--help" | "-h" => {
                println!(
                    "usage: designcraft-cli serve [--port PORT] [--bind ADDR] [--templates DIR] [--docs DIR] [--fonts DIR] [--dist DIR]\n\
                     \n\
                     Options:\n\
                       --port, -p <PORT>        Port to listen on (default: 8767)\n\
                       --bind, -b <ADDR>        Address to bind (default: 0.0.0.0)\n\
                       --templates, -t <DIR>    Directory containing .designcraft templates (default: ./templates)\n\
                       --docs, -d <DIR>         Directory for saved .designcraft documents (default: ./documents)\n\
                       --fonts, -f <DIR>        Directory containing .ttf/.otf/.woff2 fonts (default: ./fonts)\n\
                       --dist <DIR>             Directory with compiled web assets (default: ./dist/web)"
                );
                return Ok(());
            }
            other => return Err(format!("unknown option `{other}` (run `designcraft-cli serve --help`)")),
        }
    }

    // Ensure storage directories exist
    fs::create_dir_all(&templates_dir).map_err(|e| format!("cannot create `{}`: {e}", templates_dir.display()))?;
    fs::create_dir_all(&docs_dir).map_err(|e| format!("cannot create `{}`: {e}", docs_dir.display()))?;
    fs::create_dir_all(&fonts_dir).map_err(|e| format!("cannot create `{}`: {e}", fonts_dir.display()))?;

    // Load fonts into server memory RAM
    let mut raw_font_cache = std::collections::HashMap::new();
    load_fonts_into_cache(&fonts_dir, &mut raw_font_cache);
    if raw_font_cache.is_empty() && Path::new("assets/fonts").is_dir() {
        load_fonts_into_cache(Path::new("assets/fonts"), &mut raw_font_cache);
    }
    let font_count = raw_font_cache.len();
    let font_bytes: usize = raw_font_cache.values().map(|f| f.bytes.len()).sum();
    let font_cache: FontCache = Arc::new(raw_font_cache);

    let addr = format!("{bind_addr}:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("cannot bind {addr}: {e}"))?;

    println!("============================================================");
    println!("DesignCraft Editorial Server running on http://{addr}");
    println!("  Templates directory: {}", templates_dir.display());
    println!("  Documents directory: {}", docs_dir.display());
    println!("  Fonts directory:     {} ({} font(s) in RAM, {:.1} MB)", fonts_dir.display(), font_count, font_bytes as f64 / (1024.0 * 1024.0));
    println!("  Web distribution:    {}", dist_dir.display());
    println!("============================================================");

    let templates_dir = templates_dir.canonicalize().unwrap_or(templates_dir);
    let docs_dir = docs_dir.canonicalize().unwrap_or(docs_dir);
    let dist_dir = dist_dir.canonicalize().unwrap_or(dist_dir);
    let doc_locks: DocLocks = Arc::new(Mutex::new(HashMap::new()));
    let doc_leases: DocLeases = Arc::new(Mutex::new(HashMap::new()));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let t_dir = templates_dir.clone();
                let d_dir = docs_dir.clone();
                let w_dir = dist_dir.clone();
                let f_cache = font_cache.clone();
                let d_locks = doc_locks.clone();
                let d_leases = doc_leases.clone();
                std::thread::spawn(move || {
                    let _ = handle_connection(stream, &t_dir, &d_dir, &w_dir, &f_cache, &d_locks, &d_leases);
                });
            }
            Err(e) => {
                eprintln!("designcraft server: connection error: {e}");
            }
        }
    }

    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    templates: &Path,
    docs: &Path,
    dist: &Path,
    fonts: &FontCache,
    locks: &DocLocks,
    leases: &DocLeases,
) -> Result<(), String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).map_err(|e| e.to_string())?;

    let trimmed_req = request_line.trim_end_matches(['\r', '\n']);
    let Some((method, rest_req)) = trimmed_req.split_once(' ') else {
        return Ok(());
    };
    let raw_path = if let Some((p, _ver)) = rest_req.rsplit_once(' ') { p.trim() } else { rest_req.trim() };
    let path = raw_path.split('?').next().unwrap_or(raw_path);

    // Read headers to extract Content-Length
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
            && let Ok(len) = v.trim().parse::<usize>()
        {
            content_length = len;
        }
    }

    // CORS preflight
    if method == "OPTIONS" {
        send_response(&mut stream, 204, "No Content", "text/plain", &[])?;
        return Ok(());
    }

    // Route API requests
    if let Some(rest) = path.strip_prefix("/api/") {
        match rest {
            "health" => {
                let body = json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}).to_string();
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            "cloud/config" if method == "GET" || method == "HEAD" => {
                let client_id = load_env_var("VITE_GOOGLE_DRIVE_CLIENT_ID").unwrap_or_default();
                let client_secret = load_env_var("VITE_GOOGLE_DRIVE_CLIENT_SECRET").unwrap_or_default();
                let api_key = load_env_var("VITE_GOOGLE_DRIVE_API_KEY").unwrap_or_default();
                let templates_folder = load_env_var("VITE_DRIVE_TEMPLATES_FOLDER_ID").unwrap_or_default();
                let dest_folder = load_env_var("VITE_DRIVE_DESTINATION_FOLDER_ID").unwrap_or_default();
                let storage_mode = load_env_var("VITE_STORAGE_MODE").unwrap_or_else(|| "google".to_string());
                let nextcloud_url = load_env_var("VITE_NEXTCLOUD_URL").unwrap_or_default();

                let body = json!({
                    "clientId": client_id,
                    "clientSecret": client_secret,
                    "apiKey": api_key,
                    "templatesFolderId": templates_folder,
                    "destinationFolderId": dest_folder,
                    "storageMode": storage_mode,
                    "nextcloudUrl": nextcloud_url,
                })
                .to_string();
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            "cloud/catalog" if method == "GET" || method == "HEAD" => {
                let cat = match get_or_build_cloud_catalog(templates) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[serve] Error cargando catálogo: {e}");
                        json!({"status": "ok", "categories": [], "templates": []})
                    }
                };
                let body = cat.to_string();
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            "cloud/sync" if method == "POST" => {
                let payload = read_json_body(&mut reader, content_length).unwrap_or(json!({}));
                let token = payload["accessToken"].as_str().unwrap_or("");
                let force_refresh = payload["forceRefresh"].as_bool().unwrap_or(false) || payload["clearCache"].as_bool().unwrap_or(false);
                let folder_id =
                    payload["folderId"].as_str().map(String::from).or_else(|| load_env_var("VITE_DRIVE_TEMPLATES_FOLDER_ID")).unwrap_or_default();

                if force_refresh {
                    let _ = fs::remove_file(templates.join("cloud").join("catalog.json"));
                }

                let (mut cat, sync_success, sync_error) = if token.is_empty() {
                    let fallback =
                        get_or_build_cloud_catalog(templates).unwrap_or_else(|_| json!({"status": "ok", "categories": [], "templates": []}));
                    (fallback, false, Some("No access token provided".to_string()))
                } else {
                    match sync_cloud_catalog_with_drive(templates, token, &folder_id) {
                        Ok(c) => (c, true, None),
                        Err(e) => {
                            eprintln!("[serve] Advertencia sync con Drive ({e}).");
                            let fallback =
                                get_or_build_cloud_catalog(templates).unwrap_or_else(|_| json!({"status": "ok", "categories": [], "templates": []}));
                            (fallback, false, Some(e))
                        }
                    }
                };

                if let Some(obj) = cat.as_object_mut() {
                    obj.insert("syncSuccess".to_string(), json!(sync_success));
                    if let Some(err) = sync_error {
                        obj.insert("syncError".to_string(), json!(err));
                    }
                }
                let body = cat.to_string();
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            p if p.starts_with("cloud/thumbnails/") && (method == "GET" || method == "HEAD") => {
                let thumb_id = p.strip_prefix("cloud/thumbnails/").unwrap_or(p);
                let query = raw_path.split_once('?').map(|(_, q)| q).unwrap_or("");
                serve_cloud_thumbnail(&mut stream, templates, docs, thumb_id, query)?;
            }
            p if p.starts_with("cloud/open/") && method == "POST" => {
                let id = p.strip_prefix("cloud/open/").unwrap_or(p);
                let payload = read_json_body(&mut reader, content_length).unwrap_or(json!({}));
                open_cloud_template(&mut stream, templates, docs, locks, id, &payload)?;
            }
            "cloud/upload" if method == "POST" => {
                let payload = match read_json_body(&mut reader, content_length) {
                    Ok(v) => v,
                    Err((code, status, msg)) => {
                        let err = json!({"error": msg}).to_string();
                        return send_response(&mut stream, code, status, "application/json", err.as_bytes());
                    }
                };
                let doc_id = payload["docId"].as_str().unwrap_or("");
                let token = payload["accessToken"].as_str().unwrap_or("");
                let dest_folder_id = payload["destinationFolderId"]
                    .as_str()
                    .map(String::from)
                    .or_else(|| load_env_var("VITE_DRIVE_DESTINATION_FOLDER_ID"))
                    .unwrap_or_default();
                let is_page_folder = payload["isPageFolder"].as_bool().unwrap_or(false);
                let base_name = payload["baseName"].as_str().unwrap_or("");
                let date_str = payload["date"].as_str().unwrap_or("");
                let export_pdf = payload["exportPdf"].as_bool().unwrap_or(true);
                let pdf_standard = payload["pdfStandard"].as_str().unwrap_or("x1a");
                upload_document_to_drive(
                    &mut stream,
                    docs,
                    doc_id,
                    token,
                    &dest_folder_id,
                    is_page_folder,
                    base_name,
                    date_str,
                    export_pdf,
                    pdf_standard,
                )?;
            }
            "templates" if method == "GET" || method == "HEAD" => {
                let list = list_files(templates)?;
                let body = serde_json::to_string(&list).map_err(|e| e.to_string())?;
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            p if p.starts_with("templates/") => {
                let sub = p.strip_prefix("templates/").unwrap_or(p);
                if sub.contains('/') {
                    route_document_request(&mut reader, &mut stream, templates, templates, locks, leases, sub, method, content_length)?;
                } else if method == "GET" || method == "HEAD" {
                    serve_file_or_404(&mut stream, templates, sub)?;
                } else {
                    let err = json!({"error": "templates are read-only"}).to_string();
                    send_response(&mut stream, 405, "Method Not Allowed", "application/json", err.as_bytes())?;
                }
            }
            "documents" if method == "GET" || method == "HEAD" => {
                let list = list_files(docs)?;
                let body = serde_json::to_string(&list).map_err(|e| e.to_string())?;
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            p if p.starts_with("documents/exists/") && (method == "GET" || method == "HEAD") => {
                let id = p.strip_prefix("documents/exists/").unwrap_or(p);
                if let Some(clean_id) = sanitize_id(id) {
                    let file = docs.join(format!("{clean_id}.designcraft"));
                    let exists = file.is_file();
                    let body = json!({"exists": exists, "id": clean_id}).to_string();
                    send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
                } else {
                    let body = json!({"exists": false, "error": "invalid id"}).to_string();
                    send_response(&mut stream, 400, "Bad Request", "application/json", body.as_bytes())?;
                }
            }
            p if p.starts_with("documents/") => {
                let sub = p.strip_prefix("documents/").unwrap_or(p);
                route_document_request(&mut reader, &mut stream, docs, templates, locks, leases, sub, method, content_length)?;
            }
            "fonts" if method == "GET" || method == "HEAD" => {
                let mut list: Vec<serde_json::Value> = fonts
                    .values()
                    .map(|f| {
                        json!({
                            "filename": f.filename,
                            "url": format!("/api/fonts/{}", url_encode(&f.filename)),
                            "size": f.bytes.len(),
                        })
                    })
                    .collect();
                list.sort_by(|a, b| a["filename"].as_str().cmp(&b["filename"].as_str()));
                let body = serde_json::to_string(&list).map_err(|e| e.to_string())?;
                send_response(&mut stream, 200, "OK", "application/json", body.as_bytes())?;
            }
            p if p.starts_with("fonts/") && (method == "GET" || method == "HEAD") => {
                let raw_name = p.strip_prefix("fonts/").unwrap_or(p);
                let name = url_decode(raw_name);
                if let Some(font) = fonts.get(&name) {
                    send_cached_response(&mut stream, 200, "OK", font.mime, &font.bytes)?;
                } else {
                    let err = json!({"error": format!("font `{name}` not found")}).to_string();
                    send_response(&mut stream, 404, "Not Found", "application/json", err.as_bytes())?;
                }
            }
            _ => {
                let body = json!({"error": "endpoint not found"}).to_string();
                send_response(&mut stream, 404, "Not Found", "application/json", body.as_bytes())?;
            }
        }
        return Ok(());
    }

    // Gallery web UI
    if path == "/galeria" || path == "/templates" {
        serve_gallery(&mut stream, templates, docs)?;
        return Ok(());
    }

    // Static assets from dist
    serve_static(&mut stream, dist, path)?;
    Ok(())
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or_default();
            if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                out.push(byte);
                i = i.saturating_add(3);
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i = i.saturating_add(1);
            continue;
        }
        out.push(bytes[i]);
        i = i.saturating_add(1);
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn sanitize_id(id: &str) -> Option<String> {
    let decoded = url_decode(id);
    let trimmed = decoded.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") || trimmed.contains('\0') {
        return None;
    }
    let mut base = trimmed;
    loop {
        let lower = base.to_ascii_lowercase();
        if lower.ends_with(".designcraft") {
            base = &base[..base.len() - ".designcraft".len()];
        } else if lower.ends_with(".idml") {
            base = &base[..base.len() - ".idml".len()];
        } else if lower.ends_with(".png") {
            base = &base[..base.len() - ".png".len()];
        } else {
            break;
        }
    }
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    if base.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return None;
    }
    Some(base.to_string())
}

fn list_files(dir: &Path) -> Result<Vec<serde_json::Value>, String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().is_some_and(|ext| ext == "designcraft") {
            let file_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let id = file_name.strip_suffix(".designcraft").unwrap_or(&file_name).to_string();
            let meta = p.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified =
                meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let title = id.replace(['_', '-'], " ");
            let has_thumb = dir.join(format!("{id}.png")).is_file();
            out.push(json!({
                "id": id,
                "name": title,
                "filename": file_name,
                "size": size,
                "modified": modified,
                "has_thumb": has_thumb,
                "thumb_url": if has_thumb { format!("/api/templates/{id}.png") } else { String::new() },
            }));
        }
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(out)
}

fn serve_file_or_404(stream: &mut TcpStream, dir: &Path, id: &str) -> Result<(), String> {
    let is_png = id.ends_with(".png");
    let clean_id = if is_png { id.strip_suffix(".png").and_then(sanitize_id) } else { sanitize_id(id) };
    let Some(clean_id) = clean_id else {
        let err = json!({"error": "invalid identifier"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };
    let file_path = if is_png {
        dir.join(format!("{clean_id}.png"))
    } else {
        let primary = dir.join(format!("{clean_id}.designcraft"));
        if !primary.is_file() {
            let legacy = dir.join(format!("{clean_id}.idml.designcraft"));
            if legacy.is_file() { legacy } else { primary }
        } else {
            primary
        }
    };
    match fs::read(&file_path) {
        Ok(bytes) => {
            let final_bytes = if !is_png && designcraft_idml::is_idml(&bytes) {
                let link_reader = |link: &str| -> Option<Vec<u8>> {
                    let fname = Path::new(link).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| link.to_string());
                    for candidate in [
                        dir.join(&fname),
                        dir.join("Links").join(&fname),
                        dir.join("links").join(&fname),
                        PathBuf::from("/home/eder/Descargas/designcraft-main/templates/Links").join(&fname),
                        PathBuf::from("/home/eder/Documentos/Plantillas corregidas/ECONOMICA/Links").join(&fname),
                        PathBuf::from("/home/eder/Documentos/Plantillas corregidas/GENERAL/Links").join(&fname),
                        PathBuf::from("/home/eder/Documentos/Plantillas corregidas/DEPORTES/Links").join(&fname),
                    ] {
                        if let Ok(b) = fs::read(&candidate) {
                            return Some(b);
                        }
                    }
                    None
                };
                if let Ok(doc) = designcraft_idml::import_idml_with(&bytes, &link_reader) {
                    if let Ok(converted) = designcraft_format::save(&doc) {
                        let standard_path = dir.join(format!("{clean_id}.designcraft"));
                        let _ = fs::write(&standard_path, &converted);
                        converted
                    } else {
                        bytes
                    }
                } else {
                    bytes
                }
            } else {
                bytes
            };
            let mime = if is_png { "image/png" } else { "application/vnd.designcraft+zip" };
            send_response(stream, 200, "OK", mime, &final_bytes)
        }
        Err(_) => {
            let err = json!({"error": format!("file `{clean_id}` not found")}).to_string();
            send_response(stream, 404, "Not Found", "application/json", err.as_bytes())
        }
    }
}

fn serve_gallery(stream: &mut TcpStream, templates: &Path, docs: &Path) -> Result<(), String> {
    let tmpls = list_files(templates).unwrap_or_default();
    let dcs = list_files(docs).unwrap_or_default();
    let html = render_gallery_html(&tmpls, &dcs);
    send_response(stream, 200, "OK", "text/html; charset=utf-8", html.as_bytes())
}

fn render_gallery_html(templates: &[serde_json::Value], docs: &[serde_json::Value]) -> String {
    let mut cards = String::new();
    for t in templates {
        let id = t["id"].as_str().unwrap_or_default();
        let name = t["name"].as_str().unwrap_or(id);
        let size_kb = t["size"].as_u64().unwrap_or(0) / 1024;
        let has_thumb = t["has_thumb"].as_bool().unwrap_or(false);
        let enc_id = url_encode(id);
        let img_tag = if has_thumb {
            format!(
                "<img src='/api/templates/{enc_id}.png' alt='{name}' class='thumb-img' onerror=\"this.style.display='none';this.nextElementSibling.style.display='flex'\"/><div class='thumb-fallback' style='display:none;'>📄</div>"
            )
        } else {
            "<div class='thumb-fallback'>📄</div>".to_string()
        };

        let safe_name = name.replace('\'', "\\'");
        cards.push_str(&format!(
            "<div class='card template-card' data-name='{name}' data-id='{id}' onclick=\"openCloneModal('{enc_id}', '{safe_name}', 'layout')\" style='cursor:pointer;'>\
               <div class='thumb-box'>{img_tag}</div>\
               <div class='card-body'>\
                 <div class='card-badge'>Plantilla maquetada</div>\
                 <h3 class='card-title'>{name}</h3>\
                 <p class='card-meta'>{size_kb} KB · Maqueta completa .designcraft</p>\
                 <div class='card-actions'>\
                   <button class='btn btn-primary' onclick=\"event.stopPropagation(); openCloneModal('{enc_id}', '{safe_name}', 'layout')\">📖 Usar plantilla</button>\
                   <button class='btn btn-ghost' onclick=\"event.stopPropagation(); openCloneModal('{enc_id}', '{safe_name}', 'editor')\" title='Crear copia y abrir en modo redactor'>✍ Modo redactor</button>\
                 </div>\
               </div>\
             </div>"
        ));
    }

    let mut doc_cards = String::new();
    for d in docs {
        let id = d["id"].as_str().unwrap_or_default();
        let name = d["name"].as_str().unwrap_or(id);
        let size_kb = d["size"].as_u64().unwrap_or(0) / 1024;
        let enc_id = url_encode(id);
        doc_cards.push_str(&format!(
            "<div class='card doc-card'>\
               <div class='card-body'>\
                 <div class='card-badge badge-draft'>Publicación guardada</div>\
                 <h3 class='card-title'>{name}</h3>\
                 <p class='card-meta'>{size_kb} KB · Persistido en servidor</p>\
                 <div class='card-actions'>\
                   <a href='/?doc={enc_id}' class='btn btn-primary' title='Abrir maqueta completa'>📖 Maquetador</a>\
                   <a href='/?mode=editor&doc={enc_id}' class='btn btn-ghost' title='Abrir en modo redactor'>✍ Redactor</a>\
                   <a href='/api/documents/{enc_id}/idml' class='btn btn-ghost' title='Descargar paquete IDML para Adobe InDesign'>📄 IDML InDesign</a>\
                   <a href='/api/documents/{enc_id}/pdf' class='btn btn-ghost' target='_blank' title='Descargar PDF/X-4 listo para prensa'>🖨 PDF Prensa</a>\
                 </div>\
               </div>\
             </div>"
        ));
    }

    let docs_section = if doc_cards.is_empty() {
        String::new()
    } else {
        format!(
            "<section class='section'>\
               <div class='section-header'>\
                 <h2>📑 Tus documentos guardados ({})</h2>\
                 <p>Publicaciones y maquetas persistidas en el servidor listas para continuar.</p>\
               </div>\
               <div class='grid'>{}</div>\
             </section>",
            docs.len(),
            doc_cards
        )
    };

    format!(
        "<!DOCTYPE html>\
<html lang='es'>\
<head>\
  <meta charset='utf-8'>\
  <meta name='viewport' content='width=device-width, initial-scale=1.0'>\
  <title>Galería de Plantillas — DesignCraft Editorial</title>\
  <style>\
    :root {{\
      --bg: #121214;\
      --surface: #1e1e24;\
      --surface-hover: #26262e;\
      --border: #33333d;\
      --accent: #2563eb;\
      --accent-hover: #1d4ed8;\
      --green: #10b981;\
      --text: #f4f4f5;\
      --text-muted: #a1a1aa;\
    }}\
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}\
    body {{ font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; background: var(--bg); color: var(--text); min-height: 100vh; padding: 32px 20px; line-height: 1.5; }}\
    .container {{ max-width: 1200px; margin: 0 auto; }}\
    header {{ display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 16px; margin-bottom: 36px; padding-bottom: 24px; border-bottom: 1px solid var(--border); }}\
    .brand {{ display: flex; align-items: center; gap: 12px; }}\
    .brand-logo {{ font-size: 28px; background: linear-gradient(135deg, #3b82f6, #8b5cf6); -webkit-background-clip: text; -webkit-text-fill-color: transparent; font-weight: 800; letter-spacing: -0.5px; }}\
    .brand-sub {{ color: var(--text-muted); font-size: 14px; }}\
    .header-actions {{ display: flex; gap: 10px; }}\
    .search-bar {{ width: 100%; max-width: 480px; margin-bottom: 28px; position: relative; }}\
    .search-input {{ width: 100%; padding: 12px 16px; border-radius: 8px; border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 15px; outline: none; transition: border-color 0.2s; }}\
    .search-input:focus {{ border-color: var(--accent); }}\
    .section {{ margin-bottom: 48px; }}\
    .section-header {{ margin-bottom: 20px; }}\
    .section-header h2 {{ font-size: 22px; font-weight: 700; margin-bottom: 4px; }}\
    .section-header p {{ color: var(--text-muted); font-size: 14px; }}\
    .grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 24px; }}\
    .card {{ background: var(--surface); border: 1px solid var(--border); border-radius: 12px; overflow: hidden; display: flex; flex-direction: column; transition: transform 0.2s, border-color 0.2s, box-shadow 0.2s; }}\
    .card:hover {{ transform: translateY(-4px); border-color: #4b5563; box-shadow: 0 12px 24px rgba(0,0,0,0.3); }}\
    .thumb-box {{ aspect-ratio: 16/10; background: #16161a; display: flex; align-items: center; justify-content: center; overflow: hidden; position: relative; border-bottom: 1px solid var(--border); }}\
    .thumb-img {{ width: 100%; height: 100%; object-fit: cover; object-position: top; display: block; }}\
    .thumb-fallback {{ font-size: 48px; opacity: 0.4; }}\
    .card-body {{ padding: 20px; display: flex; flex-direction: column; flex-grow: 1; }}\
    .card-badge {{ align-self: flex-start; padding: 3px 8px; border-radius: 4px; font-size: 11px; font-weight: 600; text-transform: uppercase; background: rgba(59, 130, 246, 0.15); color: #60a5fa; margin-bottom: 10px; }}\
    .badge-draft {{ background: rgba(16, 185, 129, 0.15); color: #34d399; }}\
    .card-title {{ font-size: 18px; font-weight: 700; text-transform: capitalize; margin-bottom: 6px; }}\
    .card-meta {{ font-size: 13px; color: var(--text-muted); margin-bottom: 16px; }}\
    .card-actions {{ display: flex; gap: 8px; margin-top: auto; }}\
    .btn {{ display: inline-flex; align-items: center; justify-content: center; padding: 10px 16px; border-radius: 8px; font-size: 14px; font-weight: 600; text-decoration: none; cursor: pointer; border: none; transition: background 0.2s; }}\
    .btn-primary {{ background: var(--accent); color: white; flex-grow: 1; }}\
    .btn-primary:hover {{ background: var(--accent-hover); }}\
    .btn-ghost {{ background: transparent; border: 1px solid var(--border); color: var(--text-muted); }}\
    .btn-ghost:hover {{ background: var(--surface-hover); color: var(--text); }}\
    .modal-backdrop {{ position: fixed; inset: 0; background: rgba(0,0,0,0.75); backdrop-filter: blur(4px); display: none; align-items: center; justify-content: center; z-index: 1000; padding: 20px; }}\
    .modal-window {{ background: var(--surface); border: 1px solid var(--border); border-radius: 14px; max-width: 520px; width: 100%; box-shadow: 0 20px 40px rgba(0,0,0,0.6); overflow: hidden; }}\
    .modal-header {{ display: flex; justify-content: space-between; align-items: center; padding: 18px 24px; border-bottom: 1px solid var(--border); }}\
    .modal-header h3 {{ font-size: 18px; font-weight: 700; color: var(--text); }}\
    .modal-close {{ background: none; border: none; font-size: 26px; color: var(--text-muted); cursor: pointer; line-height: 1; padding: 0 4px; }}\
    .modal-close:hover {{ color: var(--text); }}\
    .modal-body {{ padding: 24px; display: flex; flex-direction: column; gap: 18px; }}\
    .modal-desc {{ font-size: 13.5px; color: var(--text-muted); line-height: 1.4; }}\
    .modal-desc strong {{ color: #60a5fa; }}\
    .form-group {{ display: flex; flex-direction: column; gap: 6px; }}\
    .form-label {{ font-size: 13px; font-weight: 600; color: var(--text); }}\
    .modal-input {{ width: 100%; padding: 10px 14px; border-radius: 8px; border: 1px solid var(--border); background: #16161a; color: var(--text); font-size: 14px; outline: none; transition: border-color 0.2s; }}\
    .modal-input:focus {{ border-color: var(--accent); }}\
    .radio-group {{ display: flex; flex-direction: column; gap: 8px; }}\
    .radio-card {{ display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-radius: 8px; border: 1px solid var(--border); background: #18181d; cursor: pointer; transition: border-color 0.2s, background 0.2s; }}\
    .radio-card:hover {{ border-color: #4b5563; background: #202026; }}\
    .radio-card input {{ margin-top: 3px; accent-color: var(--accent); }}\
    .radio-card strong {{ display: block; font-size: 14px; color: var(--text); margin-bottom: 2px; }}\
    .radio-card span {{ font-size: 12px; color: var(--text-muted); display: block; line-height: 1.3; }}\
    .modal-footer {{ display: flex; justify-content: flex-end; gap: 10px; padding: 16px 24px; border-top: 1px solid var(--border); background: rgba(0,0,0,0.15); }}\
  </style>\
</head>\
<body>\
  <div class='container'>\
    <header>\
      <div class='brand'>\
        <div>\
          <div class='brand-logo'>DesignCraft Editorial</div>\
          <div class='brand-sub'>Plataforma de redacción y maquetación conectada</div>\
        </div>\
      </div>\
      <div class='header-actions'>\
        <a href='/?mode=editor' class='btn btn-ghost'>✍ Redactor en blanco</a>\
        <a href='/' class='btn btn-ghost'>🖥 Maquetador completo</a>\
      </div>\
    </header>\
\
    <div class='search-bar'>\
      <input type='text' id='search' class='search-input' placeholder='🔍 Buscar plantilla por nombre...' oninput='filterTemplates()' />\
    </div>\
\
    <section class='section'>\
      <div class='section-header'>\
        <h2>📚 Plantillas Maquetadas Disponibles ({})</h2>\
        <p>Selecciona una plantilla para abrir la maqueta completa con todas sus páginas, pliegos, tipografía y estilos en español.</p>\
      </div>\
      <div class='grid' id='template-grid'>\
        {}\
      </div>\
    </section>\
\
    {}\
  </div>\
\
  <div id='clone-modal' class='modal-backdrop' onclick='if(event.target===this)closeModal()'>\
    <div class='modal-window'>\
      <div class='modal-header'>\
        <h3>📄 Crear copia de trabajo desde plantilla</h3>\
        <button class='modal-close' onclick='closeModal()'>&times;</button>\
      </div>\
      <div class='modal-body'>\
        <p class='modal-desc'>Trabajarás sobre una <strong>copia independiente</strong>. La plantilla original permanecerá intacta en el servidor.</p>\
        <div class='form-group'>\
          <label class='form-label' for='clone-name'>Nombre de tu nota o publicación:</label>\
          <input type='text' id='clone-name' class='modal-input' placeholder='ej. 2026-10-15-noticia-salud' onkeydown='if(event.key===\"Enter\")submitClone()' />\
        </div>\
        <div class='form-group'>\
          <label class='form-label'>Modo de trabajo inicial:</label>\
          <div class='radio-group'>\
            <label class='radio-card'>\
              <input type='radio' name='clone-mode' value='editor' id='mode-editor' checked />\
              <div>\
                <strong>✍ Modo Redactor</strong>\
                <span>Enfocado en artículos, texto y semáforo de copyfit sin alterar el diseño maquetado.</span>\
              </div>\
            </label>\
            <label class='radio-card'>\
              <input type='radio' name='clone-mode' value='layout' id='mode-layout' />\
              <div>\
                <strong>📖 Modo Maquetador</strong>\
                <span>Vista de diseño completa para maquetar páginas, pliegos, tipografía y cajas.</span>\
              </div>\
            </label>\
          </div>\
        </div>\
      </div>\
      <div class='modal-footer'>\
        <button class='btn btn-ghost' onclick='closeModal()'>Cancelar</button>\
        <button class='btn btn-primary' onclick='submitClone()'>🚀 Comenzar a trabajar</button>\
      </div>\
    </div>\
  </div>\
\
  <script>\
    var currentTemplateId = '';\
    var currentTemplateName = '';\
\
    function openCloneModal(templateId, templateName, mode) {{\
      currentTemplateId = templateId;\
      currentTemplateName = templateName;\
      var d = new Date();\
      var month = String(d.getMonth() + 1).padStart(2, '0');\
      var day = String(d.getDate()).padStart(2, '0');\
      var dateStr = d.getFullYear() + '-' + month + '-' + day;\
      var cleanSlug = templateId.replace(/\\.designcraft$/i, '').replace(/[^a-zA-Z0-9_-]/g, '_');\
      document.getElementById('clone-name').value = dateStr + '-' + cleanSlug;\
      if (mode === 'layout') {{\
        document.getElementById('mode-layout').checked = true;\
      }} else {{\
        document.getElementById('mode-editor').checked = true;\
      }}\
      document.getElementById('clone-modal').style.display = 'flex';\
      setTimeout(function() {{\
        var inp = document.getElementById('clone-name');\
        inp.focus();\
        inp.select();\
      }}, 50);\
    }}\
\
    function closeModal() {{\
      document.getElementById('clone-modal').style.display = 'none';\
    }}\
\
    function submitClone() {{\
      var val = document.getElementById('clone-name').value.trim();\
      if (!val) {{\
        alert('Por favor introduce un nombre para tu copia de trabajo.');\
        return;\
      }}\
      var cleanDoc = val.replace(/\\.designcraft$/i, '');\
      var isEditor = document.getElementById('mode-editor').checked;\
      var url = '/?doc=' + encodeURIComponent(cleanDoc) + '&template=' + encodeURIComponent(currentTemplateId) + (isEditor ? '&mode=editor' : '');\
      window.location.href = url;\
    }}\
\
    function filterTemplates() {{\
      var q = document.getElementById('search').value.toLowerCase();\
      var cards = document.querySelectorAll('.template-card');\
      cards.forEach(function(c) {{\
        var name = c.getAttribute('data-name').toLowerCase();\
        var id = c.getAttribute('data-id').toLowerCase();\
        if (name.includes(q) || id.includes(q)) {{\
          c.style.display = 'flex';\
        }} else {{\
          c.style.display = 'none';\
        }}\
      }});\
    }}\
  </script>\
</body>\
</html>",
        templates.len(),
        cards,
        docs_section
    )
}

fn route_document_request(
    reader: &mut impl Read,
    stream: &mut TcpStream,
    docs: &Path,
    templates: &Path,
    locks: &DocLocks,
    leases: &DocLeases,
    sub: &str,
    method: &str,
    content_length: usize,
) -> Result<(), String> {
    let mut parts: Vec<&str> = sub.split('/').collect();
    if parts.len() >= 2 && (parts[0] == "templates" || parts[0] == "documents") {
        parts.remove(0);
    }
    let raw_id = parts.first().copied().unwrap_or("");
    let Some(clean_id) = sanitize_id(raw_id) else {
        let err = json!({"error": "invalid identifier"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };

    match parts.as_slice() {
        [_] if method == "GET" || method == "HEAD" => {
            serve_file_or_404(stream, docs, &clean_id)?;
        }
        [_] if method == "POST" || method == "PUT" => {
            save_document(reader, stream, docs, locks, leases, &clean_id, content_length)?;
        }
        [_, "lease"] if method == "GET" || method == "HEAD" => {
            handle_document_lease_get(stream, leases, &clean_id)?;
        }
        [_, "lease"] if method == "POST" => {
            handle_document_lease_acquire(reader, stream, leases, &clean_id, content_length)?;
        }
        [_, "heartbeat"] if method == "POST" => {
            handle_document_lease_heartbeat(reader, stream, leases, &clean_id, content_length)?;
        }
        [_, "release"] if method == "POST" || method == "DELETE" => {
            handle_document_lease_release(reader, stream, leases, &clean_id, content_length)?;
        }
        [_, "stories"] if method == "GET" || method == "HEAD" => {
            handle_stories_list(stream, docs, &clean_id)?;
        }
        [_, "stories", raw_sid] if method == "GET" || method == "HEAD" => {
            let Ok(sid) = raw_sid.parse::<u64>() else {
                let err = json!({"error": format!("invalid story id `{raw_sid}`")}).to_string();
                return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
            };
            handle_story_get(stream, docs, &clean_id, sid)?;
        }
        [_, "stories", raw_sid] if method == "POST" || method == "PUT" => {
            let Ok(sid) = raw_sid.parse::<u64>() else {
                let err = json!({"error": format!("invalid story id `{raw_sid}`")}).to_string();
                return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
            };
            handle_story_update(reader, stream, docs, locks, &clean_id, sid, content_length)?;
        }
        [_, "articles"] if method == "GET" || method == "HEAD" => {
            handle_articles_list(stream, docs, &clean_id)?;
        }
        [_, "articles", rest @ ..] if (method == "GET" || method == "HEAD") && !rest.is_empty() => {
            let art_name = url_decode(&rest.join("/"));
            handle_article_get(stream, docs, &clean_id, &art_name)?;
        }
        [_, "articles", rest @ ..] if (method == "POST" || method == "PUT") && !rest.is_empty() => {
            let art_name = url_decode(&rest.join("/"));
            handle_article_update(reader, stream, docs, locks, &clean_id, &art_name, content_length)?;
        }
        [_, "inject"] if method == "POST" || method == "PUT" => {
            handle_inject_fields(reader, stream, docs, templates, locks, &clean_id, content_length)?;
        }
        [_, "copyfit"] if method == "GET" || method == "HEAD" => {
            handle_copyfit_status(stream, docs, &clean_id)?;
        }
        [_, "pdf"] if method == "GET" || method == "HEAD" => {
            handle_document_pdf(stream, docs, &clean_id, sub)?;
        }
        [_, "idml"] if method == "GET" || method == "HEAD" => {
            handle_document_idml(stream, docs, &clean_id)?;
        }
        _ => {
            let err = json!({"error": format!("unknown endpoint `/api/documents/{sub}`")}).to_string();
            send_response(stream, 404, "Not Found", "application/json", err.as_bytes())?;
        }
    }
    Ok(())
}

fn read_json_body(reader: &mut impl Read, max_len: usize) -> Result<serde_json::Value, (u16, &'static str, String)> {
    if max_len == 0 || max_len > MAX_BODY_SIZE {
        return Err((400, "Bad Request", "invalid or excessive content length".to_string()));
    }
    let mut buf = vec![0u8; max_len];
    reader.read_exact(&mut buf).map_err(|e| (400, "Bad Request", format!("failed reading request body: {e}")))?;
    serde_json::from_slice(&buf).map_err(|e| (400, "Bad Request", format!("invalid json payload: {e}")))
}

fn load_doc_session(dir: &Path, clean_id: &str) -> Result<(designcraft_engine::Session, PathBuf), (u16, &'static str, String)> {
    let file_path = dir.join(format!("{clean_id}.designcraft"));
    if !file_path.is_file() {
        return Err((404, "Not Found", format!("document `{clean_id}` not found")));
    }
    let bytes = fs::read(&file_path).map_err(|e| (500, "Internal Server Error", format!("failed reading document `{clean_id}`: {e}")))?;
    let doc = designcraft_format::load(&bytes).map_err(|e| (500, "Internal Server Error", format!("failed decoding document `{clean_id}`: {e}")))?;
    let mut session = designcraft_engine::Session::new();
    session.add_document(designcraft_engine::DocState::new(doc, Some(file_path.to_string_lossy().to_string())));
    Ok((session, file_path))
}

fn atomic_save_doc(session: &designcraft_engine::Session, dir: &Path, clean_id: &str) -> Result<usize, (u16, &'static str, String)> {
    let state = session.doc().map_err(|e| (500, "Internal Server Error", format!("engine session error: {e}")))?;
    let bytes = designcraft_format::save(&state.doc).map_err(|e| (500, "Internal Server Error", format!("serialization error: {e}")))?;
    let len = bytes.len();
    let target = dir.join(format!("{clean_id}.designcraft"));
    let cnt = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("{clean_id}.tmp-{}-{cnt}", std::process::id()));
    fs::write(&tmp, &bytes).map_err(|e| (500, "Internal Server Error", format!("cannot write tmp file: {e}")))?;
    fs::rename(&tmp, &target).map_err(|e| (500, "Internal Server Error", format!("cannot rename to target: {e}")))?;
    Ok(len)
}

fn handle_stories_list(stream: &mut TcpStream, dir: &Path, clean_id: &str) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    match session.execute("story.list", &json!({})) {
        Ok(val) => {
            let body = serde_json::to_string(&val).map_err(|e| e.to_string())?;
            send_response(stream, 200, "OK", "application/json", body.as_bytes())
        }
        Err(e) => {
            let err = json!({"error": format!("failed listing stories: {e}")}).to_string();
            send_response(stream, 500, "Internal Server Error", "application/json", err.as_bytes())
        }
    }
}

fn handle_story_get(stream: &mut TcpStream, dir: &Path, clean_id: &str, sid: u64) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    match session.execute("story.get", &json!({"story": sid})) {
        Ok(val) => {
            let body = serde_json::to_string(&val).map_err(|e| e.to_string())?;
            send_response(stream, 200, "OK", "application/json", body.as_bytes())
        }
        Err(e) => {
            let err = json!({"error": format!("story `{sid}` not found: {e}")}).to_string();
            send_response(stream, 404, "Not Found", "application/json", err.as_bytes())
        }
    }
}

fn handle_story_update(
    reader: &mut impl Read,
    stream: &mut TcpStream,
    dir: &Path,
    locks: &DocLocks,
    clean_id: &str,
    sid: u64,
    content_length: usize,
) -> Result<(), String> {
    let body_json = match read_json_body(reader, content_length) {
        Ok(v) => v,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };

    let doc_lock = get_doc_lock(locks, clean_id);
    let _guard = doc_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };

    let text_opt = body_json.get("text").and_then(serde_json::Value::as_str);
    let style_opt = body_json.get("paragraphStyle").and_then(serde_json::Value::as_str);

    let mut cmd_param = json!({"story": sid});
    if let Some(t) = text_opt {
        cmd_param["text"] = json!(t);
    }
    if let Some(s) = style_opt {
        cmd_param["paragraphStyle"] = json!(s);
    }

    let res = match session.execute("story.update", &cmd_param) {
        Ok(v) => v,
        Err(e) => {
            let err = json!({"error": format!("failed updating story: {e}")}).to_string();
            return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
        }
    };

    if let Err((code, status, msg)) = atomic_save_doc(&session, dir, clean_id) {
        let err = json!({"error": msg}).to_string();
        return send_response(stream, code, status, "application/json", err.as_bytes());
    }

    let resp_str = serde_json::to_string(&res).map_err(|e| e.to_string())?;
    send_response(stream, 200, "OK", "application/json", resp_str.as_bytes())
}

fn handle_articles_list(stream: &mut TcpStream, dir: &Path, clean_id: &str) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    match session.execute("article.get", &json!({})) {
        Ok(val) => {
            let body = serde_json::to_string(&val).map_err(|e| e.to_string())?;
            send_response(stream, 200, "OK", "application/json", body.as_bytes())
        }
        Err(e) => {
            let err = json!({"error": format!("failed listing articles: {e}")}).to_string();
            send_response(stream, 500, "Internal Server Error", "application/json", err.as_bytes())
        }
    }
}

fn handle_article_get(stream: &mut TcpStream, dir: &Path, clean_id: &str, article_name: &str) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    match session.execute("article.get", &json!({"name": article_name})) {
        Ok(val) => {
            let body = serde_json::to_string(&val).map_err(|e| e.to_string())?;
            send_response(stream, 200, "OK", "application/json", body.as_bytes())
        }
        Err(e) => {
            let err = json!({"error": format!("article `{article_name}` not found: {e}")}).to_string();
            send_response(stream, 404, "Not Found", "application/json", err.as_bytes())
        }
    }
}

fn handle_article_update(
    reader: &mut impl Read,
    stream: &mut TcpStream,
    dir: &Path,
    locks: &DocLocks,
    clean_id: &str,
    article_name: &str,
    content_length: usize,
) -> Result<(), String> {
    let mut body_json = match read_json_body(reader, content_length) {
        Ok(v) => v,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };

    let doc_lock = get_doc_lock(locks, clean_id);
    let _guard = doc_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };

    body_json["name"] = json!(article_name);
    let res = match session.execute("article.update", &body_json) {
        Ok(v) => v,
        Err(e) => {
            let err = json!({"error": format!("failed updating article: {e}")}).to_string();
            return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
        }
    };

    if let Err((code, status, msg)) = atomic_save_doc(&session, dir, clean_id) {
        let err = json!({"error": msg}).to_string();
        return send_response(stream, code, status, "application/json", err.as_bytes());
    }

    let resp_str = serde_json::to_string(&res).map_err(|e| e.to_string())?;
    send_response(stream, 200, "OK", "application/json", resp_str.as_bytes())
}

fn handle_inject_fields(
    reader: &mut impl Read,
    stream: &mut TcpStream,
    docs_dir: &Path,
    templates_dir: &Path,
    locks: &DocLocks,
    clean_id: &str,
    content_length: usize,
) -> Result<(), String> {
    let body_json = match read_json_body(reader, content_length) {
        Ok(v) => v,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };

    let fields = if let Some(f) = body_json.get("fields") { f.clone() } else { body_json.clone() };

    let doc_lock = get_doc_lock(locks, clean_id);
    let _guard = doc_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    // Find source document or template
    let doc_path = docs_dir.join(format!("{clean_id}.designcraft"));
    let tmpl_path = body_json.get("template").and_then(serde_json::Value::as_str).map(|t| templates_dir.join(format!("{t}.designcraft")));
    let direct_tmpl_path = templates_dir.join(format!("{clean_id}.designcraft"));

    let source_path = if doc_path.is_file() {
        doc_path
    } else if let Some(ref tp) = tmpl_path
        && tp.is_file()
    {
        tp.clone()
    } else if direct_tmpl_path.is_file() {
        direct_tmpl_path
    } else {
        let err = json!({"error": format!("document or template for `{clean_id}` not found")}).to_string();
        return send_response(stream, 404, "Not Found", "application/json", err.as_bytes());
    };

    let bytes = fs::read(&source_path).map_err(|e| format!("cannot read source document: {e}"))?;
    let doc = designcraft_format::load(&bytes).map_err(|e| format!("cannot parse document: {e}"))?;
    let mut session = designcraft_engine::Session::new();
    session
        .add_document(designcraft_engine::DocState::new(doc, Some(docs_dir.join(format!("{clean_id}.designcraft")).to_string_lossy().to_string())));

    let res = match session.execute("story.injectFields", &json!({"fields": fields})) {
        Ok(v) => v,
        Err(e) => {
            let err = json!({"error": format!("failed injecting fields: {e}")}).to_string();
            return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
        }
    };

    if let Err((code, status, msg)) = atomic_save_doc(&session, docs_dir, clean_id) {
        let err = json!({"error": msg}).to_string();
        return send_response(stream, code, status, "application/json", err.as_bytes());
    }

    let resp_str = serde_json::to_string(&res).map_err(|e| e.to_string())?;
    send_response(stream, 200, "OK", "application/json", resp_str.as_bytes())
}

fn handle_copyfit_status(stream: &mut TcpStream, dir: &Path, clean_id: &str) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    match session.execute("document.copyfit", &json!({})) {
        Ok(val) => {
            let body = serde_json::to_string(&val).map_err(|e| e.to_string())?;
            send_response(stream, 200, "OK", "application/json", body.as_bytes())
        }
        Err(e) => {
            let err = json!({"error": format!("failed computing copyfit: {e}")}).to_string();
            send_response(stream, 500, "Internal Server Error", "application/json", err.as_bytes())
        }
    }
}

fn handle_document_pdf(stream: &mut TcpStream, dir: &Path, clean_id: &str, query: &str) -> Result<(), String> {
    let (mut session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    let is_reader = query.contains("preset=reader");
    let params = if is_reader {
        json!({
            "bleed": false,
            "marks": false,
            "standard": "none",
            "tagged": true,
            "compressImages": true,
            "flatten": "low"
        })
    } else {
        let standard = if query.contains("standard=x4") { "x4" } else { "x1a" };
        json!({
            "bleed": true,
            "marks": true,
            "standard": standard,
            "tagged": true,
            "compressImages": true,
            "flatten": "high"
        })
    };
    match session.execute("file.exportPdf", &params) {
        Ok(val) => {
            let b64 = val["base64"].as_str().unwrap_or_default();
            let bytes = designcraft_engine::cmd::base64_decode(b64);
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/pdf\r\n\
                 Content-Disposition: attachment; filename=\"{clean_id}.pdf\"\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Connection: close\r\n\r\n",
                bytes.len()
            );
            stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(&bytes).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())
        }
        Err(e) => {
            let err = json!({"error": format!("failed generating PDF: {e}")}).to_string();
            send_response(stream, 500, "Internal Server Error", "application/json", err.as_bytes())
        }
    }
}

fn handle_document_idml(stream: &mut TcpStream, dir: &Path, clean_id: &str) -> Result<(), String> {
    let (session, _) = match load_doc_session(dir, clean_id) {
        Ok(s) => s,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    let st = session.doc().map_err(|e| e.to_string())?;
    let mut idml_doc = (*st.doc).clone();
    let local_links = dir.join("Links");
    for (aid, a) in &mut idml_doc.assets {
        let raw_name = a.link.as_deref().unwrap_or(&a.name);
        let fname = Path::new(raw_name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("asset-{}.jpg", aid.0));
        let mut na = (**a).clone();
        na.link = Some(format!("Links/{fname}"));
        if na.data.is_empty() {
            let cand = [
                local_links.join(&fname),
                dir.parent().unwrap_or(dir).join("templates").join("Links").join(&fname),
                PathBuf::from("/home/eder/Descargas/designcraft-main/templates/Links").join(&fname),
                PathBuf::from("/home/eder/Descargas/designcraft-main/documents/Links").join(&fname),
                PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/ECONOMICA/Links/{fname}")),
                PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/GENERAL/Links/{fname}")),
                PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/DEPORTES/Links/{fname}")),
            ];
            if let Some(b) = cand.into_iter().find(|p| p.is_file()).and_then(|p| fs::read(p).ok()) {
                na.data = Arc::new(b);
            }
        }
        *a = Arc::new(na);
    }
    let idml_bytes = designcraft_idml::export_idml(&idml_doc);
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: application/vnd.adobe.indesign-idml-package\r\n\
         Content-Disposition: attachment; filename=\"{clean_id}.idml\"\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n",
        idml_bytes.len()
    );
    stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(&idml_bytes).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn save_document(
    reader: &mut impl Read,
    stream: &mut TcpStream,
    dir: &Path,
    locks: &DocLocks,
    leases: &DocLeases,
    id: &str,
    len: usize,
) -> Result<(), String> {
    let Some(clean_id) = sanitize_id(id) else {
        let err = json!({"error": "invalid document identifier"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };
    if len == 0 || len > MAX_BODY_SIZE {
        let err = json!({"error": "invalid or excessive content length"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    }

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).map_err(|e| format!("failed reading body: {e}"))?;

    let doc_lock = get_doc_lock(locks, &clean_id);
    let _guard = doc_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    let target_path = dir.join(format!("{clean_id}.designcraft"));
    let cnt = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!("{clean_id}.tmp-{}-{cnt}", std::process::id()));

    // Automatic recovery snapshot of previous version before atomic overwrite
    if target_path.is_file() {
        let backups_dir = dir.join(".backups");
        if fs::create_dir_all(&backups_dir).is_ok() {
            let now = current_unix_timestamp();
            let backup_path = backups_dir.join(format!("{clean_id}_{now}.designcraft"));
            let _ = fs::copy(&target_path, backup_path);
        }
    }

    // Check if incoming payload is IDML format and convert to native .designcraft
    let is_idml = designcraft_idml::is_idml(&body);
    let final_bytes = if is_idml {
        match designcraft_idml::import_idml(&body) {
            Ok(imported_doc) => {
                println!("[serve] Auto-converted IDML `{clean_id}` to native .designcraft");
                match designcraft_format::save(&imported_doc) {
                    Ok(saved) => saved,
                    Err(e) => {
                        eprintln!("[serve] Warning: cannot serialize converted IDML `{clean_id}`: {e}; saving raw body");
                        body
                    }
                }
            }
            Err(e) => {
                eprintln!("[serve] Warning: invalid IDML `{clean_id}`: {e}; saving raw body");
                body
            }
        }
    } else {
        body
    };

    // Atomic write via temporary file
    fs::write(&tmp_path, &final_bytes).map_err(|e| format!("cannot write tmp file: {e}"))?;
    fs::rename(&tmp_path, &target_path).map_err(|e| format!("cannot rename to target: {e}"))?;

    // Update lease heartbeat on save
    {
        let mut map = leases.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get_mut(&clean_id) {
            existing.last_heartbeat = current_unix_timestamp();
        }
    }

    // Persist any non-empty assets into dir/Links/ and compute copyfit
    let mut copyfit_val = serde_json::Value::Null;
    if let Ok(doc) = designcraft_format::load(&final_bytes) {
        let links_dir = dir.join("Links");
        if fs::create_dir_all(&links_dir).is_ok() {
            for (aid, a) in &doc.assets {
                if !a.data.is_empty() {
                    let raw_name = a.link.as_deref().unwrap_or(&a.name);
                    let fname =
                        Path::new(raw_name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("asset-{}.jpg", aid.0));
                    let asset_file = links_dir.join(&fname);
                    if !asset_file.is_file() {
                        let _ = fs::write(&asset_file, &*a.data);
                    }
                }
            }
        }
        let mut session = designcraft_engine::Session::new();
        session.add_document(designcraft_engine::DocState::new(doc, Some(target_path.to_string_lossy().to_string())));
        if let Ok(cf) = session.execute("document.copyfit", &json!({})) {
            copyfit_val = cf;
        }
    }

    let res = json!({
        "ok": true,
        "id": clean_id,
        "filename": format!("{clean_id}.designcraft"),
        "bytes": final_bytes.len(),
        "copyfit": copyfit_val
    })
    .to_string();

    send_response(stream, 200, "OK", "application/json", res.as_bytes())
}

fn handle_document_lease_get(stream: &mut TcpStream, leases: &DocLeases, id: &str) -> Result<(), String> {
    let now = current_unix_timestamp();
    let map = leases.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(lease) = map.get(id) {
        let elapsed = now.saturating_sub(lease.last_heartbeat);
        if elapsed <= LEASE_TIMEOUT_SECS {
            let body = json!({
                "isLocked": true,
                "holder": lease.holder,
                "holderName": lease.holder_name,
                "acquiredAt": lease.acquired_at,
                "secondsAgo": elapsed,
                "timeoutRemaining": LEASE_TIMEOUT_SECS.saturating_sub(elapsed),
                "mode": lease.mode,
            })
            .to_string();
            return send_response(stream, 200, "OK", "application/json", body.as_bytes());
        }
    }
    let body = json!({
        "isLocked": false,
    })
    .to_string();
    send_response(stream, 200, "OK", "application/json", body.as_bytes())
}

fn handle_document_lease_acquire(reader: &mut impl Read, stream: &mut TcpStream, leases: &DocLeases, id: &str, len: usize) -> Result<(), String> {
    let payload = match read_json_body(reader, len) {
        Ok(v) => v,
        Err((code, status, msg)) => {
            let err = json!({"error": msg}).to_string();
            return send_response(stream, code, status, "application/json", err.as_bytes());
        }
    };
    let holder = payload["holder"].as_str().unwrap_or("").trim();
    if holder.is_empty() {
        let err = json!({"error": "`holder` is required"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    }
    let holder_name = payload["holderName"].as_str().or_else(|| payload["name"].as_str()).unwrap_or(holder).trim();
    let mode = payload["mode"].as_str().unwrap_or("editor");

    let now = current_unix_timestamp();
    let mut map = leases.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(existing) = map.get(id) {
        let elapsed = now.saturating_sub(existing.last_heartbeat);
        if elapsed <= LEASE_TIMEOUT_SECS && existing.holder != holder {
            let body = json!({
                "status": "locked",
                "holder": existing.holder,
                "holderName": existing.holder_name,
                "acquiredAt": existing.acquired_at,
                "secondsAgo": elapsed,
                "timeoutRemaining": LEASE_TIMEOUT_SECS.saturating_sub(elapsed),
                "message": format!("Página bloqueada para edición por {}", existing.holder_name),
            })
            .to_string();
            return send_response(stream, 423, "Locked", "application/json", body.as_bytes());
        }
    }

    let lease =
        DocLease { holder: holder.to_string(), holder_name: holder_name.to_string(), acquired_at: now, last_heartbeat: now, mode: mode.to_string() };
    map.insert(id.to_string(), lease);

    let body = json!({
        "status": "acquired",
        "holder": holder,
        "holderName": holder_name,
        "leaseTimeoutSeconds": LEASE_TIMEOUT_SECS,
    })
    .to_string();
    send_response(stream, 200, "OK", "application/json", body.as_bytes())
}

fn handle_document_lease_heartbeat(reader: &mut impl Read, stream: &mut TcpStream, leases: &DocLeases, id: &str, len: usize) -> Result<(), String> {
    let payload = if len > 0 { read_json_body(reader, len).unwrap_or(json!({})) } else { json!({}) };
    let holder = payload["holder"].as_str().unwrap_or("").trim();
    let now = current_unix_timestamp();
    let mut map = leases.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(existing) = map.get_mut(id)
        && (existing.holder == holder || holder.is_empty())
    {
        existing.last_heartbeat = now;
        let body = json!({
            "status": "renewed",
            "lastHeartbeat": now,
        })
        .to_string();
        return send_response(stream, 200, "OK", "application/json", body.as_bytes());
    }

    let err = json!({"error": "lease not found or held by another user"}).to_string();
    send_response(stream, 404, "Not Found", "application/json", err.as_bytes())
}

fn handle_document_lease_release(reader: &mut impl Read, stream: &mut TcpStream, leases: &DocLeases, id: &str, len: usize) -> Result<(), String> {
    let payload = if len > 0 { read_json_body(reader, len).unwrap_or(json!({})) } else { json!({}) };
    let holder = payload["holder"].as_str().unwrap_or("").trim();
    let force = payload["force"].as_bool().unwrap_or(false);

    let mut map = leases.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = map.get(id)
        && (force || holder.is_empty() || existing.holder == holder)
    {
        map.remove(id);
    }

    let body = json!({"status": "released"}).to_string();
    send_response(stream, 200, "OK", "application/json", body.as_bytes())
}

fn serve_static(stream: &mut TcpStream, dist: &Path, path: &str) -> Result<(), String> {
    let clean = path.trim_start_matches('/');
    let target = if clean.is_empty() || clean == "index.html" { dist.join("index.html") } else { dist.join(clean) };

    // Prevent directory traversal
    if clean.contains("..") {
        return send_response(stream, 403, "Forbidden", "text/plain", b"Forbidden");
    }

    if target.is_file() {
        let bytes = fs::read(&target).map_err(|e| e.to_string())?;
        let mime = mime_for_ext(target.extension().and_then(|s| s.to_str()).unwrap_or(""));
        let is_immutable = clean.ends_with(".woff2") || clean.ends_with(".otf") || clean.ends_with(".ttf");
        if is_immutable {
            return send_cached_response(stream, 200, "OK", mime, &bytes);
        } else {
            return send_response(stream, 200, "OK", mime, &bytes);
        }
    }

    // SPA fallback: return index.html if file doesn't exist
    let index = dist.join("index.html");
    if index.is_file() {
        let bytes = fs::read(&index).map_err(|e| e.to_string())?;
        return send_response(stream, 200, "OK", "text/html; charset=utf-8", &bytes);
    }

    // Welcome page when web bundle is not built yet
    let fallback = format!(
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>DesignCraft Server</title></head>\
         <body style='font-family: system-ui, sans-serif; padding: 40px; line-height: 1.6;'>\
         <h1>DesignCraft Editorial Server</h1>\
         <p>The server is running, but the Web UI is not compiled at <code>{}</code>.</p>\
         <p>Compile the web client with:</p>\
         <pre style='background: #eee; padding: 12px; border-radius: 6px;'>trunk build --release apps/designcraft-web/index.html --dist ./dist/web</pre>\
         <h3>Available API endpoints:</h3>\
         <ul>\
           <li><a href='/api/templates'>/api/templates</a></li>\
           <li><a href='/api/documents'>/api/documents</a></li>\
           <li><a href='/api/health'>/api/health</a></li>\
         </ul></body></html>",
        dist.display()
    );

    send_response(stream, 200, "OK", "text/html; charset=utf-8", fallback.as_bytes())
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "wasm" => "application/wasm",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "designcraft" => "application/vnd.designcraft+zip",
        _ => "application/octet-stream",
    }
}

fn send_response(stream: &mut TcpStream, code: u16, status_text: &str, content_type: &str, body: &[u8]) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {code} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, PUT, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
         Cache-Control: no-cache, no-store, must-revalidate\r\n\
         Pragma: no-cache\r\n\
         Expires: 0\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn send_cached_response(stream: &mut TcpStream, code: u16, status_text: &str, content_type: &str, body: &[u8]) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {code} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, OPTIONS\r\n\
         Cache-Control: public, max-age=31536000, immutable\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn run_curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("curl").args(args).output().map_err(|e| format!("failed to execute curl: {e}"))?;
    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed with status {:?}: {}", output.status.code(), err_msg));
    }
    Ok(output.stdout)
}

fn run_curl_to_file(args: &[&str], dest: &Path) -> Result<(), String> {
    let dest_str = dest.to_string_lossy();
    let mut full_args = args.to_vec();
    full_args.extend(&["-o", &dest_str]);
    let output = std::process::Command::new("curl").args(&full_args).output().map_err(|e| format!("failed to execute curl: {e}"))?;
    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed with status {:?}: {}", output.status.code(), err_msg));
    }
    Ok(())
}

fn add_template_file_to_list(
    p: &Path,
    templates_root: &Path,
    folder_cat: Option<&str>,
    out_list: &mut Vec<serde_json::Value>,
    cat_counts: &mut HashMap<String, usize>,
) {
    let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
        return;
    };
    let ext_lower = ext.to_ascii_lowercase();
    if ext_lower != "idml" && ext_lower != "designcraft" {
        return;
    }
    let file_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    if file_name.starts_with('.') {
        return;
    }
    let Some(stem) = sanitize_id(&file_name) else {
        return;
    };

    let is_idml = ext_lower == "idml";
    let cat = if let Some(fc) = folder_cat {
        fc.to_string()
    } else {
        let lower = stem.to_ascii_lowercase();
        if lower.contains("opinion") || lower.contains("columna") {
            "Opinión".to_string()
        } else if lower.contains("editorial") || lower.contains("reportaje") {
            "Editorial".to_string()
        } else if lower.contains("revista") {
            "Revista".to_string()
        } else if lower.contains("portada") || stem == "1" {
            "Portada".to_string()
        } else {
            "General".to_string()
        }
    };

    let has_thumb = templates_root.join(format!("{stem}.png")).is_file()
        || templates_root.join(format!("{stem}.jpg")).is_file()
        || templates_root.join("cloud").join("thumbnails").join(format!("{stem}.png")).is_file()
        || templates_root.join("cloud").join("thumbnails").join(format!("{stem}.jpg")).is_file()
        || p.parent().is_some_and(|par| par.join(format!("{stem}.png")).is_file() || par.join(format!("{stem}.jpg")).is_file())
        || !is_idml;

    let meta = p.metadata().ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);

    let display_name = stem
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ");

    let count = cat_counts.entry(cat.clone()).or_insert(0);
    *count = count.saturating_add(1);

    out_list.push(json!({
        "id": stem,
        "name": if display_name.is_empty() { stem.clone() } else { display_name },
        "filename": file_name,
        "category": cat,
        "isIdml": is_idml,
        "size": size,
        "hasThumbnail": has_thumb,
        "thumbnailUrl": if has_thumb { format!("/api/cloud/thumbnails/{stem}") } else { String::new() },
        "source": "local"
    }));
}

fn scan_local_templates(templates: &Path, out_list: &mut Vec<serde_json::Value>, cat_counts: &mut HashMap<String, usize>) {
    if let Ok(entries) = fs::read_dir(templates) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let dir_name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if dir_name == "cloud" || dir_name.starts_with('.') || dir_name == "Document fonts" || dir_name == "Links" {
                    continue;
                }
                if let Ok(sub_entries) = fs::read_dir(&p) {
                    for sub in sub_entries.flatten() {
                        add_template_file_to_list(&sub.path(), templates, Some(&dir_name), out_list, cat_counts);
                    }
                }
            } else if p.is_file() {
                add_template_file_to_list(&p, templates, None, out_list, cat_counts);
            }
        }
    }
}

fn get_or_build_cloud_catalog(templates: &Path) -> Result<serde_json::Value, String> {
    let cloud_dir = templates.join("cloud");
    let catalog_file = cloud_dir.join("catalog.json");
    let mut drive_templates = Vec::new();
    let mut cat_counts: HashMap<String, usize> = HashMap::new();

    if catalog_file.is_file()
        && let Ok(bytes) = fs::read(&catalog_file)
        && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(arr) = val.get("templates").and_then(|t| t.as_array())
    {
        for t in arr {
            if t["source"] == "drive" {
                drive_templates.push(t.clone());
                let cat = t["category"].as_str().unwrap_or("General");
                let count = cat_counts.entry(cat.to_string()).or_insert(0);
                *count = count.saturating_add(1);
            }
        }
    }

    let mut local_list = Vec::new();
    scan_local_templates(templates, &mut local_list, &mut cat_counts);
    for lt in local_list {
        let lt_name = lt["name"].as_str().unwrap_or("");
        if !drive_templates.iter().any(|dt| dt["name"].as_str() == Some(lt_name)) {
            drive_templates.push(lt);
        }
    }

    drive_templates.sort_by(|a, b| {
        let name_a = a["name"].as_str().unwrap_or("");
        let name_b = b["name"].as_str().unwrap_or("");
        name_a.cmp(name_b)
    });

    let mut categories_list = vec![json!({
        "id": "all",
        "name": "Todas",
        "count": drive_templates.len()
    })];

    let mut sorted_cats: Vec<(String, usize)> = cat_counts.into_iter().collect();
    sorted_cats.sort_by(|a, b| a.0.cmp(&b.0));
    for (cat_name, count) in sorted_cats {
        categories_list.push(json!({
            "id": cat_name,
            "name": cat_name,
            "count": count
        }));
    }

    let catalog = json!({
        "status": "ok",
        "syncedAt": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "categories": categories_list,
        "templates": drive_templates
    });

    let _ = fs::create_dir_all(&cloud_dir);
    let _ = fs::write(&catalog_file, catalog.to_string());

    Ok(catalog)
}

fn fetch_drive_folder_items(token: &str, folder_id: &str) -> Result<Vec<serde_json::Value>, String> {
    let mut all_files = Vec::new();
    let mut page_token: Option<String> = None;
    let query = format!("'{}' in parents and trashed = false", folder_id);

    // Limit pages to 10 (up to 10,000 files per folder)
    for _ in 0..10 {
        let mut url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=nextPageToken,files(id,name,mimeType,size,thumbnailLink,shortcutDetails)&pageSize=1000&supportsAllDrives=true&includeItemsFromAllDrives=true",
            url_encode(&query)
        );
        if let Some(ref pt) = page_token {
            url.push_str("&pageToken=");
            url.push_str(&url_encode(pt));
        }

        let bytes = run_curl(&["-s", "--connect-timeout", "10", "--max-time", "30", "-H", &format!("Authorization: Bearer {token}"), &url])?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid Google Drive JSON: {e}"))?;

        if let Some(err) = json.get("error") {
            let msg = err["message"].as_str().unwrap_or("unknown Google Drive API error");
            return Err(format!("Google Drive API: {msg}"));
        }

        if let Some(files) = json["files"].as_array() {
            all_files.extend(files.iter().cloned());
        }

        if let Some(next) = json["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
            page_token = Some(next.to_string());
        } else {
            break;
        }
    }

    Ok(all_files)
}

struct FolderQueueItem {
    id: String,
    category_path: String,
    depth: usize,
}

fn sync_cloud_catalog_with_drive(templates: &Path, token: &str, folder_id: &str) -> Result<serde_json::Value, String> {
    if folder_id.trim().is_empty() {
        return get_or_build_cloud_catalog(templates);
    }

    let cloud_dir = templates.join("cloud");
    let thumbs_dir = cloud_dir.join("thumbnails");
    fs::create_dir_all(&thumbs_dir).map_err(|e| e.to_string())?;

    // Persist token for background thumbnail generation
    if !token.trim().is_empty() {
        let _ = fs::write(cloud_dir.join(".drive_token"), token.trim());
    }

    println!("[serve] Sincronizando con Google Drive (exploración recursiva) carpeta `{folder_id}`...");

    let mut queue = VecDeque::new();
    queue.push_back(FolderQueueItem { id: folder_id.to_string(), category_path: String::new(), depth: 0 });

    let mut visited: HashSet<String> = HashSet::new();
    let mut drive_templates = Vec::new();
    let mut cat_counts: HashMap<String, usize> = HashMap::new();

    while let Some(current) = queue.pop_front() {
        if visited.len() >= 200 || current.depth > 6 {
            continue;
        }
        if !visited.insert(current.id.clone()) {
            continue;
        }

        let files = match fetch_drive_folder_items(token, &current.id) {
            Ok(f) => f,
            Err(e) => {
                println!("[serve] Advertencia listando carpeta `{}`: {e}", current.id);
                if current.depth == 0 {
                    return Err(format!("Google Drive API: {e}"));
                }
                continue;
            }
        };

        let mut folder_templates = Vec::new();
        let mut images_by_stem: HashMap<String, (String, String)> = HashMap::new();

        for f in &files {
            let name = f["name"].as_str().unwrap_or("");
            let mime = f["mimeType"].as_str().unwrap_or("");
            let id = f["id"].as_str().unwrap_or("");

            // Folders and shortcuts to folders
            let is_folder = mime == "application/vnd.google-apps.folder";
            let (target_id, is_shortcut_folder) = if mime == "application/vnd.google-apps.shortcut" {
                let target_mime = f["shortcutDetails"]["targetMimeType"].as_str().unwrap_or("");
                let tid = f["shortcutDetails"]["targetId"].as_str().unwrap_or("");
                (tid, target_mime == "application/vnd.google-apps.folder")
            } else {
                ("", false)
            };

            if is_folder || is_shortcut_folder {
                let sub_id = if is_folder { id } else { target_id };
                if !sub_id.is_empty() {
                    let sub_path = if current.category_path.is_empty() { name.to_string() } else { format!("{} / {}", current.category_path, name) };
                    queue.push_back(FolderQueueItem { id: sub_id.to_string(), category_path: sub_path, depth: current.depth.saturating_add(1) });
                }
                continue;
            }

            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".webp")
                || mime.starts_with("image/")
            {
                let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
                images_by_stem.insert(stem, (id.to_string(), name.to_string()));
            } else if lower.ends_with(".idml") || lower.ends_with(".designcraft") || mime.contains("indesign") {
                folder_templates.push(f.clone());
            }
        }

        let cat_name = if current.category_path.is_empty() { "General".to_string() } else { current.category_path.clone() };

        for f in folder_templates {
            let name = f["name"].as_str().unwrap_or("");
            let id = f["id"].as_str().unwrap_or("");
            let mime = f["mimeType"].as_str().unwrap_or("");
            let lower = name.to_ascii_lowercase();
            let is_idml = lower.ends_with(".idml") || mime.contains("indesign");

            let stem =
                name.strip_suffix(".idml").or_else(|| name.strip_suffix(".IDML")).or_else(|| name.strip_suffix(".designcraft")).unwrap_or(name);
            let stem_lower = stem.to_ascii_lowercase();

            let mut preview_id = String::new();
            let mut has_thumb = false;

            if let Some((img_id, _img_name)) = images_by_stem.get(&stem_lower) {
                preview_id = img_id.clone();
                has_thumb = true;
                let thumb_path = thumbs_dir.join(format!("{id}.png"));
                if !thumb_path.is_file() {
                    let dl_url = format!("https://www.googleapis.com/drive/v3/files/{img_id}?alt=media&supportsAllDrives=true");
                    let _ = run_curl_to_file(
                        &["-s", "--connect-timeout", "6", "--max-time", "12", "-H", &format!("Authorization: Bearer {token}"), &dl_url],
                        &thumb_path,
                    );
                }
            } else if let Some(thumb_link) = f["thumbnailLink"].as_str() {
                has_thumb = true;
                let thumb_path = thumbs_dir.join(format!("{id}.png"));
                if !thumb_path.is_file() {
                    let _ = run_curl_to_file(&["-s", "--connect-timeout", "6", "--max-time", "12", thumb_link], &thumb_path);
                }
            } else if !is_idml {
                // Native DesignCraft template without a Drive companion image
                let thumb_path = thumbs_dir.join(format!("{id}.png"));
                let dc_dest = cloud_dir.join(format!("{id}.designcraft"));
                if !thumb_path.is_file() {
                    if !dc_dest.is_file() {
                        let dl_url = format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media&supportsAllDrives=true");
                        let _ = run_curl_to_file(
                            &["-s", "-L", "--connect-timeout", "10", "--max-time", "45", "-H", &format!("Authorization: Bearer {token}"), &dl_url],
                            &dc_dest,
                        );
                    }
                    if dc_dest.is_file()
                        && let Ok(dc_bytes) = fs::read(&dc_dest)
                        && let Some(png_bytes) = render_designcraft_thumbnail(&dc_bytes)
                    {
                        let _ = fs::write(&thumb_path, &png_bytes);
                    }
                }
                has_thumb = true;
            }

            let size: u64 = f["size"].as_str().and_then(|s| s.parse().ok()).or_else(|| f["size"].as_u64()).unwrap_or(0);
            let display_name = stem.replace(['_', '-'], " ");

            let count = cat_counts.entry(cat_name.clone()).or_insert(0);
            *count = count.saturating_add(1);

            drive_templates.push(json!({
                "id": id,
                "name": display_name,
                "filename": name,
                "category": cat_name,
                "categoryId": current.id,
                "isIdml": is_idml,
                "size": size,
                "hasThumbnail": has_thumb,
                "thumbnailUrl": if has_thumb { format!("/api/cloud/thumbnails/{id}") } else { String::new() },
                "previewFileId": preview_id,
                "source": "drive"
            }));
        }
    }

    // Merge any local disk templates that aren't already present from Drive
    let mut local_list = Vec::new();
    let mut local_cat_counts = HashMap::new();
    scan_local_templates(templates, &mut local_list, &mut local_cat_counts);
    for lt in local_list {
        let lt_name = lt["name"].as_str().unwrap_or("");
        if !drive_templates.iter().any(|dt| dt["name"].as_str() == Some(lt_name)) {
            let cat = lt["category"].as_str().unwrap_or("General");
            let count = cat_counts.entry(cat.to_string()).or_insert(0);
            *count = count.saturating_add(1);
            drive_templates.push(lt);
        }
    }

    let mut categories_list = vec![json!({
        "id": "all",
        "name": "Todas",
        "count": drive_templates.len()
    })];

    let mut sorted_cats: Vec<(String, usize)> = cat_counts.into_iter().collect();
    sorted_cats.sort_by(|a, b| a.0.cmp(&b.0));
    for (cat_name, count) in sorted_cats {
        categories_list.push(json!({
            "id": cat_name,
            "name": cat_name,
            "count": count
        }));
    }

    let catalog = json!({
        "status": "ok",
        "syncedAt": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "categories": categories_list,
        "templates": drive_templates
    });

    let catalog_file = cloud_dir.join("catalog.json");
    let has_drive_templates = drive_templates.iter().any(|dt| dt["source"] == "drive");
    if !has_drive_templates
        && catalog_file.is_file()
        && let Ok(existing_bytes) = fs::read(&catalog_file)
        && let Ok(existing_cat) = serde_json::from_slice::<serde_json::Value>(&existing_bytes)
        && existing_cat.get("templates").and_then(|t| t.as_array()).is_some_and(|a| a.iter().any(|x| x["source"] == "drive"))
    {
        println!("[serve] Google Drive no devolvió maquetas. Conservando catálogo previo en disco.");
        return Ok(existing_cat);
    }

    let _ = fs::write(&catalog_file, catalog.to_string());

    println!("[serve] Google Drive sincronizado exitosamente: {} maquetas en {} categorías.", drive_templates.len(), categories_list.len());
    Ok(catalog)
}

fn parse_query_param(query: &str, key: &str) -> Option<String> {
    for part in query.split('&') {
        if let Some((k, v)) = part.split_once('=')
            && k == key
        {
            return Some(url_decode(v));
        }
    }
    None
}

fn render_designcraft_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let doc = designcraft_format::load(bytes).ok()?;
    let p0 = doc.page(0)?;
    let scale = (360.0 / p0.width.max(1.0)).clamp(0.05, 1.0);
    let mut renderer = designcraft_render::Renderer::new();
    let cache = designcraft_compose::Cache::default();
    let opts = designcraft_render::RenderOptions { printing_only: true, ..Default::default() };
    let rendered = renderer.render_page(&doc, &cache, 0, scale, true, &opts)?;
    Some(rendered.to_png())
}

fn serve_cloud_thumbnail(stream: &mut TcpStream, templates: &Path, docs: &Path, thumb_id: &str, query: &str) -> Result<(), String> {
    let Some(clean_id) = sanitize_id(thumb_id) else {
        let err = json!({"error": "invalid thumbnail id"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };

    let thumbs_dir = templates.join("cloud").join("thumbnails");
    let token_file = templates.join("cloud").join(".drive_token");

    // 0. Cache or resolve Google Drive access token
    let query_token = parse_query_param(query, "token");
    if let Some(ref tok) = query_token {
        let trimmed = tok.trim();
        if !trimmed.is_empty() {
            let _ = fs::create_dir_all(templates.join("cloud"));
            let _ = fs::write(&token_file, trimmed);
        }
    }
    let effective_token = query_token
        .filter(|s| !s.trim().is_empty())
        .or_else(|| fs::read_to_string(&token_file).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));

    // 1. Direct pre-rendered or existing thumbnail files
    let candidates = [
        thumbs_dir.join(format!("{clean_id}.png")),
        thumbs_dir.join(format!("{clean_id}.jpg")),
        templates.join(format!("{clean_id}.png")),
        templates.join(format!("{clean_id}.jpg")),
    ];

    for path in &candidates {
        if path.is_file()
            && let Ok(bytes) = fs::read(path)
        {
            let mime = if path.extension().and_then(|s| s.to_str()).unwrap_or("").eq_ignore_ascii_case("jpg") { "image/jpeg" } else { "image/png" };
            return send_cached_response(stream, 200, "OK", mime, &bytes);
        }
    }

    // 1b. Check immediate subdirectories of templates/ for companion image
    if let Ok(entries) = fs::read_dir(templates) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name();
                let s = name.to_string_lossy();
                if s == "cloud" || s == "Links" || s.starts_with('.') {
                    continue;
                }
                let sub_png = p.join(format!("{clean_id}.png"));
                if sub_png.is_file()
                    && let Ok(bytes) = fs::read(&sub_png)
                {
                    return send_cached_response(stream, 200, "OK", "image/png", &bytes);
                }
                let sub_jpg = p.join(format!("{clean_id}.jpg"));
                if sub_jpg.is_file()
                    && let Ok(bytes) = fs::read(&sub_jpg)
                {
                    return send_cached_response(stream, 200, "OK", "image/jpeg", &bytes);
                }
            }
        }
    }

    // 2. Fast instant extraction of embedded XMP JPEG from IDML if present locally
    let (local_dc, local_idml) = find_local_template(templates, &clean_id);
    let mut idml_paths = vec![templates.join("cloud").join(format!("{clean_id}.idml")), templates.join(format!("{clean_id}.idml"))];
    if let Some(ref li) = local_idml {
        idml_paths.push(li.clone());
    }
    for idml_path in &idml_paths {
        if idml_path.is_file()
            && let Ok(idml_bytes) = fs::read(idml_path)
            && let Some(jpeg_bytes) = designcraft_idml::extract_thumbnail(&idml_bytes)
        {
            let _ = fs::create_dir_all(&thumbs_dir);
            let cache_dest = thumbs_dir.join(format!("{clean_id}.jpg"));
            let _ = fs::write(&cache_dest, &jpeg_bytes);
            return send_cached_response(stream, 200, "OK", "image/jpeg", &jpeg_bytes);
        }
    }

    // 3. Render thumbnail from .designcraft (local, cached in cloud, or in docs)
    let dc_cloud = templates.join("cloud").join(format!("{clean_id}.designcraft"));
    let mut dc_bytes_opt: Option<Vec<u8>> = None;

    if dc_cloud.is_file() {
        dc_bytes_opt = fs::read(&dc_cloud).ok();
    } else if let Some(ref ldc) = local_dc {
        dc_bytes_opt = fs::read(ldc).ok();
    } else if templates.join(format!("{clean_id}.designcraft")).is_file() {
        dc_bytes_opt = fs::read(templates.join(format!("{clean_id}.designcraft"))).ok();
    } else if docs.join(format!("{clean_id}.designcraft")).is_file() {
        dc_bytes_opt = fs::read(docs.join(format!("{clean_id}.designcraft"))).ok();
    }

    // 4. If not found locally, try downloading from Google Drive if token is available
    if dc_bytes_opt.is_none()
        && let Some(ref tok) = effective_token
    {
        println!("[serve] Descargando archivo `{clean_id}` desde Google Drive para generar miniatura...");
        let dl_url = format!("https://www.googleapis.com/drive/v3/files/{clean_id}?alt=media&supportsAllDrives=true");
        if let Ok(raw_bytes) =
            run_curl(&["-s", "-L", "--connect-timeout", "10", "--max-time", "60", "-H", &format!("Authorization: Bearer {tok}"), &dl_url])
            && !raw_bytes.is_empty()
        {
            if designcraft_idml::is_idml(&raw_bytes) {
                if let Some(jpeg_bytes) = designcraft_idml::extract_thumbnail(&raw_bytes) {
                    let _ = fs::create_dir_all(&thumbs_dir);
                    let cache_dest = thumbs_dir.join(format!("{clean_id}.jpg"));
                    let _ = fs::write(&cache_dest, &jpeg_bytes);
                    return send_cached_response(stream, 200, "OK", "image/jpeg", &jpeg_bytes);
                }
            } else {
                let _ = fs::create_dir_all(templates.join("cloud"));
                let _ = fs::write(&dc_cloud, &raw_bytes);
                dc_bytes_opt = Some(raw_bytes);
            }
        }
    }

    // 5. Render .designcraft document to PNG thumbnail
    if let Some(dc_bytes) = dc_bytes_opt
        && let Some(png_bytes) = render_designcraft_thumbnail(&dc_bytes)
    {
        let _ = fs::create_dir_all(&thumbs_dir);
        let cache_dest = thumbs_dir.join(format!("{clean_id}.png"));
        let _ = fs::write(&cache_dest, &png_bytes);
        return send_cached_response(stream, 200, "OK", "image/png", &png_bytes);
    }

    let err = json!({"error": format!("thumbnail `{clean_id}` not found")}).to_string();
    send_response(stream, 404, "Not Found", "application/json", err.as_bytes())
}

fn find_local_template(templates: &Path, clean_id: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let dc_name = format!("{clean_id}.designcraft");
    let idml_name = format!("{clean_id}.idml");

    // 1. Direct in templates/
    let direct_dc = templates.join(&dc_name);
    let direct_idml = templates.join(&idml_name);
    if direct_dc.is_file() || direct_idml.is_file() {
        return (direct_dc.is_file().then_some(direct_dc), direct_idml.is_file().then_some(direct_idml));
    }

    // 2. Search immediate subdirectories (excluding cloud, Links, hidden)
    let mut found_dc = None;
    let mut found_idml = None;
    if let Ok(entries) = fs::read_dir(templates) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let s = name.to_string_lossy();
                if s == "cloud" || s == "Links" || s.starts_with('.') {
                    continue;
                }
                let sub_dc = path.join(&dc_name);
                if sub_dc.is_file() && found_dc.is_none() {
                    found_dc = Some(sub_dc);
                }
                let sub_idml = path.join(&idml_name);
                if sub_idml.is_file() && found_idml.is_none() {
                    found_idml = Some(sub_idml);
                }
            }
        }
    }

    (found_dc, found_idml)
}

fn open_cloud_template(
    stream: &mut TcpStream,
    templates: &Path,
    docs: &Path,
    locks: &DocLocks,
    id: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let Some(clean_id) = sanitize_id(id) else {
        let err = json!({"error": "invalid template id"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };

    let token = payload["accessToken"].as_str();
    let target_doc_id = payload["targetDocId"].as_str().and_then(sanitize_id).unwrap_or_else(|| clean_id.clone());
    let force = payload["force"].as_bool().unwrap_or(false);

    let target_doc_path = docs.join(format!("{target_doc_id}.designcraft"));
    if !force && target_doc_path.is_file() {
        let is_valid = fs::read(&target_doc_path).ok().map(|b| designcraft_format::load(&b).is_ok()).unwrap_or(false);
        if is_valid {
            let res = json!({
                "status": "conflict",
                "exists": true,
                "docId": target_doc_id,
                "url": format!("/?doc={}&mode=editor", url_encode(&target_doc_id))
            })
            .to_string();
            return send_response(stream, 409, "Conflict", "application/json", res.as_bytes());
        }
    }

    let cloud_cached = templates.join("cloud").join(format!("{clean_id}.designcraft"));
    let (local_tmpl, local_idml) = find_local_template(templates, &clean_id);

    let doc_bytes = if cloud_cached.is_file() {
        fs::read(&cloud_cached).map_err(|e| e.to_string())?
    } else if let Some(lt) = local_tmpl {
        fs::read(&lt).map_err(|e| e.to_string())?
    } else if let Some(li) = local_idml {
        let idml_bytes = fs::read(&li).map_err(|e| e.to_string())?;
        let doc = designcraft_idml::import_idml(&idml_bytes).map_err(|e| format!("cannot parse IDML: {e}"))?;
        let saved = designcraft_format::save(&doc).map_err(|e| format!("cannot serialize designcraft: {e}"))?;
        let cloud_dir = templates.join("cloud");
        let _ = fs::create_dir_all(&cloud_dir);
        let _ = fs::write(&cloud_cached, &saved);
        saved
    } else if let Some(tok) = token.filter(|t| !t.trim().is_empty()) {
        println!("[serve] Descargando plantilla `{clean_id}` desde Google Drive...");
        let url = format!("https://www.googleapis.com/drive/v3/files/{clean_id}?alt=media&supportsAllDrives=true");
        let raw_bytes = run_curl(&["-s", "-L", "--max-time", "120", "-H", &format!("Authorization: Bearer {tok}"), &url])?;
        if raw_bytes.is_empty() {
            let err = json!({"error": "downloaded file is empty"}).to_string();
            return send_response(stream, 502, "Bad Gateway", "application/json", err.as_bytes());
        }

        if raw_bytes.starts_with(b"{\n \"error\":") || raw_bytes.starts_with(b"{\"error\":") {
            let err_val = serde_json::from_slice::<serde_json::Value>(&raw_bytes).ok();
            if let Some(err_obj) = err_val.as_ref().and_then(|v| v.get("error")) {
                let code = err_obj.get("code").and_then(|c| c.as_u64()).unwrap_or(401);
                let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("Error en credenciales de Google Drive");
                eprintln!("[serve] Error de autenticación Google Drive ({code}): {msg}");
                let res = json!({
                    "error": "google_auth_error",
                    "status": code,
                    "message": format!("Sesión de Google Drive expirada o no autorizada ({code}): {msg}")
                })
                .to_string();
                return send_response(
                    stream,
                    if code == 401 { 401 } else { 502 },
                    if code == 401 { "Unauthorized" } else { "Bad Gateway" },
                    "application/json",
                    res.as_bytes(),
                );
            }
        }

        let converted = if designcraft_idml::is_idml(&raw_bytes) {
            let doc = designcraft_idml::import_idml(&raw_bytes).map_err(|e| format!("cannot parse IDML: {e}"))?;
            designcraft_format::save(&doc).map_err(|e| format!("cannot serialize designcraft: {e}"))?
        } else if designcraft_format::load(&raw_bytes).is_ok() {
            raw_bytes
        } else {
            let err = json!({
                "error": "invalid_template_format",
                "message": "El archivo descargado no es una maqueta compatible (.designcraft o .idml)"
            })
            .to_string();
            return send_response(stream, 502, "Bad Gateway", "application/json", err.as_bytes());
        };

        let cloud_dir = templates.join("cloud");
        let _ = fs::create_dir_all(&cloud_dir);
        let _ = fs::write(&cloud_cached, &converted);
        converted
    } else {
        eprintln!("[serve] Intento de abrir plantilla remota `{clean_id}` sin sesión de Google activa");
        let res = json!({
            "error": "google_auth_error",
            "status": 401,
            "message": "Se requiere iniciar sesión con Google Drive para descargar esta plantilla."
        })
        .to_string();
        return send_response(stream, 401, "Unauthorized", "application/json", res.as_bytes());
    };

    // If target metadata is supplied, inject edition metadata and update text variables and placeholders
    let final_bytes = if let Ok(mut doc) = designcraft_format::load(&doc_bytes) {
        let edition_date = payload["editionDate"].as_str();
        let section = payload["section"].as_str();
        let page_number = payload["pageNumber"].as_u64().map(|n| n as u32);

        let n_pages = doc.page_count();
        for i in 0..n_pages {
            if let Some(page) = doc.page_mut(i) {
                if let Some(d) = edition_date {
                    page.editorial.edition_date = Some(d.to_string());
                }
                if let Some(s) = section {
                    page.editorial.section = Some(s.to_string());
                }
                if let Some(p) = page_number {
                    page.editorial.page_number = Some(p.saturating_add(i as u32));
                }
                page.editorial.status = designcraft_doc::PageStatus::InProgress;
            }
        }

        doc.title = target_doc_id.clone();

        // Synchronize text variables in document
        if let Some(d) = edition_date {
            if let Some(v) = doc.text_variables.iter_mut().find(|v| v.name.eq_ignore_ascii_case("Fecha")) {
                v.kind = designcraft_doc::vars::VarKind::Custom { text: d.to_string() };
            } else {
                doc.text_variables
                    .push(designcraft_doc::vars::TextVariable::new("Fecha", designcraft_doc::vars::VarKind::Custom { text: d.to_string() }));
            }
        }
        if let Some(s) = section {
            if let Some(v) = doc.text_variables.iter_mut().find(|v| v.name.eq_ignore_ascii_case("Sección")) {
                v.kind = designcraft_doc::vars::VarKind::Custom { text: s.to_string() };
            } else {
                doc.text_variables
                    .push(designcraft_doc::vars::TextVariable::new("Sección", designcraft_doc::vars::VarKind::Custom { text: s.to_string() }));
            }
        }
        if let Some(p) = page_number {
            let p_str = format!("{p}");
            if let Some(v) = doc.text_variables.iter_mut().find(|v| v.name.eq_ignore_ascii_case("Página")) {
                v.kind = designcraft_doc::vars::VarKind::Custom { text: p_str.clone() };
            } else {
                doc.text_variables.push(designcraft_doc::vars::TextVariable::new("Página", designcraft_doc::vars::VarKind::Custom { text: p_str }));
            }
        }

        // Replace placeholders in stories
        for story in doc.stories.values_mut() {
            let mut text = story.text.clone();
            let mut changed = false;
            if let Some(d) = edition_date {
                let replaced = text.replace("[FECHA]", d).replace("[Fecha]", d);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if let Some(s) = section {
                let replaced = text.replace("[SECCION]", s).replace("[SECCIÓN]", s).replace("[Sección]", s);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if let Some(p) = page_number {
                let pstr = format!("{p:02}");
                let replaced = text.replace("[PAGINA]", &pstr).replace("[PÁGINA]", &pstr).replace("[Página]", &pstr);
                if replaced != text {
                    text = replaced;
                    changed = true;
                }
            }
            if changed {
                let mut modified = (**story).clone();
                modified.text = text;
                *story = std::sync::Arc::new(modified);
            }
        }

        designcraft_format::save(&doc).unwrap_or(doc_bytes)
    } else {
        doc_bytes
    };

    let doc_lock = get_doc_lock(locks, &target_doc_id);
    let _guard = doc_lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    fs::write(&target_doc_path, &final_bytes).map_err(|e| e.to_string())?;

    let res = json!({
        "status": "ok",
        "docId": target_doc_id,
        "url": format!("/?doc={}&mode=editor", url_encode(&target_doc_id))
    })
    .to_string();
    send_response(stream, 200, "OK", "application/json", res.as_bytes())
}

fn upload_or_update_drive_file(token: &str, parent_id: &str, file_name: &str, file_path: &Path, mime_type: &str) -> Result<String, String> {
    let safe_name = file_name.replace('\'', "\\'");
    let query = format!("'{}' in parents and name = '{}' and trashed = false", parent_id, safe_name);
    let search_url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true",
        url_encode(&query)
    );
    let search_out = run_curl(&["-s", "-H", &format!("Authorization: Bearer {token}"), &search_url])?;
    let search_json: serde_json::Value = serde_json::from_slice(&search_out).unwrap_or(json!({}));

    if let Some(err_obj) = search_json.get("error") {
        let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("error de Google Drive");
        return Err(format!("Drive API search error: {msg}"));
    }

    let existing_id = search_json["files"].as_array().and_then(|arr| arr.first()).and_then(|f| f["id"].as_str()).map(String::from);

    if let Some(fid) = existing_id {
        // Update existing file content (PATCH)
        println!("[serve] Actualizando archivo existente `{file_name}` (ID: {fid}) en Google Drive...");
        let patch_url = format!("https://www.googleapis.com/upload/drive/v3/files/{fid}?uploadType=media&supportsAllDrives=true");
        let patch_out = run_curl(&[
            "-s",
            "-X",
            "PATCH",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            &format!("Content-Type: {mime_type}"),
            "--data-binary",
            &format!("@{}", file_path.to_string_lossy()),
            &patch_url,
        ])?;
        let patch_json: serde_json::Value = serde_json::from_slice(&patch_out).unwrap_or(json!({}));
        if let Some(err_obj) = patch_json.get("error") {
            let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("error de Google Drive");
            return Err(format!("Error actualizando `{file_name}`: {msg}"));
        }
        let id = patch_json["id"].as_str().unwrap_or(&fid).to_string();
        Ok(id)
    } else {
        // Create new file (POST multipart)
        println!("[serve] Creando nuevo archivo `{file_name}` en Google Drive (carpeta: {parent_id})...");
        let file_meta = json!({
            "name": file_name,
            "parents": [parent_id]
        })
        .to_string();
        let upload_url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&supportsAllDrives=true&fields=id,name";
        let upload_out = run_curl(&[
            "-s",
            "-X",
            "POST",
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-F",
            &format!("metadata={};type=application/json;charset=UTF-8", file_meta),
            "-F",
            &format!("file=@{};type={}", file_path.to_string_lossy(), mime_type),
            upload_url,
        ])?;
        let upload_json: serde_json::Value = serde_json::from_slice(&upload_out).unwrap_or(json!({}));
        if let Some(err_obj) = upload_json.get("error") {
            let msg = err_obj.get("message").and_then(|m| m.as_str()).unwrap_or("error de Google Drive");
            return Err(format!("Error creando `{file_name}`: {msg}"));
        }
        let id = upload_json["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            return Err(format!("Google Drive no devolvió ID para `{file_name}`"));
        }
        Ok(id)
    }
}

fn upload_document_to_drive(
    stream: &mut TcpStream,
    docs: &Path,
    doc_id: &str,
    token: &str,
    dest_folder_id: &str,
    is_page_folder: bool,
    base_name: &str,
    date_str: &str,
    export_pdf: bool,
    pdf_standard: &str,
) -> Result<(), String> {
    if token.is_empty() {
        let err = json!({"error": "Falta token de acceso a Google Drive"}).to_string();
        return send_response(stream, 401, "Unauthorized", "application/json", err.as_bytes());
    }

    let Some(clean_id) = sanitize_id(doc_id) else {
        let err = json!({"error": "ID de documento inválido"}).to_string();
        return send_response(stream, 400, "Bad Request", "application/json", err.as_bytes());
    };

    let doc_path = docs.join(format!("{clean_id}.designcraft"));
    if !doc_path.is_file() {
        let err = json!({"error": format!("Documento `{clean_id}` no encontrado en el servidor")}).to_string();
        return send_response(stream, 404, "Not Found", "application/json", err.as_bytes());
    }

    let date_clean = if date_str.trim().is_empty() {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let days = now / 86400;
        format!("edicion-{days}")
    } else {
        date_str.trim().to_string()
    };

    let root_dest_id = load_env_var("VITE_DRIVE_DESTINATION_FOLDER_ID").unwrap_or_default();
    let is_direct_folder = is_page_folder || (!dest_folder_id.is_empty() && dest_folder_id != root_dest_id);

    let target_folder_id = if is_direct_folder || dest_folder_id.is_empty() || date_clean.is_empty() {
        dest_folder_id.to_string()
    } else {
        let query = format!(
            "'{}' in parents and name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
            dest_folder_id, date_clean
        );
        let search_url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true",
            url_encode(&query)
        );
        let search_out = run_curl(&["-s", "-H", &format!("Authorization: Bearer {token}"), &search_url])?;
        let search_json: serde_json::Value = serde_json::from_slice(&search_out).unwrap_or(json!({}));

        if let Some(first) = search_json["files"].as_array().and_then(|arr| arr.first()) {
            first["id"].as_str().unwrap_or(dest_folder_id).to_string()
        } else {
            let create_body = json!({
                "name": date_clean,
                "mimeType": "application/vnd.google-apps.folder",
                "parents": [dest_folder_id]
            })
            .to_string();
            let create_out = run_curl(&[
                "-s",
                "-X",
                "POST",
                "-H",
                &format!("Authorization: Bearer {token}"),
                "-H",
                "Content-Type: application/json",
                "-d",
                &create_body,
                "https://www.googleapis.com/drive/v3/files?supportsAllDrives=true&fields=id,name",
            ])?;
            let create_json: serde_json::Value = serde_json::from_slice(&create_out).unwrap_or(json!({}));
            create_json["id"].as_str().unwrap_or(dest_folder_id).to_string()
        }
    };

    println!("[serve] Guardando `{clean_id}` en Google Drive carpeta destino `{target_folder_id}`...");

    let file_stem = if !base_name.trim().is_empty() { base_name.trim().to_string() } else { clean_id.to_string() };
    let dc_filename = format!("{file_stem}.designcraft");
    let idml_filename = format!("{file_stem}.idml");
    let pdf_filename = format!("{file_stem}.pdf");

    // 1. Upload .designcraft to Drive
    let file_id = match upload_or_update_drive_file(token, &target_folder_id, &dc_filename, &doc_path, "application/vnd.designcraft+zip") {
        Ok(id) => {
            println!("[serve] ✓ Maqueta `{dc_filename}` guardada en Drive: ID {id}");
            id
        }
        Err(e) => {
            eprintln!("[serve] Error subiendo maqueta a Google Drive: {e}");
            let err_resp = json!({
                "status": "error",
                "error": format!("Error al subir maqueta a Google Drive: {e}")
            })
            .to_string();
            return send_response(stream, 502, "Bad Gateway", "application/json", err_resp.as_bytes());
        }
    };

    // 2. Locate or create Links/ folder in Google Drive
    let links_query =
        format!("'{}' in parents and name = 'Links' and mimeType = 'application/vnd.google-apps.folder' and trashed = false", target_folder_id);
    let links_search_url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true",
        url_encode(&links_query)
    );
    let links_folder_id = if let Ok(links_out) = run_curl(&["-s", "-H", &format!("Authorization: Bearer {token}"), &links_search_url]) {
        let links_json: serde_json::Value = serde_json::from_slice(&links_out).unwrap_or(json!({}));
        if let Some(first) = links_json["files"].as_array().and_then(|arr| arr.first()) {
            first["id"].as_str().unwrap_or(&target_folder_id).to_string()
        } else {
            let create_links = json!({
                "name": "Links",
                "mimeType": "application/vnd.google-apps.folder",
                "parents": [&target_folder_id]
            })
            .to_string();
            if let Ok(create_out) = run_curl(&[
                "-s",
                "-X",
                "POST",
                "-H",
                &format!("Authorization: Bearer {token}"),
                "-H",
                "Content-Type: application/json",
                "-d",
                &create_links,
                "https://www.googleapis.com/drive/v3/files?supportsAllDrives=true&fields=id,name",
            ]) {
                let c_json: serde_json::Value = serde_json::from_slice(&create_out).unwrap_or(json!({}));
                c_json["id"].as_str().unwrap_or(&target_folder_id).to_string()
            } else {
                target_folder_id.clone()
            }
        }
    } else {
        target_folder_id.clone()
    };

    // 3. Process Document Assets and IDML Package
    let mut idml_file_id = String::new();
    let mut links_count = 0usize;
    let local_links = docs.join("Links");
    let _ = fs::create_dir_all(&local_links);

    if let Ok((session, _)) = load_doc_session(docs, &clean_id)
        && let Ok(st) = session.doc()
    {
        let mut idml_doc = (*st.doc).clone();
        for (aid, a) in &mut idml_doc.assets {
            let raw_name = a.link.as_deref().unwrap_or(&a.name);
            let fname = Path::new(raw_name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("asset-{}.jpg", aid.0));

            let bytes_opt = if !a.data.is_empty() {
                Some(a.data.to_vec())
            } else {
                let cand = [
                    local_links.join(&fname),
                    docs.parent().unwrap_or(docs).join("templates").join("Links").join(&fname),
                    PathBuf::from("/home/eder/Descargas/designcraft-main/templates/Links").join(&fname),
                    PathBuf::from("/home/eder/Descargas/designcraft-main/documents/Links").join(&fname),
                    PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/ECONOMICA/Links/{fname}")),
                    PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/GENERAL/Links/{fname}")),
                    PathBuf::from(format!("/home/eder/Documentos/Plantillas corregidas/DEPORTES/Links/{fname}")),
                ];
                cand.into_iter().find(|p| p.is_file()).and_then(|p| fs::read(p).ok())
            };

            if let Some(bytes) = bytes_opt {
                let local_asset = local_links.join(&fname);
                if !local_asset.is_file() {
                    let _ = fs::write(&local_asset, &bytes);
                }

                let mime = if a.mime.is_empty() { "application/octet-stream" } else { &a.mime };
                if let Ok(link_fid) = upload_or_update_drive_file(token, &links_folder_id, &fname, &local_asset, mime) {
                    println!("[serve] ✓ Vínculo `{fname}` en Drive: ID {link_fid}");
                    links_count = links_count.saturating_add(1);
                }

                // Relink asset for IDML export so InDesign looks in Links/
                let mut na = (**a).clone();
                na.link = Some(format!("Links/{fname}"));
                na.data = Arc::new(bytes);
                *a = Arc::new(na);
            }
        }

        // Export IDML package for Adobe InDesign
        let idml_bytes = designcraft_idml::export_idml(&idml_doc);
        let idml_tmp = docs.join(format!("{clean_id}.idml"));
        if fs::write(&idml_tmp, &idml_bytes).is_ok() {
            match upload_or_update_drive_file(token, &target_folder_id, &idml_filename, &idml_tmp, "application/vnd.adobe.indesign-idml-package") {
                Ok(id) => {
                    println!("[serve] ✓ IDML `{idml_filename}` guardado en Drive: ID {id}");
                    idml_file_id = id;
                }
                Err(e) => {
                    eprintln!("[serve] Advertencia subiendo IDML: {e}");
                }
            }
        }
    }

    // 4. Export and upload Press PDF
    let mut pdf_file_id = String::new();
    if export_pdf {
        let pdf_res = if let Ok((mut session, _)) = load_doc_session(docs, &clean_id) {
            let standard = if pdf_standard == "x4" { "x4" } else { "x1a" };
            let params = json!({
                "bleed": true,
                "marks": true,
                "standard": standard,
                "tagged": true,
                "compressImages": true,
                "flatten": "high"
            });
            session.execute("file.exportPdf", &params).ok().and_then(|val| {
                let b64 = val["base64"].as_str().unwrap_or_default();
                let bytes = designcraft_engine::cmd::base64_decode(b64);
                if bytes.is_empty() { None } else { Some(bytes) }
            })
        } else {
            None
        };
        if let Some(pdf_bytes) = pdf_res {
            let pdf_tmp = docs.join(format!("{clean_id}.pdf"));
            if fs::write(&pdf_tmp, &pdf_bytes).is_ok() {
                match upload_or_update_drive_file(token, &target_folder_id, &pdf_filename, &pdf_tmp, "application/pdf") {
                    Ok(id) => {
                        println!("[serve] ✓ PDF `{pdf_filename}` guardado en Drive: ID {id}");
                        pdf_file_id = id;
                    }
                    Err(e) => {
                        eprintln!("[serve] Advertencia subiendo PDF: {e}");
                    }
                }
            }
        }
    }

    let res = json!({
        "status": "ok",
        "dateFolder": date_clean,
        "dateFolderId": target_folder_id,
        "fileId": file_id,
        "fileName": dc_filename,
        "idmlFileId": idml_file_id,
        "pdfFileId": pdf_file_id,
        "linksCount": links_count,
        "message": format!("Maqueta `{dc_filename}` guardada exitosamente en Google Drive")
    })
    .to_string();

    send_response(stream, 200, "OK", "application/json", res.as_bytes())
}
