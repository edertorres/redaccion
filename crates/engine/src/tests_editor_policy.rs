//! Editorial workflow regressions: all calls use the same engine entry point as UI/CLI/MCP.
use serde_json::{Value, json};

use crate::{Session, cmd};
use designcraft_doc::{Content, ItemId, StoryId};

fn setup() -> (Session, Value, Value) {
    let mut s = Session::new();
    s.execute("file.new", &json!({})).unwrap();
    let text = s.execute("frame.create", &json!({"rect": [36, 36, 240, 200], "content": "text", "text": "Texto editorial"})).unwrap();
    let photo = s.execute("frame.create", &json!({"rect": [250, 36, 450, 200], "content": "unassigned"})).unwrap();
    (s, text, photo)
}

fn image() -> String {
    cmd::base64_encode(&designcraft_render::Rendered { width: 2, height: 2, pixels: vec![150; 16] }.to_png())
}

fn unchanged_after_rejection(s: &mut Session, id: &str, p: Value) {
    let st = s.active().unwrap();
    let doc = st.doc.clone();
    let sel = st.selection.clone();
    let counts = (st.history.undo.len(), st.history.redo.len(), st.revision);
    assert!(s.execute(id, &p).is_err(), "{id} unexpectedly succeeded");
    let st = s.active().unwrap();
    assert_eq!(*st.doc, *doc, "{id} changed the document");
    assert_eq!(st.selection, sel, "{id} changed selection");
    assert_eq!((st.history.undo.len(), st.history.redo.len(), st.revision), counts, "{id} changed history/revision");
}

#[test]
fn editor_content_selection_cannot_bypass_structure_or_styles() {
    let (mut s, text, photo) = setup();
    s.execute("selection.set", &json!({"ids": [photo["id"]], "content": true})).unwrap();
    s.editor_mode = true;
    for (id, params) in [
        ("frame.create", json!({"rect": [0,0,50,50]})),
        ("object.delete", json!({"ids": [text["id"]]})),
        ("object.unlockAll", json!({})),
        ("object.setFlags", json!({"ids": [photo["id"]], "locked": false})),
        ("layout.pages.insert", json!({"after":0,"count":1})),
        ("style.paragraph.create", json!({"name":"Forbidden"})),
        ("story.update", json!({"story":text["story"],"text":"Partial edit", "paragraphStyle":"Other"})),
    ] {
        unchanged_after_rejection(&mut s, id, params);
    }
    assert!(!s.commands().iter().find(|c| c.id == "style.paragraph.create").unwrap().enabled);
    assert!(!s.commands().iter().find(|c| c.id == "frame.create").unwrap().enabled);
}

#[test]
fn editorial_photos_can_be_placed_replaced_and_cropped_without_moving_the_frame() {
    let (mut s, _, photo) = setup();
    let id = ItemId(photo["id"].as_u64().unwrap());
    s.execute("object.setFlags", &json!({"ids":[id.0],"locked":true})).unwrap();
    let original = s.active().unwrap().doc.item(id).unwrap().clone();
    s.editor_mode = true;
    s.execute("file.place", &json!({"frame":id.0,"base64":image(),"name":"foto.png"})).unwrap();
    s.execute("file.place", &json!({"frame":id.0,"base64":image(),"name":"sustituta.png"})).unwrap();
    s.execute("selection.set", &json!({"ids":[id.0],"content":true})).unwrap();
    s.execute("transform.move", &json!({"dx":3,"dy":2})).unwrap();
    s.execute("object.fit", &json!({"mode":"fitProportionally"})).unwrap();
    let placed = s.active().unwrap().doc.item(id).unwrap();
    assert_eq!(placed.xf, original.xf);
    assert_eq!(placed.path, original.path);
    assert!(placed.layout_locked);
    unchanged_after_rejection(&mut s, "transform.move", json!({"dx":5,"dy":0,"copy":true}));
    unchanged_after_rejection(&mut s, "file.place", json!({"frame":999999,"base64":image(),"name":"foto.png"}));
    s.execute("selection.set", &json!({"ids":[]})).unwrap();
    unchanged_after_rejection(&mut s, "file.place", json!({"base64":image(),"name":"foto.png"}));
}

