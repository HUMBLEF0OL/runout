"""Regenerate BRD.md from BRD.docx. Run after every BRD.docx edit: python scripts/brd_to_md.py"""
from pathlib import Path

import docx
from docx.table import Table
from docx.text.paragraph import Paragraph

ROOT = Path(__file__).resolve().parent.parent
HEADINGS = {"Heading1": "## ", "Heading2": "### ", "Heading3": "#### "}
SKIP = {"Contents", "Right-click the table above and choose Update Field to refresh page numbers."}


def cell(text):
    return text.replace("|", "\\|").replace("\n", "<br>").strip()


def convert(src):
    d = docx.Document(src)
    out = ["<!-- Generated from BRD.docx by scripts/brd_to_md.py. Edit the .docx, then regenerate. -->", ""]
    first = True
    for el in d.element.body.iterchildren():
        if el.tag.endswith("}tbl"):
            rows = [[cell(c.text) for c in r.cells] for r in Table(el, d).rows]
            out += ["| " + " | ".join(rows[0]) + " |", "|" + "---|" * len(rows[0])]
            out += ["| " + " | ".join(r) + " |" for r in rows[1:]] + [""]
        elif el.tag.endswith("}p"):
            p = Paragraph(el, d)
            text = p.text.strip()
            if not text or text in SKIP:
                continue
            ppr = el.pPr
            style = ppr.pStyle.val if ppr is not None and ppr.pStyle is not None else None
            if first:  # ponytail: title is the first non-empty paragraph, subtitle the second
                out += ["# " + text, ""]
                first = None
            elif first is None:
                out += ["**" + text + "**", ""]
                first = False
            elif style in HEADINGS:
                out += [HEADINGS[style] + text, ""]
            elif ppr is not None and ppr.numPr is not None:
                out.append("- " + text)
            else:
                if out[-1].startswith("- "):
                    out.append("")
                out += [text, ""]
    return "\n".join(out).rstrip() + "\n"


if __name__ == "__main__":
    md = convert(ROOT / "BRD.docx")
    (ROOT / "BRD.md").write_text(md, encoding="utf-8")
    assert "Runout" in md and "## 1. Executive Summary" in md, "conversion lost content"
    print(f"BRD.md written ({len(md.splitlines())} lines)")
