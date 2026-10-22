//! The browser shell: web `Services`, drag-and-drop, server persistence, and the eframe web runner.

use designcraft_engine::Session;
use designcraft_ui_egui::{DesignApp, Inbox, Services};
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast as _;

const DOC_EXTS: &[&str] = &["designcraft", "idml"];
const IMAGE_EXTS: &[&str] = &[
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "tif",
    "tiff",
    "bmp",
    "psd",
    "svg",
    "pdf",
    "ai",
    "txt",
    "docx",
    "rtf",
    "md",
    "xlsx",
    "idml",
    "designcraft",
    "ttf",
    "otf",
    "woff",
    "woff2",
];
const CANVAS_ID: &str = "designcraft_canvas";
const LOADING_ID: &str = "designcraft_loading";
static SAVE_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    if let Some(w) = web_sys::window() {
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(|| {
            SAVE_NOW.store(true, std::sync::atomic::Ordering::Relaxed);
        }) as Box<dyn Fn()>);
        let _ = js_sys::Reflect::set(&w, &wasm_bindgen::JsValue::from_str("designcraftSaveNow"), cb.as_ref().unchecked_ref());
        cb.forget();
    }
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let mut options = eframe::WebOptions::default();
        if query().contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }

        let q = query();
        let is_editor = get_query_param("mode").as_deref() == Some("editor")
            || get_query_param("editor").is_some_and(|v| v == "true" || v == "1" || v.is_empty())
            || q.contains("mode=editor")
            || q.split('&').any(|p| {
                let p = p.trim_start_matches('?');
                p == "editor" || p == "mode=editor" || p == "editor=true" || p == "editor=1"
            });

        let template_param = get_query_param("template");
        let doc_param = get_query_param("doc");
        let story_param = get_query_param("story").and_then(|s| s.parse::<u64>().ok());
        let article_param = get_query_param("article");
        let is_editor = is_editor || story_param.is_some() || article_param.is_some();

        let initial_doc_id = doc_param.as_deref().or(template_param.as_deref()).map(clean_doc_stem);
        let active_doc_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(initial_doc_id.clone()));

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new({
                    let active_doc_id = active_doc_id.clone();
                    move |cc| {
                        if let Some(rs) = &cc.wgpu_render_state {
                            log::info!("designcraft-web: wgpu backend {:?}", rs.adapter.get_info().backend);
                        }
                        let inbox: Inbox = Inbox::default();
                        let mut app = DesignApp::new(Session::new(), services(inbox.clone(), cc.egui_ctx.clone(), active_doc_id.clone()));
                        if is_editor {
                            app.session.restrict_to_editor();
                        }

                        let ctx_fonts = cc.egui_ctx.clone();
                        let inbox_fonts = inbox.clone();

                        if let Some(target_stem) = initial_doc_id {
                            let inbox_clone = inbox.clone();
                            let ctx_clone = cc.egui_ctx.clone();
                            let active_doc_clone = active_doc_id.clone();
                            let tmpl_raw = template_param.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                load_server_fonts(ctx_clone.clone(), inbox_clone.clone()).await;

                                // 1. Try to load saved document draft first
                                let enc_doc = js_sys::encode_uri_component(&target_stem).as_string().unwrap_or_else(|| target_stem.clone());
                                let doc_url = format!("/api/documents/{enc_doc}");
                                let mut loaded_bytes = None;
                                match fetch_api_bytes(&doc_url).await {
                                    Ok(Some(bytes)) => {
                                        log::info!("Loaded document draft `{target_stem}` from server ({} bytes)", bytes.len());
                                        loaded_bytes = Some(bytes);
                                    }
                                    Ok(None) => {
                                        log::info!("Document draft `{target_stem}` not found on server, looking for template...");
                                    }
                                    Err(e) => {
                                        log::warn!("Could not fetch draft `{target_stem}`: {e}");
                                    }
                                }

                                // 2. If no server draft, check template
                                if loaded_bytes.is_none() {
                                    let tmpl_id = tmpl_raw.as_deref().map(clean_doc_stem).unwrap_or_else(|| target_stem.clone());
                                    let enc_name = js_sys::encode_uri_component(&tmpl_id).as_string().unwrap_or_else(|| tmpl_id.clone());
                                    let t_url = format!("/api/templates/{enc_name}");
                                    match fetch_api_bytes(&t_url).await {
                                        Ok(Some(bytes)) => {
                                            log::info!("Loaded template `{tmpl_id}` as draft `{target_stem}` ({} bytes)", bytes.len());
                                            loaded_bytes = Some(bytes);
                                        }
                                        Ok(None) => {
                                            log::error!("Template `{tmpl_id}` not found on server");
                                        }
                                        Err(e) => {
                                            log::error!("Error fetching template `{tmpl_id}`: {e}");
                                        }
                                    }
                                }

                                // 3. Fallback to localStorage backup
                                if loaded_bytes.is_none() {
                                    if let Some(window) = web_sys::window() {
                                        if let Ok(Some(storage)) = window.local_storage() {
                                            if let Ok(Some(b64)) = storage.get_item(&format!("designcraft_backup:{target_stem}")) {
                                                let bytes = designcraft_engine::cmd::base64_decode(&b64);
                                                if !bytes.is_empty() {
                                                    log::info!("Restored draft `{target_stem}` from localStorage ({} bytes)", bytes.len());
                                                    loaded_bytes = Some(bytes);
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(bytes) = loaded_bytes {
                                    *active_doc_clone.lock().unwrap_or_else(|e| e.into_inner()) = Some(target_stem.clone());
                                    update_browser_url(&target_stem, is_editor);
                                    inbox_clone.lock().unwrap_or_else(|e| e.into_inner()).push((format!("{target_stem}.designcraft"), bytes));
                                    ctx_clone.request_repaint();
                                }
                            });
                        } else {
                            wasm_bindgen_futures::spawn_local(async move {
                                load_server_fonts(ctx_fonts, inbox_fonts).await;
                            });
                            if q.contains("sample") {
                                let _ = app.run("file.newSample", serde_json::json!({}));
                            }
                        }

                        let target_story = story_param;
                        let target_article = article_param.clone();
                        Ok(Box::new(WebShell {
                            app,
                            inbox,
                            active_doc_id,
                            target_story,
                            target_article,
                            last_seen_rev: 0,
                            last_change_time: 0.0,
                            is_saving: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                        }))
                    }
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>DesignCraft failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>")),
            }
        }
    });
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

fn get_query_param(key: &str) -> Option<String> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get(key)
}

