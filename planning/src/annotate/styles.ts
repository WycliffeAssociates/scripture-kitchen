/**
 * The annotator's own styles, as a string injected into its shadow root.
 *
 * A shadow root rather than a class-name convention, and this is the single
 * most important decision in the folder. The tool's whole job is to sit on top
 * of a design somebody is actively changing — changing the very tokens,
 * resets and font stacks the tool would otherwise inherit. A panel that
 * restyles itself when you darken a token is a panel you cannot trust to tell
 * you what the token did. Shadow DOM is the only way to be certain, and it
 * also means this module needs no build step and no CSS file to copy when it
 * is lifted into another project.
 *
 * Deliberately NOT theme-aware. It is one fixed dark chrome in both themes,
 * so the tool always reads as "not part of the page" — which is exactly what
 * it is.
 */

export const PANEL_STYLES = `
:host { all: initial; }
* { box-sizing: border-box; font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; }

.root {
  position: fixed;
  z-index: 2147483000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  color: #e8e8ec;
  font-size: 12px;
  line-height: 1.4;
}
.root[data-corner="bottom-right"] { right: 16px; bottom: 16px; align-items: flex-end; }
.root[data-corner="bottom-left"]  { left: 16px;  bottom: 16px; align-items: flex-start; }
.root[data-corner="top-right"]    { right: 16px; top: 16px;    align-items: flex-end; }
.root[data-corner="top-left"]     { left: 16px;  top: 16px;    align-items: flex-start; }

.panel {
  min-width: 260px;
  max-width: 340px;
  background: #1c1c20;
  border: 1px solid #34343c;
  border-radius: 10px;
  box-shadow: 0 10px 30px rgb(0 0 0 / 45%);
  overflow: hidden;
}
.head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 6px 6px 10px;
  border-bottom: 1px solid #2a2a31;
}
.title { flex: 1; font-weight: 600; font-size: 11px; letter-spacing: .02em; color: #9a9aa6; text-transform: uppercase; }
.body { padding: 10px; display: flex; flex-direction: column; gap: 10px; max-height: 60vh; overflow-y: auto; }
.section { display: flex; flex-direction: column; gap: 6px; }
.legend { font-size: 10px; letter-spacing: .04em; text-transform: uppercase; color: #74747f; }

button {
  font: inherit;
  color: inherit;
  background: #26262c;
  border: 1px solid #3a3a44;
  border-radius: 6px;
  padding: 4px 8px;
  cursor: pointer;
}
button:hover { background: #303038; }
button[aria-pressed="true"] { background: #3d6be0; border-color: #3d6be0; color: #fff; }
button.icon { padding: 4px 6px; line-height: 1; }
button.ghost { background: transparent; border-color: transparent; color: #9a9aa6; }
button.ghost:hover { background: #26262c; color: #e8e8ec; }
button.primary { background: #3d6be0; border-color: #3d6be0; color: #fff; }
button.primary:hover { background: #4a76e6; }

.row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.segmented { display: inline-flex; background: #26262c; border: 1px solid #3a3a44; border-radius: 6px; overflow: hidden; }
.segmented button { border: 0; border-radius: 0; background: transparent; }
.segmented button + button { border-left: 1px solid #3a3a44; }
/* Specificity, not decoration: ".segmented button" and "button[aria-pressed]"
   tie at (0,1,1), so without this the later rule wins and the SELECTED segment
   paints transparent — which means comment mode looks exactly like interact
   mode, on the one control where knowing which mode you are in is the whole
   point. */
.segmented button[aria-pressed="true"] { background: #3d6be0; color: #fff; }

label.field { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
label.field > span { color: #b6b6c0; }
select { font: inherit; color: inherit; background: #26262c; border: 1px solid #3a3a44; border-radius: 6px; padding: 3px 6px; }
input[type="checkbox"] { accent-color: #3d6be0; width: 14px; height: 14px; }

textarea {
  font: inherit;
  width: 100%;
  min-height: 64px;
  resize: vertical;
  color: inherit;
  background: #26262c;
  border: 1px solid #4a4a56;
  border-radius: 6px;
  padding: 6px 8px;
}
textarea:focus { outline: 2px solid #3d6be0; outline-offset: -1px; }

/* A normal flex child of .root, NOT absolutely positioned. Absolute with no
   offsets left it at its static position, which for the bottom-right corner
   pushed it off the right edge of the viewport — a menu you cannot read. As a
   flow child it inherits the corner alignment and can only ever grow back into
   the screen. */
.menu {
  position: relative;
  max-width: 340px;
  background: #26262c;
  border: 1px solid #3a3a44;
  border-radius: 8px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
}
.menu button { background: transparent; border: 0; text-align: left; white-space: nowrap; }
.menu button:hover { background: #33333b; }
.menu-head { display: flex; align-items: center; gap: 6px; padding: 2px 2px 4px 8px; }
.menu-head .title { flex: 1; }
/* The close button keeps the icon padding but not the left-aligned, full-width
   shape the other menu rows have. */
.menu-head button { flex: 0 0 auto; padding: 4px 6px; line-height: 1; }

.comments { display: flex; flex-direction: column; gap: 6px; }
.comment { display: flex; gap: 6px; align-items: flex-start; background: #232329; border: 1px solid #32323a; border-radius: 6px; padding: 6px 8px; }
.comment .where { color: #8d8d98; font-size: 10px; word-break: break-all; }
.comment .what { color: #dcdce4; }
.comment > div { flex: 1; min-width: 0; }

.puck {
  width: 36px; height: 36px; border-radius: 999px;
  background: #1c1c20; border: 1px solid #34343c;
  box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
  display: grid; place-items: center; cursor: pointer;
}
.puck[data-mode="comment"] { background: #3d6be0; border-color: #3d6be0; }

/* Minimised with comments waiting: the copy pill sits beside the puck. */
.dock { display: flex; align-items: center; gap: 8px; }
.copy-all {
  background: #3d6be0; color: #fff; border: 0; border-radius: 999px;
  padding: 7px 12px; font: inherit; font-weight: 600; cursor: pointer;
  box-shadow: 0 8px 24px rgb(0 0 0 / 35%);
}

/* Shown whatever state the panel is in. */
.toast {
  position: fixed; left: 50%; bottom: 24px; transform: translate(-50%, 8px);
  background: #1c1c20; color: #f2f2f6; border: 1px solid #34343c;
  border-radius: 8px; padding: 8px 14px; font-size: 12px;
  box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
  opacity: 0; pointer-events: none; transition: opacity 140ms, transform 140ms;
}
.toast[data-show] { opacity: 1; transform: translate(-50%, 0); }

.empty { color: #74747f; }
.hint { color: #74747f; font-size: 10px; }

/* Pins. The layer is inert and the pins themselves are not, so a pin can be
   hovered for its text while every pixel between them still belongs to the
   application underneath. */
.pins { position: fixed; inset: 0; pointer-events: none; }
.pin {
  position: fixed;
  transform: translate(-50%, -50%);
  width: 20px; height: 20px;
  border-radius: 999px;
  background: #3d6be0;
  border: 2px solid #fff;
  box-shadow: 0 2px 8px rgb(0 0 0 / 35%);
  display: grid; place-items: center;
  font-size: 10px; font-weight: 700; color: #fff;
  pointer-events: auto;
  cursor: default;
}
/* A pin waiting for its sentence, in the colour of something unfinished. */
.pin[data-pending="true"] { background: #d08a2c; }
.pin .bubble {
  display: none;
  position: absolute;
  top: 16px; left: 12px;
  width: max-content;
  max-width: 260px;
  background: #1c1c20;
  border: 1px solid #34343c;
  border-radius: 8px;
  box-shadow: 0 10px 30px rgb(0 0 0 / 45%);
  padding: 6px 8px;
  font-size: 11px; font-weight: 400;
  color: #dcdce4;
  text-align: left;
  white-space: pre-wrap;
}
.pin:hover { z-index: 1; }
.pin:hover .bubble { display: block; }
.pin .bubble .where { color: #8d8d98; font-size: 10px; word-break: break-all; }
`;

/**
 * Styles for the two things that must live in the PAGE rather than the shadow
 * root, because they position themselves against page elements: the hover
 * outline and the crosshair cursor.
 *
 * Scoped under one attribute and one class, both prefixed, so they cannot
 * collide with anything the host has. This is the only mark the annotator
 * leaves on the document, and it is removed on `destroy()`.
 */
export const PAGE_STYLES = `
[data-sefer-annotate-mode="comment"], [data-sefer-annotate-mode="comment"] * { cursor: crosshair !important; }
.sefer-annotate-outline {
  position: fixed;
  pointer-events: none;
  z-index: 2147482999;
  border: 2px solid #3d6be0;
  border-radius: 3px;
  background: rgb(61 107 224 / 12%);
  transition: all .06s ease-out;
}
.sefer-annotate-edge {
  position: fixed;
  inset: 0;
  pointer-events: none;
  z-index: 2147482998;
  box-shadow: inset 0 0 0 3px rgb(61 107 224 / 55%);
}
`;
