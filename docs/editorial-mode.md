# Editorial template protection

DesignCraft's editorial mode keeps the existing template structure while writers edit
text, place photographs in existing frames and change their crops.

## Preparing a template

In designer mode, select a frame and enable **Protect content from editors** in Properties.
Use this for advertising, fixed text and other content that writers must retain.
Protect a group to protect its descendants. Locked or hidden layers also protect content.
The flag is stored in native `.designcraft` documents and defaults to false for older files.
Existing templates need their advertising explicitly marked; names are not used to guess it.

A layout lock protects geometry. A content lock protects the text or image as well.
In editorial mode geometry is fixed even for frames without a layout lock, so leave photo
content unprotected when writers should be able to replace it or adjust its crop.
If a story is threaded through multiple frames, protecting any frame protects the entire story.
Parent-page content is protected.

The existing command is also available to automation:

```json
{"command":"object.setFlags","params":{"ids":[42],"contentLocked":true}}
```

## Working as an editor

- Edit text on the canvas or in the Story Editor. Protected stories open read-only.
- Place or replace photographs in existing unprotected frames. Use direct selection for
  crop adjustments, or the contextual proportional fitting controls.
- Paragraph/character style definitions and template geometry cannot be changed.
- Character adjustments require an explicit existing CopyfitPolicy for the requested
  parameter. Every affected character run and paragraph must allow the value.
- Pasting text uses the destination formatting. Formatted document imports and changes
  to table structure/formatting are unavailable in editorial mode.
- Undo/redo applies to edits made in the current editorial session, preserving designer
  history underneath. Designers can still preview editorial mode using the regular toggle.
- Sessions opened with `?mode=editor` or `file.newFromTemplate {editorMode:true}` are pinned
  to editorial mode: the UI toggle and template commands cannot promote them to designer.
  A failed template open does not pin a designer session.
- Smart Text Reflow does not add or remove pages in editorial mode. Overset remains visible
  for the writer to resolve. Unprotected live captions still follow their source photos;
  protected captions retain their text.

Rejected edits do not publish a partially changed document. Replacing a shared asset is
also rejected when it would modify a protected placement.

This is a local workflow mode. User authentication, server-side authorization and
simultaneous editing are separate work for the collaboration phase; this flag is not
an account role or an access-control boundary for uploaded document files.

## Conservative automatic microfit

In the writer's contextual toolbar, **Microfit Text** adjusts the entire selected story,
including its threaded frames. The command is also available under Type. Select the frame
or text, then click once; successful changes are one undo step. Text is never rewritten.

In designer mode, edit the paragraph style under **Copyfitting**. Enable Size Flexibility
and/or Tracking Flexibility and save their minimum/maximum values. Character styles with
their own copyfit policy take precedence. All affected runs must permit the adjustment.
For short text, also enable **Close small bottom gap** (off in existing documents).
Without it, a normal short ending is never automatically expanded.

The automatic search is stricter than the manual limits: at most ±1% body size and
±5 tracking units, with tracking kept within ±15. It tries tracking first, then small
body steps, recomposing every candidate. Excess is limited to 3 words / 32 characters.
Closing a gap requires roughly one empty line at the bottom of the final column; only
2–3 words may move onto the new last line. A residual gap below 0.8 line is accepted.
This estimates a small shortage geometrically; it does not infer missing editorial content.

A candidate must fit without increasing H&J violation counts at any severity, keep
violations or consecutive hyphens, and cannot end with a lone word. These checks use the
existing compositor's diagnostics; visual editorial review remains necessary.
If none qualifies, nothing is applied and the writer is asked to revise the text.

The initial implementation supports ordinary horizontal stories with uniform body size
and tracking, at most 16 KiB of text, 16 frames, 256 paragraphs and 1024 character runs.
Tables, notes, anchored objects, linked stories, empty paragraphs, automatic frame sizing
and vertical alignment other than top are excluded. Style definitions, frame geometry, photographs and protected content remain outside
the automatic adjustment. Leading is never directly overridden; automatic leading still
follows body size when that is how the template defines it.

Automation uses the same command and template rules:

```json
{"command":"text.microfit","params":{"story":9}}
```

Configure a paragraph policy in designer mode, for example:

```json
{"command":"style.paragraph.edit","params":{"name":"Body","copyfit":{"size_min":9.9,"size_max":10.1,"trk_min":-5,"trk_max":5,"close_bottom":true}}}
```

These example values are not applied to existing user templates.

### Manual microsteps and visible feedback

The writer toolbar's size/tracking fields have spinner arrows. Each click changes size by
0.01 pt or tracking by 0.1; focused Up/Down uses the same steps, Shift multiplies by ten.
Typing an exact value remains available. Values stay within the template's limits.
Select actual text for a fragment adjustment, or select its frame to affect the whole story.
A caret with no selection shows guidance instead of presenting controls that do nothing.
Clicks against an already reached limit do not create redundant undo steps.

Automatic microfit success/rejection stays visible below the contextual controls for the
current story/revision, in addition to the footer. Changing the story content or undoing
invalidates that feedback. Missing policies and rejected manual changes are explained there.

### Configuring versus using the limits

Open a saved publication using **Maquetador** to configure its template styles. **Redactor**
opens an assigned editorial session and cannot save style definitions. Style dialogs in
that session explicitly show read-only controls and a disabled OK button; rejected style
saves retain the form and its error instead of discarding the input.

In designer mode, both paragraph and character Copyfitting dialogs share the same form,
with 0.01 pt / 0.1 spinner steps and decimal input. Written values persist when confirmed
and reopened. Invalid or incomplete ranges now return an error rather than silently
removing a permitted range. Opening an inherited policy without editing it preserves
inheritance; editing it creates an explicit local policy. Turning both ranges off in an
edited policy explicitly blocks those adjustments rather than re-enabling a base style.

Effective policies follow the existing bounded character and paragraph style inheritance
chains, with the nearest character policy taking precedence. The manual controls,
selection query and automatic search all use the same resolution. Automatic microfit
still intersects these limits with its conservative ±1% size / ±5 tracking safety margin,
which is now stated in the style form.

In editorial mode, Properties and Character show the allowed content/copyfit controls
instead of presenting disabled designer fields as if they were useful editing controls.
Size/tracking precision is consistent in designer Properties, Character, the control bar
and the contextual bar. A quick spinner click while its field has focus no longer lets
focus-loss processing overwrite the increment with the old text buffer.