async fn fetch_api_bytes(url: &str) -> Result<Option<Vec<u8>>, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_val = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await.map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| "not a Response".to_string())?;
    if resp.status() == 404 {
        return Ok(None);
    }
    if !resp.ok() {
        return Err(format!("HTTP error {}: {}", resp.status(), resp.status_text()));
    }
    let buf_promise = resp.array_buffer().map_err(|e| format!("{e:?}"))?;
    let buf_val = wasm_bindgen_futures::JsFuture::from(buf_promise).await.map_err(|e| format!("{e:?}"))?;
    let array = js_sys::Uint8Array::new(&buf_val);
    Ok(Some(array.to_vec()))
}

fn clean_doc_stem(id: &str) -> String {
    let s = id.trim();
    let s = std::path::Path::new(s).file_stem().and_then(|s| s.to_str()).unwrap_or(s);
    let mut s = s.strip_suffix(".designcraft").unwrap_or(s);
    s = s.strip_suffix(".idml").unwrap_or(s);
    s.trim_matches('/').to_string()
}

fn update_browser_url(doc_id: &str, is_editor: bool) {
    let clean = clean_doc_stem(doc_id);
    if clean.is_empty() {
        return;
    }

    if let Some(window) = web_sys::window() {
        let mode_str = if is_editor { "&mode=editor" } else { "" };
        let encoded = js_sys::encode_uri_component(&clean).as_string().unwrap_or(clean);
        let new_url = format!("/?doc={}{}", encoded, mode_str);
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&new_url));
        }
    }
}

async fn save_to_server(id: &str, bytes: &[u8]) -> Result<(), String> {
    let clean = clean_doc_stem(id);
    if clean.is_empty() {
        return Err("invalid document id".to_string());
    }

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;

    // Only backup to localStorage if the document is small (<= 512 KB) to prevent
    // freezing the browser UI and exceeding the browser's 5MB origin quota.
    if bytes.len() <= 512 * 1024 {
        if let Ok(Some(storage)) = window.local_storage() {
            let b64 = designcraft_engine::cmd::base64_encode(bytes);
            let _ = storage.set_item(&format!("designcraft_backup:{clean}"), &b64);
        }
    }

    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    let uint8 = js_sys::Uint8Array::from(bytes);
    opts.set_body(&uint8);
    let url = format!("/api/documents/{clean}");
    let request = web_sys::Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request.headers().set("Content-Type", "application/vnd.designcraft+zip").map_err(|e| format!("{e:?}"))?;
    let resp_val = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await.map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| "not a Response".to_string())?;
    if !resp.ok() {
        return Err(format!("server returned HTTP {}", resp.status()));
    }
    Ok(())
}