#[test]
fn content_protection_blocks_text_images_shared_assets_and_unlocking_atomically() {
    let (mut s, text, photo) = setup();
    s.execute("file.place", &json!({"frame":photo["id"],"base64":image(),"name":"pauta.png"})).unwrap();
    let photo_id = ItemId(photo["id"].as_u64().unwrap());
    let aid = match &s.active().unwrap().doc.item(photo_id).unwrap().content {
        Content::Graphic(g) => g.asset,
        _ => panic!("graphic"),
    };
    s.execute("object.setFlags", &json!({"ids":[text["id"],photo["id"]],"contentLocked":true})).unwrap();
    let bytes = cmd::file_bytes(&s.active().unwrap().doc);
    let loaded = cmd::file_from(&bytes).unwrap();
    assert!(loaded.item(photo_id).unwrap().content_locked);
    s.editor_mode = true;
    unchanged_after_rejection(&mut s, "story.update", json!({"story":text["story"],"text":"Changed ad"}));
    unchanged_after_rejection(&mut s, "file.place", json!({"frame":photo["id"],"base64":image(),"name":"otra.png"}));
    s.execute("selection.set", &json!({"ids":[photo["id"]],"content":true})).unwrap();
    unchanged_after_rejection(&mut s, "transform.move", json!({"dx":5,"dy":0}));
    unchanged_after_rejection(&mut s, "object.setFlags", json!({"ids":[photo["id"]],"contentLocked":false}));
    let before = s.active().unwrap().doc.clone();
    assert!(
        s.edit(|d, _| {
            std::sync::Arc::make_mut(d.assets.get_mut(&aid).unwrap()).name = "Shared asset modified".into();
            Ok(())
        })
        .is_err()
    );
    assert_eq!(*s.active().unwrap().doc, *before);
}

