//! The agent-facing command line: scripts with references, describe, filtered command lists.

use std::io::Write;
use std::process::{Command, Stdio};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_designcraft-cli"))
}

fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[test]
fn script_from_stdin_with_references_saves_and_exports() {
    let dir = std::env::temp_dir().join(format!("dc-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (doc, png) = (dir.join("a.designcraft"), dir.join("a.png"));
    let mut child = cli()
        .args(["script", "-", "--save", doc.to_str().unwrap(), "--export", png.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"# comment\nframe.create {\"rect\": [72, 72, 300, 200], \"content\": \"text\", \"text\": \"Hi\"}\nstory.get {\"story\": \"$0.story\"}\n",
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["completed"], 2);
    assert_eq!(v["results"][1]["text"], "Hi");
    assert!(doc.exists() && png.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failing_script_exits_nonzero_with_the_step() {
    let mut child = cli().args(["script", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"document.inspect\nno.such.command\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["failedIndex"], 1);
}

#[test]
fn run_cmd_references_and_describe() {
    let out = cli()
        .args([
            "run",
            "--cmd",
            r#"frame.create={"rect":[72,72,300,200],"content":"text","text":"Hello"}"#,
            "--cmd",
            r#"story.get={"story":"$0.story"}"#,
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"text\":\"Hello\""));
    let out = cli().args(["describe", "footnote.insert"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["label"], "Insert Footnote");
    let out = cli().args(["commands", "footnote."]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v.as_array().unwrap().iter().all(|c| c.to_string().contains("footnote")));
}

/// `designcraft-cli commands | head -1` panicked with "failed printing to
/// stdout: Broken pipe (os error 32)" and exit status 101.
#[test]
fn closed_stdout_ends_quietly() {
    use std::io::Read;
    let mut child = cli().arg("commands").stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut first = [0u8; 16];
    child.stdout.as_mut().unwrap().read_exact(&mut first).unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !err.contains("panicked"), "{:?}: {err}", out.status);
}

#[test]
fn serve_endpoints_work() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = std::env::temp_dir().join(format!("dc-serve-{}", std::process::id()));
    let t_dir = dir.join("templates");
    let d_dir = dir.join("docs");
    let w_dir = dir.join("dist");
    let f_dir = dir.join("fonts");
    std::fs::create_dir_all(&t_dir).unwrap();
    std::fs::create_dir_all(&d_dir).unwrap();
    std::fs::create_dir_all(&w_dir).unwrap();
    std::fs::create_dir_all(&f_dir).unwrap();

    std::fs::write(t_dir.join("test_tmpl.designcraft"), b"TEST_TEMPLATE_CONTENT").unwrap();
    std::fs::write(w_dir.join("index.html"), b"<html><body>OK</body></html>").unwrap();
    std::fs::write(f_dir.join("MockFont.ttf"), b"MOCK_FONT_BYTES_12345").unwrap();

    let port = pick_free_port();
    let mut child = cli()
        .args([
            "serve",
            "--port",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--templates",
            t_dir.to_str().unwrap(),
            "--docs",
            d_dir.to_str().unwrap(),
            "--fonts",
            f_dir.to_str().unwrap(),
            "--dist",
            w_dir.to_str().unwrap(),
        ])
        .spawn()
        .unwrap();

    // Wait briefly for server to bind
    std::thread::sleep(std::time::Duration::from_millis(150));

    // 1. Health check
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("\"status\":\"ok\""));

    // 2. Templates list
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/templates HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("test_tmpl"));

    // 3. Document save (POST)
    let body = b"SAVED_DOCUMENT_CONTENT";
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_req = format!("POST /api/documents/draft_one HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(post_req.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("\"ok\":true"));

    // Verify written file
    let saved = d_dir.join("draft_one.designcraft");
    assert!(saved.is_file());
    assert_eq!(std::fs::read(saved).unwrap(), body);

    // 4. Fonts list (GET /api/fonts)
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/fonts HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    assert!(resp.contains("MockFont.ttf"));

    // 5. Font binary delivery (GET /api/fonts/MockFont.ttf)
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/fonts/MockFont.ttf HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(resp_str.contains("200 OK"));
    assert!(resp_str.contains("font/ttf"));
    assert!(resp_str.contains("Cache-Control: public, max-age=31536000, immutable"));
    assert!(resp.ends_with(b"MOCK_FONT_BYTES_12345"));

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_story_article_endpoints_and_atomic_save() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = std::env::temp_dir().join(format!("dc-serve-stories-{}", std::process::id()));
    let t_dir = dir.join("templates");
    let d_dir = dir.join("docs");
    let w_dir = dir.join("dist");
    let f_dir = dir.join("fonts");
    std::fs::create_dir_all(&t_dir).unwrap();
    std::fs::create_dir_all(&d_dir).unwrap();
    std::fs::create_dir_all(&w_dir).unwrap();
    std::fs::create_dir_all(&f_dir).unwrap();

    // Create a real initial .designcraft document in docs/
    let mut s = designcraft_engine::Session::new();
    s.execute("file.new", &serde_json::json!({})).unwrap();
    s.execute("frame.create", &serde_json::json!({"rect": [72, 72, 300, 150], "content": "text", "name": "Titular", "text": "Texto Original"}))
        .unwrap();
    s.execute("frame.create", &serde_json::json!({"rect": [72, 200, 300, 400], "content": "text", "name": "Cuerpo", "text": "Cuerpo Original"}))
        .unwrap();
    let doc_bytes = designcraft_format::save(&s.doc().unwrap().doc).unwrap();
    std::fs::write(d_dir.join("noticia.designcraft"), &doc_bytes).unwrap();

    let port = pick_free_port();
    let mut child = cli()
        .args([
            "serve",
            "--port",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--templates",
            t_dir.to_str().unwrap(),
            "--docs",
            d_dir.to_str().unwrap(),
            "--fonts",
            f_dir.to_str().unwrap(),
            "--dist",
            w_dir.to_str().unwrap(),
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(200));

    // Helper to extract JSON body from HTTP response string
    fn parse_http_json(resp: &str) -> serde_json::Value {
        let body = resp.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(resp);
        serde_json::from_str(body).unwrap_or_default()
    }

    // 1. GET /api/documents/noticia/stories
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/documents/noticia/stories HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"), "{resp}");
    let stories_json = parse_http_json(&resp);
    let stories = stories_json.as_array().expect("stories array");
    assert_eq!(stories.len(), 2);
    let titular_sid = stories.iter().find(|s| s["name"] == "Titular").unwrap()["id"].as_u64().unwrap();

    // 2. GET /api/documents/noticia/stories/:sid
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let get_req = format!("GET /api/documents/noticia/stories/{titular_sid} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stream.write_all(get_req.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let story_data = parse_http_json(&resp);
    assert_eq!(story_data["text"], "Texto Original");
    assert_eq!(story_data["name"], "Titular");
    assert_eq!(story_data["copyfit"]["words"], 2);

    // 3. POST /api/documents/noticia/stories/:sid (Atomic update of story text)
    let body = serde_json::json!({"text": "Acuerdo Historico en el Congreso"}).to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_req = format!(
        "POST /api/documents/noticia/stories/{titular_sid} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(post_req.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let upd_res = parse_http_json(&resp);
    assert_eq!(upd_res["ok"], true);
    assert_eq!(upd_res["copyfit"]["words"], 5);

    // 4. Verify that GET /api/documents/noticia/stories/:sid reflects the updated text
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let get_req = format!("GET /api/documents/noticia/stories/{titular_sid} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stream.write_all(get_req.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    let story_data2 = parse_http_json(&resp);
    assert_eq!(story_data2["text"], "Acuerdo Historico en el Congreso");

    // 5. POST /api/documents/noticia/inject (Structured field injection)
    let inj_body = serde_json::json!({
        "fields": {
            "Cuerpo": "Tras 14 horas de debate se aprueba la reforma."
        }
    })
    .to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_inj = format!(
        "POST /api/documents/noticia/inject HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        inj_body.len(),
        inj_body
    );
    stream.write_all(post_inj.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let inj_res = parse_http_json(&resp);
    assert_eq!(inj_res["ok"], true);
    let updated = inj_res["updated"].as_array().unwrap();
    assert!(updated.iter().any(|u| u == "Cuerpo"));

    // 6. GET /api/documents/noticia/copyfit
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/documents/noticia/copyfit HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let copyfit_res = parse_http_json(&resp);
    assert_eq!(copyfit_res["hasOverset"], false);
    assert_eq!(copyfit_res["totalOverflowLines"], 0);

    // 7. GET /api/documents/noticia/pdf
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/documents/noticia/pdf HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes).unwrap();
    let resp_str = String::from_utf8_lossy(&resp_bytes);
    assert!(resp_str.starts_with("HTTP/1.1 200 OK"), "{resp_str}");
    assert!(resp_str.contains("application/pdf"));
    assert!(resp_str.contains("attachment; filename=\"noticia.pdf\""));
    let body_offset = resp_bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let pdf_bytes = &resp_bytes[body_offset..];
    assert!(pdf_bytes.starts_with(b"%PDF-"), "invalid PDF header magic");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_concurrency_lease_and_heartbeat() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = std::env::temp_dir().join(format!("dc-serve-lease-{}", std::process::id()));
    let t_dir = dir.join("templates");
    let d_dir = dir.join("docs");
    let w_dir = dir.join("dist");
    let f_dir = dir.join("fonts");
    std::fs::create_dir_all(&t_dir).unwrap();
    std::fs::create_dir_all(&d_dir).unwrap();
    std::fs::create_dir_all(&w_dir).unwrap();
    std::fs::create_dir_all(&f_dir).unwrap();

    let port = pick_free_port();
    let mut child = cli()
        .args([
            "serve",
            "--port",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--templates",
            t_dir.to_str().unwrap(),
            "--docs",
            d_dir.to_str().unwrap(),
            "--fonts",
            f_dir.to_str().unwrap(),
            "--dist",
            w_dir.to_str().unwrap(),
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(200));

    fn parse_http_json(resp: &str) -> serde_json::Value {
        let body = resp.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(resp);
        serde_json::from_str(body).unwrap_or_default()
    }

    // 1. Check initial lease is not locked
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/documents/articulo_uno/lease HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let lease_json = parse_http_json(&resp);
    assert_eq!(lease_json["isLocked"], false);

    // 2. User A acquires lease
    let body_a = serde_json::json!({
        "holder": "user_a",
        "holderName": "Ana Redactora",
        "mode": "editor"
    })
    .to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_req = format!(
        "POST /api/documents/articulo_uno/lease HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body_a.len(),
        body_a
    );
    stream.write_all(post_req.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let acq_json = parse_http_json(&resp);
    assert_eq!(acq_json["status"], "acquired");

    // 3. Verify GET /lease shows locked by User A
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /api/documents/articulo_uno/lease HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let status_json = parse_http_json(&resp);
    assert_eq!(status_json["isLocked"], true);
    assert_eq!(status_json["holder"], "user_a");
    assert_eq!(status_json["holderName"], "Ana Redactora");

    // 4. User B attempts to acquire lease -> 423 Locked
    let body_b = serde_json::json!({
        "holder": "user_b",
        "holderName": "Carlos Maquetador",
        "mode": "layout"
    })
    .to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_req_b = format!(
        "POST /api/documents/articulo_uno/lease HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body_b.len(),
        body_b
    );
    stream.write_all(post_req_b.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("423 Locked"), "expected 423 Locked, got: {resp}");
    let locked_res = parse_http_json(&resp);
    assert_eq!(locked_res["status"], "locked");
    assert_eq!(locked_res["holder"], "user_a");

    // 5. User A sends heartbeat
    let hb_body = serde_json::json!({"holder": "user_a"}).to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_hb = format!(
        "POST /api/documents/articulo_uno/heartbeat HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        hb_body.len(),
        hb_body
    );
    stream.write_all(post_hb.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let hb_res = parse_http_json(&resp);
    assert_eq!(hb_res["status"], "renewed");

    // 6. Save document V1 and V2, verifying automatic recovery snapshot in .backups/
    let v1_bytes = b"DOCUMENT_VERSION_ONE_CONTENT";
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_v1 = format!("POST /api/documents/articulo_uno HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", v1_bytes.len());
    stream.write_all(post_v1.as_bytes()).unwrap();
    stream.write_all(v1_bytes).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));

    let v2_bytes = b"DOCUMENT_VERSION_TWO_CONTENT";
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_v2 = format!("POST /api/documents/articulo_uno HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", v2_bytes.len());
    stream.write_all(post_v2.as_bytes()).unwrap();
    stream.write_all(v2_bytes).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));

    // Verify .backups/ holds the snapshot of V1
    let backups_dir = d_dir.join(".backups");
    assert!(backups_dir.is_dir());
    let mut backup_entries: Vec<_> =
        std::fs::read_dir(&backups_dir).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|ext| ext == "designcraft")).collect();
    assert!(!backup_entries.is_empty(), "expected backup snapshot in .backups");
    backup_entries.sort_by_key(|e| e.path());
    let backup_data = std::fs::read(backup_entries.first().unwrap().path()).unwrap();
    assert_eq!(backup_data, v1_bytes);

    // 7. User A releases lease
    let rel_body = serde_json::json!({"holder": "user_a"}).to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_rel = format!(
        "POST /api/documents/articulo_uno/release HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        rel_body.len(),
        rel_body
    );
    stream.write_all(post_rel.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let rel_res = parse_http_json(&resp);
    assert_eq!(rel_res["status"], "released");

    // 8. User B can now acquire lease successfully
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let post_req_b2 = format!(
        "POST /api/documents/articulo_uno/lease HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body_b.len(),
        body_b
    );
    stream.write_all(post_req_b2.as_bytes()).unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"));
    let b_acq = parse_http_json(&resp);
    assert_eq!(b_acq["status"], "acquired");

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