/// Wraps the app to read dropped files asynchronously (browsers can't read them synchronously)
/// and feed them through the inbox.
struct WebShell {
    app: DesignApp,
    inbox: Inbox,
    active_doc_id: Arc<Mutex<Option<String>>>,
    target_story: Option<u64>,
    target_article: Option<String>,
    last_seen_rev: u64,
    last_change_time: f64,
    is_saving: Arc<std::sync::atomic::AtomicBool>,
}

impl eframe::App for WebShell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            let inbox = self.inbox.clone();
            let ctx = ctx.clone();
            let active_doc_id = self.active_doc_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped".into());
                let lower = name.to_ascii_lowercase();
                let is_doc = lower.ends_with(".designcraft") || lower.ends_with(".idml");
                match f.bytes_async().await {
                    Ok(bytes) => {
                        if is_doc {
                            let stem = clean_doc_stem(&name);
                            if !stem.is_empty() {
                                *active_doc_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(stem);
                            }
                        }
                        inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                        ctx.request_repaint();
                    }
                    Err(e) => log::error!("couldn't read dropped file {name}: {e}"),
                }
            });
        }
        self.app.logic(ctx);

        if (self.target_story.is_some() || self.target_article.is_some())
            && let Some(st) = self.app.session.active()
        {
            if let Some(sid) = self.target_story.take() {
                if st.doc.story(designcraft_doc::StoryId(sid)).is_some() {
                    self.app.story_editor = Some(designcraft_doc::StoryId(sid));
                }
            } else if let Some(ref art_name) = self.target_article.take() {
                let mut matched = None;
                if let Some(art) = st.doc.articles.iter().find(|a| a.name.eq_ignore_ascii_case(art_name)) {
                    for &id in &art.items {
                        if let Some(it) = st.doc.item(id)
                            && let designcraft_doc::Content::Text(tf) = &it.content
                        {
                            matched = Some(tf.story);
                            break;
                        }
                    }
                }
                if matched.is_none() {
                    for sp in &st.doc.spreads {
                        for it in &sp.items {
                            if it.name.eq_ignore_ascii_case(art_name) || it.label.eq_ignore_ascii_case(art_name) {
                                if let designcraft_doc::Content::Text(tf) = &it.content {
                                    matched = Some(tf.story);
                                    break;
                                }
                            }
                        }
                        if matched.is_some() {
                            break;
                        }
                    }
                }
                if let Some(sid) = matched {
                    self.app.story_editor = Some(sid);
                }
            }
        }

        // Auto-save check: if document has unsaved modifications and user has stopped typing for 2.5 seconds
        if let Some(st) = self.app.session.active() {
            let current_rev = st.revision;
            let saved_rev = st.saved_revision;

            if current_rev != self.last_seen_rev {
                self.last_seen_rev = current_rev;
                self.last_change_time = web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0);
            }

            let force_save = SAVE_NOW.swap(false, std::sync::atomic::Ordering::Relaxed);
            if current_rev != saved_rev || force_save {
                let now = web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0);
                let elapsed = now - self.last_change_time;

                if (force_save || elapsed > 2500.0) && !self.is_saving.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Ok(bytes) = designcraft_format::save(&st.doc) {
                        let current_id = self.active_doc_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        let doc_id = current_id.filter(|id| !id.trim().is_empty()).unwrap_or_else(|| {
                            let title = clean_doc_stem(&st.doc.title);
                            if !title.is_empty() && title != "Untitled" && title != "Sin título" { title } else { "document".to_string() }
                        });
                        let is_saving = self.is_saving.clone();
                        is_saving.store(true, std::sync::atomic::Ordering::Relaxed);

                        wasm_bindgen_futures::spawn_local(async move {
                            match save_to_server(&doc_id, &bytes).await {
                                Ok(()) => {
                                    log::info!("Auto-saved `{doc_id}` ({} bytes)", bytes.len());
                                }
                                Err(e) => {
                                    log::warn!("Auto-save failed for `{doc_id}`: {e}");
                                }
                            }
                            is_saving.store(false, std::sync::atomic::Ordering::Relaxed);
                        });

                        if let Some(active_st) = self.app.session.active_mut() {
                            active_st.saved_revision = current_rev;
                        }
                    }
                } else if elapsed <= 2500.0 {
                    let remaining = (2500.0 - elapsed).max(100.0) as u64;
                    ctx.request_repaint_after(std::time::Duration::from_millis(remaining));
                }
            }
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }
}