#[test]
fn text_editing_paste_and_undo_preserve_template_and_designer_history() {
    let (mut s, text, _) = setup();
    let sid = StoryId(text["story"].as_u64().unwrap());
    s.execute("text.select", &json!({"story":sid.0,"anchor":0})).unwrap();
    let history = s.active().unwrap().history.undo.len();
    s.editor_mode = true;
    unchanged_after_rejection(&mut s, "edit.undo", json!({}));
    s.execute("text.insert", &json!({"text":"Última hora: "})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    unchanged_after_rejection(&mut s, "edit.undo", json!({}));
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("edit.paste", &json!({"text":"漢字 🙂"})).unwrap();
    assert!(s.active().unwrap().doc.story(sid).unwrap().text.contains("漢字 🙂"));
    assert_eq!(s.active().unwrap().history.undo.len(), history + 2);
    s.editor_mode = false;
    s.execute("style.paragraph.create", &json!({"name":"Designer still works"})).unwrap();
}

#[test]
fn protected_group_or_one_frame_of_a_thread_protects_the_entire_story() {
    let (mut s, text, photo) = setup();
    let last = s.execute("text.thread", &json!({"from":text["id"],"rect":[36,220,240,400]})).unwrap();
    s.execute("object.setFlags", &json!({"ids":[last["to"]],"contentLocked":true})).unwrap();
    s.execute("selection.set", &json!({"ids":[text["id"],photo["id"]]})).unwrap();
    let group = s.execute("object.group", &json!({})).unwrap();
    // Protect the group independently of its individual children.
    s.execute("object.setFlags", &json!({"ids":[group["id"]],"contentLocked":true})).unwrap();
    s.editor_mode = true;
    unchanged_after_rejection(&mut s, "story.setText", json!({"story":text["story"],"text":"Cannot change protected group/thread"}));
    unchanged_after_rejection(&mut s, "file.place", json!({"frame":photo["id"],"base64":image(),"name":"foto.png"}));
}

#[test]
fn copyfit_requires_explicit_limits_and_checks_every_paragraph() {
    let (mut s, text, _) = setup();
    let sid = text["story"].as_u64().unwrap();
    s.execute("text.select", &json!({"story":sid,"anchor":0,"focus":14})).unwrap();
    s.editor_mode = true;
    unchanged_after_rejection(&mut s, "type.char", json!({"attrs":{"size":12}}));
    unchanged_after_rejection(&mut s, "type.char", json!({"attrs":{"fontFamily":"Other"}}));
    unchanged_after_rejection(&mut s, "type.char", json!({"attrs":{"fill":null}}));
    s.editor_mode = false;
    s.execute("story.update", &json!({"story":sid,"text":"Uno\nDos"})).unwrap();
    s.execute("style.paragraph.create", &json!({"name":"Fit"})).unwrap();
    s.execute("style.paragraph.edit", &json!({"name":"Fit","copyfit":{"size_min":10,"size_max":14}})).unwrap();
    s.execute("text.select", &json!({"story":sid,"anchor":0,"focus":3})).unwrap();
    s.execute("style.paragraph.apply", &json!({"name":"Fit"})).unwrap();
    s.editor_mode = true;
    s.execute("type.char", &json!({"attrs":{"size":11}})).unwrap();
    unchanged_after_rejection(&mut s, "type.char", json!({"attrs":{"tracking":-5}}));
    s.execute("text.select", &json!({"story":sid,"anchor":0,"focus":7})).unwrap();
    unchanged_after_rejection(&mut s, "type.char", json!({"attrs":{"size":11}}));
}

#[test]
fn locked_layers_are_read_only_but_view_rotation_and_copyfit_still_work() {
    let (mut s, text, _) = setup();
    let sid = StoryId(text["story"].as_u64().unwrap());
    let id = ItemId(text["id"].as_u64().unwrap());
    let layer = s.active().unwrap().doc.item(id).unwrap().layer;
    s.edit(|d, _| {
        d.layers.iter_mut().find(|l| l.id == layer).unwrap().locked = true;
        Ok(())
    })
    .unwrap();
    s.editor_mode = true;
    assert!(!s.editorial_story_editable(sid));
    unchanged_after_rejection(&mut s, "story.update", json!({"story":sid.0,"text":"No"}));
    let original = s.active().unwrap().doc.page(0).unwrap().bounds();
    s.execute("layout.rotateSpreadView", &json!({"angle":90})).unwrap();
    assert_eq!(s.active().unwrap().doc.page(0).unwrap().bounds(), original);
    s.execute("document.copyfit", &json!({})).unwrap();
}

#[test]
fn gesture_commit_can_be_undone_without_crossing_the_designer_boundary() {
    let (mut s, _, photo) = setup();
    s.execute("file.place", &json!({"frame":photo["id"],"base64":image(),"name":"foto.png"})).unwrap();
    s.execute("selection.set", &json!({"ids":[photo["id"]],"content":true})).unwrap();
    let original = s.active().unwrap().doc.clone();
    s.editor_mode = true;
    s.begin_interaction("Crop photograph").unwrap();
    s.preview("transform.move", json!({"dx":5,"dy":2})).unwrap();
    s.commit_interaction().unwrap();
    assert_ne!(*s.active().unwrap().doc, *original);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(*s.active().unwrap().doc, *original);
    unchanged_after_rejection(&mut s, "edit.undo", json!({}));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_ne!(*s.active().unwrap().doc, *original);
}

#[test]
fn fixed_editorial_templates_do_not_gain_pages_from_smart_reflow() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"pages":1,"primaryTextFrame":true})).unwrap();
    let sid = s.active().unwrap().doc.settings.primary_story.unwrap();
    s.editor_mode = true;
    s.execute("text.select", &json!({"story":sid.0,"anchor":0})).unwrap();
    s.execute("text.insert", &json!({"text":"Información para el diario. ".repeat(1000)})).unwrap();
    assert_eq!(s.active().unwrap().doc.page_count(), 1);
    assert_eq!(s.active().unwrap().doc.story(sid).unwrap().frames.len(), 1);
    assert!(s.cache.get(&s.active().unwrap().doc, sid, None).is_overset());
}

