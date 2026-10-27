import { test } from "node:test";
import assert from "node:assert/strict";
import { createHighlighter } from "shiki";
import { highlight } from "./highlight.ts";
import { usfmLang } from "./usfm-lang.ts";

const SNIPPET = String.raw`\id GEN unfoldingWord
\c 1
\p
\v 3-4 And God said,~\w light|lemma="אוֹר" x-morph="He,Ncmsa"\w* //be
\v 12a \w grace|gracious\w* \f + \fr 1:2 \ft Or \+w spirit\+w*.\f*
\zaln-s |x-strong="H0430"\*\w God\w*\zaln-e\*`;

const scopesOf = async (): Promise<Map<string, string>> => {
  const shiki = await createHighlighter({ themes: ["github-light"], langs: [usfmLang] });
  const lines = shiki.codeToTokens(SNIPPET, { lang: "usfm" as never, theme: "github-light", includeExplanation: true }).tokens;
  const scopes = new Map<string, string>();
  for (const token of lines.flat()) {
    for (const part of token.explanation ?? []) {
      const key = part.content.trim();
      if (key && !scopes.has(key)) scopes.set(key, part.scopes.map((s) => s.scopeName).join(" "));
    }
  }
  return scopes;
};

test("usfm tokens get the scopes the github themes colour", async () => {
  const s = await scopesOf();
  const has = (text: string, scope: string): void => assert.match(s.get(text) ?? "(none)", new RegExp(scope.replaceAll(".", "\\."), "u"), text);
  has("\\id", "keyword.control");
  has("GEN", "entity.name.type.book");
  has("\\c", "keyword.control");
  has("1", "constant.numeric");
  has("3-4", "constant.numeric");
  has("12a", "constant.numeric");
  has("\\p", "entity.name.tag");
  has("\\w", "entity.name.tag");
  has("\\+w", "entity.name.tag");
  has("\\zaln-s", "entity.name.tag");
  has("\\*", "entity.name.tag");
  has("\\f", "entity.name.tag");
  has("+", "variable.parameter.caller");
  has("1:2", "constant.numeric.reference");
  has("lemma", "entity.other.attribute-name");
  has("x-strong", "entity.other.attribute-name");
  has("אוֹר", "string.quoted.double");
  has("=", "punctuation.separator.key-value");
  has("|", "punctuation.separator.attributes");
  has("gracious", "string.unquoted.attribute-value.default");
  has("//", "keyword.operator.break");
  has("~", "constant.character.nbsp");
  assert.equal(s.get("And God said,"), "source.usfm");
});

test("highlight() knows usfm and sfm", async () => {
  for (const lang of ["usfm", "sfm"]) {
    const html = await highlight("\\v 1 \\w word\\w*", lang);
    assert.ok((html.match(/style="color:/gu) ?? []).length >= 3, `${lang}: ${html}`);
  }
});