fn services(inbox: Inbox, ctx: egui::Context, active_doc_id: Arc<Mutex<Option<String>>>) -> Services {
    let open_inbox = inbox.clone();
    let open_ctx = ctx.clone();
    let open_active_doc_id = active_doc_id.clone();

    let tmpl_inbox = inbox.clone();
    let tmpl_ctx = ctx.clone();
    let tmpl_active_doc_id = active_doc_id.clone();

    Services {
        open_async: Some(Box::new(move |purpose: &str| {
            let inbox = open_inbox.clone();
            let ctx = open_ctx.clone();
            let active_doc_id = open_active_doc_id.clone();
            let is_bullet = purpose == "bullet";
            let is_styles = purpose == "styles";
            let is_place = purpose == "place";
            let dialog = if purpose == "swatches" {
                rfd::AsyncFileDialog::new().add_filter("Swatch Exchange (ASE)", &["ase"])
            } else if is_bullet {
                rfd::AsyncFileDialog::new().add_filter("Graphics, SVG and PDF", &["svg", "pdf", "png", "jpg", "jpeg", "webp", "gif"])
            } else if is_styles {
                rfd::AsyncFileDialog::new().add_filter("DesignCraft or IDML", DOC_EXTS)
            } else if is_place {
                rfd::AsyncFileDialog::new().add_filter("Graphics", IMAGE_EXTS)
            } else {
                rfd::AsyncFileDialog::new().add_filter("DesignCraft", DOC_EXTS)
            };
            wasm_bindgen_futures::spawn_local(async move {
                let Some(file) = dialog.pick_file().await else {
                    return;
                };
                let raw_name = file.file_name();
                let bytes = file.read().await;
                if is_bullet {
                    inbox.lock().unwrap_or_else(|e| e.into_inner()).push((format!("bullet:{raw_name}"), bytes));
                } else if is_styles {
                    inbox.lock().unwrap_or_else(|e| e.into_inner()).push((format!("styles:{raw_name}"), bytes));
                } else {
                    let lower = raw_name.to_ascii_lowercase();
                    let is_doc = !is_place && (lower.ends_with(".designcraft") || lower.ends_with(".idml"));
                    if is_doc {
                        let stem = clean_doc_stem(&raw_name);
                        if !stem.is_empty() {
                            *active_doc_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(stem);
                        }
                    }
                    inbox.lock().unwrap_or_else(|e| e.into_inner()).push((raw_name, bytes));
                }
                ctx.request_repaint();
            });
        })),
        // Exports ask for a name; the browser decides where the download goes.
        pick_save: Some(Box::new(|name: &str| Some(name.to_string()))),
        write: Some(Box::new(|name: &str, bytes: &[u8]| download(name, bytes))),
        download: Some(Box::new({
            let active_doc_id = active_doc_id.clone();
            move |name: &str, bytes: &[u8]| {
                if !name.ends_with(".designcraft") {
                    if let Err(e) = download(name, bytes) {
                        log::error!("download of {name} failed: {e}");
                    }
                    return;
                }

                let clean = clean_doc_stem(name);
                let current_id = active_doc_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let target_id = if !clean.is_empty() && clean != "Untitled" && clean != "Sin título" {
                    clean
                } else {
                    current_id.filter(|id| !id.trim().is_empty()).unwrap_or_else(|| "document".to_string())
                };
                *active_doc_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(target_id.clone());

                let bytes_vec = bytes.to_vec();
                let name_str = name.to_string();
                let id = target_id;
                wasm_bindgen_futures::spawn_local(async move {
                    match save_to_server(&id, &bytes_vec).await {
                        Ok(()) => {
                            log::info!("Successfully saved `{id}` to server ({} bytes)", bytes_vec.len());
                            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                let old_title = doc.title();
                                doc.set_title(&format!("✓ Guardado `{id}`"));
                                let doc_clone = doc.clone();
                                let timer = wasm_bindgen::closure::Closure::once_into_js(move || {
                                    doc_clone.set_title(&old_title);
                                });
                                if let Some(w) = web_sys::window() {
                                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(timer.unchecked_ref(), 2500);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to save `{id}` to server: {e}. Falling back to browser download.");
                            if let Err(dl_err) = download(&name_str, &bytes_vec) {
                                log::error!("Download fallback failed: {dl_err}");
                            }
                        }
                    }
                });
            }
        })),
        inbox: Some(inbox),
        open_template: Some(Box::new({
            move |template_id: &str| {
                let inbox = tmpl_inbox.clone();
                let ctx = tmpl_ctx.clone();
                let active_doc_id = tmpl_active_doc_id.clone();
                let enc_id = js_sys::encode_uri_component(template_id).as_string().unwrap_or_else(|| template_id.to_string());
                let t_url = format!("/api/templates/{enc_id}");
                let t_name = template_id.to_string();
                wasm_bindgen_futures::spawn_local(async move {
                    load_server_fonts(ctx.clone(), inbox.clone()).await;
                    match fetch_api_bytes(&t_url).await {
                        Ok(Some(bytes)) => {
                            log::info!("Template `{t_name}` loaded ({} bytes)", bytes.len());
                            *active_doc_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(t_name.clone());
                            inbox.lock().unwrap_or_else(|e| e.into_inner()).push((format!("{t_name}.designcraft"), bytes));
                            ctx.request_repaint();
                        }
                        Ok(None) => log::error!("Template `{t_name}` not found on server"),
                        Err(e) => log::error!("Failed to fetch template `{t_name}`: {e}"),
                    }
                });
            }
        })),
        ..Default::default()
    }
}

/// Trigger a browser download of `bytes` named after the last component of `path`.
fn download(path: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "designcraft".into());
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime_for(&name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(&name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    // Revoke after the click has been dispatched; the download keeps its own reference.
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        web_sys::Url::revoke_object_url(&url).ok();
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
    Ok(())
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("designcraft") => "application/vnd.designcraft+zip",
        Some("idml") => "application/vnd.adobe.indesign-idml-package",
        _ => "application/octet-stream",
    }
}

async fn load_server_fonts(ctx: egui::Context, inbox: Inbox) {
    let Ok(Some(bytes)) = fetch_api_bytes("/api/fonts").await else {
        return;
    };
    let Ok(items) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) else {
        return;
    };
    if items.is_empty() {
        return;
    }

    let array = js_sys::Array::new();
    let loaded = Arc::new(Mutex::new(Vec::new()));

    for item in items {
        let Some(url) = item.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()) else { continue };
        let filename = item.get("filename").and_then(|v| v.as_str()).unwrap_or(&url).to_string();
        let loaded_clone = loaded.clone();
        let promise = wasm_bindgen_futures::future_to_promise(async move {
            match fetch_api_bytes(&url).await {
                Ok(Some(font_bytes)) => {
                    let count = designcraft_fonts::FontDb::global().add_font(font_bytes.clone());
                    log::info!("Server font loaded: `{filename}` ({count} face(s))");
                    if count > 0 {
                        loaded_clone.lock().unwrap_or_else(|e| e.into_inner()).push((filename, font_bytes));
                    }
                }
                Ok(None) => log::warn!("Server font `{url}` returned 404"),
                Err(e) => log::warn!("Failed to fetch font `{url}`: {e}"),
            }
            Ok(wasm_bindgen::JsValue::UNDEFINED)
        });
        array.push(&promise);
    }

    let all_promise = js_sys::Promise::all(&array);
    let _ = wasm_bindgen_futures::JsFuture::from(all_promise).await;

    let font_list = std::mem::take(&mut *loaded.lock().unwrap_or_else(|e| e.into_inner()));
    if !font_list.is_empty() {
        // Register in egui FontDefinitions so Story Editor and egui components can use the fonts
        let mut defs = ctx.fonts(|f| f.definitions().clone());
        for (filename, font_bytes) in &font_list {
            let stem = filename
                .strip_suffix(".otf")
                .or_else(|| filename.strip_suffix(".ttf"))
                .or_else(|| filename.strip_suffix(".woff2"))
                .or_else(|| filename.strip_suffix(".woff"))
                .unwrap_or(filename);

            let family = if stem.starts_with("Playfair") {
                "Playfair Display"
            } else if stem.starts_with("Myriad") {
                "Myriad Pro"
            } else if stem.starts_with("Utopia") {
                "Utopia"
            } else if stem.to_ascii_lowercase().starts_with("klavika") {
                "Klavika"
            } else if stem.starts_with("Vitesse") {
                "Vitesse"
            } else if stem.starts_with("Zapf") {
                "ZapfDingbats BT"
            } else {
                stem
            };

            defs.font_data.insert(stem.to_string(), Arc::new(egui::FontData::from_owned(font_bytes.clone())));
            defs.families.entry(egui::FontFamily::Name(family.into())).or_default().push(stem.to_string());
            defs.families.entry(egui::FontFamily::Name(stem.to_string().into())).or_default().push(stem.to_string());
            if let Some(prop) = defs.families.get_mut(&egui::FontFamily::Proportional) {
                prop.push(stem.to_string());
            }
        }
        ctx.set_fonts(defs);

        // Tell inbox to clear session compose cache and request repaint
        inbox.lock().unwrap_or_else(|e| e.into_inner()).push(("server_fonts.otf".to_string(), Vec::new()));
        ctx.request_repaint();
    }
}