#[test]
fn live_captions_update_with_photos_without_touching_protected_captions() {
    let (mut s, _, photo) = setup();
    s.execute("file.place", &json!({"frame":photo["id"],"base64":image(),"name":"original.png"})).unwrap();
    let caption = s.execute("object.caption", &json!({"ids":[photo["id"]],"text":"{name}","live":true})).unwrap();
    let id = ItemId(caption["ids"][0].as_u64().unwrap());
    let sid = s.active().unwrap().doc.item(id).unwrap().text_frame().unwrap().story;
    s.editor_mode = true;
    s.execute("file.place", &json!({"frame":photo["id"],"base64":image(),"name":"segunda.png"})).unwrap();
    assert_eq!(s.active().unwrap().doc.story(sid).unwrap().text, "segunda.png");
    s.editor_mode = false;
    s.execute("object.setFlags", &json!({"ids":[id.0],"contentLocked":true})).unwrap();
    s.editor_mode = true;
    s.execute("file.place", &json!({"frame":photo["id"],"base64":image(),"name":"tercera.png"})).unwrap();
    assert_eq!(s.active().unwrap().doc.story(sid).unwrap().text, "segunda.png");
}

#[test]
fn formatted_imports_and_table_formatting_cannot_bypass_template_styles() {
    let (mut s, text, _) = setup();
    s.execute("text.select", &json!({"story":text["story"],"anchor":0})).unwrap();
    s.editor_mode = true;
    unchanged_after_rejection(
        &mut s,
        "file.place",
        json!({"frame":text["id"],"name":"externo.rtf","base64":cmd::base64_encode(br"{\rtf1\b Imported bold}")}),
    );
    unchanged_after_rejection(&mut s, "table.setCell", json!({"attrs":{"fill":"[Black]"}}));
    unchanged_after_rejection(&mut s, "find.change", json!({"find":"Texto","change":"Noticia","attrs":{"size":48}}));
}

#[test]
fn host_assigned_editorial_sessions_cannot_promote_themselves() {
    let (mut s, _, _) = setup();
    s.restrict_to_editor();
    assert!(s.editor_role_locked());
    unchanged_after_rejection(&mut s, "file.newFromTemplate", json!({"editorMode":false}));
    // A stale host/UI mode flag cannot bypass the pinned role at the engine boundary.
    s.editor_mode = false;
    unchanged_after_rejection(&mut s, "frame.create", json!({"rect":[0,0,40,40]}));
    assert!(s.editor_mode);
}

#[test]
fn failing_to_open_a_template_does_not_pin_a_designer_session() {
    let (mut s, _, _) = setup();
    assert!(s.execute("file.newFromTemplate", &json!({"base64":"invalid","editorMode":true})).is_err());
    assert!(!s.editor_mode);
    assert!(!s.editor_role_locked());
}

#[test]
fn page_editorial_workflow_and_approved_protection() {
    let (mut s, text_id, _) = setup();
    // 1. Initial page status is Draft
    let st = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(st["status"], "draft");

    // 2. Designer configures page metadata
    s.execute(
        "page.editorial.set",
        &json!({
            "page": 0,
            "status": "inProgress",
            "section": "Portada",
            "assignedTo": "Juan Perez",
            "notes": "Foto principal pendiente de cierre"
        }),
    )
    .unwrap();

    let st = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(st["status"], "inProgress");
    assert_eq!(st["section"], "Portada");
    assert_eq!(st["assignedTo"], "Juan Perez");

    // Overview reports this page in progress
    let ov = s.execute("page.editorial.overview", &json!({})).unwrap();
    assert_eq!(ov["pageCount"], 1);
    assert_eq!(ov["approvedCount"], 0);
    assert_eq!(ov["pages"][0]["status"], "inProgress");

    // 3. In editor mode, writer can write text while page is inProgress
    s.editor_mode = true;
    let tid = ItemId(text_id["id"].as_u64().unwrap());
    let sid = s.doc().unwrap().doc.item(tid).unwrap().text_frame().unwrap().story;
    s.execute("story.setText", &json!({"story": sid.0, "text": "Noticia de portada"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.story(sid).unwrap().text, "Noticia de portada");

    // Writer cannot approve the page themselves
    assert!(s.execute("page.editorial.approve", &json!({"page": 0, "approved": true})).is_err());
    assert!(s.execute("page.editorial.set", &json!({"page": 0, "status": "approved"})).is_err());

    // Writer can mark page as NeedsReview
    s.execute("page.editorial.set", &json!({"page": 0, "status": "needsReview"})).unwrap();
    let st = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(st["status"], "needsReview");

    // 4. Designer approves the page
    s.editor_mode = false;
    s.execute("page.editorial.approve", &json!({"page": 0, "approved": true, "by": "Jefe de Cierre"})).unwrap();
    let st = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(st["status"], "approved");
    assert_eq!(st["approvedBy"], "Jefe de Cierre");

    let ov = s.execute("page.editorial.overview", &json!({})).unwrap();
    assert_eq!(ov["approvedCount"], 1);

    // 5. Now in editor mode, the approved page is completely protected
    s.editor_mode = true;
    // Writer cannot alter story on approved page
    assert!(s.execute("story.setText", &json!({"story": sid.0, "text": "Intento no autorizado"})).is_err());
    assert_eq!(s.doc().unwrap().doc.story(sid).unwrap().text, "Noticia de portada");

    // Writer cannot alter metadata on approved page
    assert!(s.execute("page.editorial.set", &json!({"page": 0, "notes": "Hacked"})).is_err());

    // 6. Designer reopens page
    s.editor_mode = false;
    s.execute("page.editorial.approve", &json!({"page": 0, "approved": false})).unwrap();
    let st = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(st["status"], "needsReview");

    // Writer can edit again
    s.editor_mode = true;
    s.execute("story.setText", &json!({"story": sid.0, "text": "Texto corregido"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.story(sid).unwrap().text, "Texto corregido");
}

#[test]
fn editorial_edition_date_and_placeholder_sync() {
    let (mut s, text, _) = setup();
    let sid = StoryId(text["story"].as_u64().unwrap());

    // Cintillo story with standard placeholders
    s.execute(
        "story.setText",
        &json!({
            "story": sid.0,
            "text": "EL DIARIO — Edición [FECHA] · [SECCION] · Pág. [PAGINA]"
        }),
    )
    .unwrap();

    // Set page editorial parameters
    s.execute(
        "page.editorial.set",
        &json!({
            "page": 0,
            "editionDate": "2026-10-18",
            "section": "OPINION",
            "pageNumber": 3
        }),
    )
    .unwrap();

    let info = s.execute("page.editorial.get", &json!({"page": 0})).unwrap();
    assert_eq!(info["editionDate"], "2026-10-18");
    assert_eq!(info["section"], "OPINION");
    assert_eq!(info["editorialPageNumber"], 3);

    // Verify story text updated
    let doc = &s.doc().unwrap().doc;
    assert_eq!(doc.story(sid).unwrap().text, "EL DIARIO — Edición 2026-10-18 · OPINION · Pág. 03");

    // Verify text variables updated
    let fv = doc.text_variables.iter().find(|v| v.name == "Fecha").unwrap();
    match &fv.kind {
        designcraft_doc::vars::VarKind::Custom { text } => assert_eq!(text, "2026-10-18"),
        _ => panic!("expected Custom text variable for Fecha"),
    }
    let sv = doc.text_variables.iter().find(|v| v.name == "Sección").unwrap();
    match &sv.kind {
        designcraft_doc::vars::VarKind::Custom { text } => assert_eq!(text, "OPINION"),
        _ => panic!("expected Custom text variable for Sección"),
    }
}
