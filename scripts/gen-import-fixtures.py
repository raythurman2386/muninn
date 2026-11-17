#!/usr/bin/env python3
"""Generate the tiny import fixtures committed under crates/muninn-import/tests.

Run from the repository root:

    python3 scripts/gen-import-fixtures.py

The fixtures are hand-built so they stay a few hundred bytes each and need no
office suite: minimal OOXML/EPUB packages, plain RTF, a text PDF, and one
OLE (CFB) container with the encryption streams a protected .docx carries.
Nothing here is part of a crate build; the generated files are committed.
"""

from __future__ import annotations

import base64
import pathlib
import struct
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "muninn-import" / "tests" / "fixtures"

# A 1x1 opaque red PNG. The docx fixture embeds exactly these bytes.
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
)

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
WP_NS = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
PIC_NS = "http://schemas.openxmlformats.org/drawingml/2006/picture"
P_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"
SML_NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"


def write_zip(name: str, parts: list[tuple[str, bytes]], *, mimetype: str | None = None) -> None:
    """Write a deflated zip; OCF requires its `mimetype` part first and stored."""
    with zipfile.ZipFile(OUT / name, "w", zipfile.ZIP_DEFLATED) as package:
        if mimetype is not None:
            package.writestr(
                zipfile.ZipInfo("mimetype"), mimetype, compress_type=zipfile.ZIP_STORED
            )
        for part, data in parts:
            package.writestr(part, data)


def content_types(defaults: dict[str, str], overrides: dict[str, str]) -> bytes:
    lines = [f'<Types xmlns="{CT_NS}">']
    for extension, media in defaults.items():
        lines.append(f'<Default Extension="{extension}" ContentType="{media}"/>')
    for part, media in overrides.items():
        lines.append(f'<Override PartName="{part}" ContentType="{media}"/>')
    lines.append("</Types>")
    return "".join(lines).encode()


def relationships(entries: list[tuple[str, str, str]]) -> bytes:
    lines = [f'<Relationships xmlns="{REL_NS}">']
    for rid, rel_type, target in entries:
        lines.append(f'<Relationship Id="{rid}" Type="{rel_type}" Target="{target}"/>')
    lines.append("</Relationships>")
    return "".join(lines).encode()


def docx() -> None:
    document = (
        f'<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}"><w:body>'
        '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>'
        "<w:r><w:t>Quarterly Report</w:t></w:r></w:p>"
        "<w:p><w:r><w:t>Revenue grew in every region.</w:t></w:r></w:p>"
        "</w:body></w:document>"
    ).encode()
    styles = (
        f'<w:styles xmlns:w="{W_NS}">'
        '<w:style w:type="paragraph" w:styleId="Heading1">'
        '<w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr>'
        "</w:style></w:styles>"
    ).encode()
    write_zip(
        "sample.docx",
        [
            ("[Content_Types].xml",
             content_types(
                 {"xml": "application/xml"},
                 {
                     "/word/document.xml": "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                     "/word/styles.xml": "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
                 },
             )),
            ("_rels/.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                  "word/document.xml"),
             ])),
            ("word/document.xml", document),
            ("word/styles.xml", styles),
            ("word/_rels/document.xml.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
                  "styles.xml"),
             ])),
        ],
    )


def docx_with_image() -> None:
    drawing = (
        '<w:p><w:r><w:drawing><wp:inline>'
        '<wp:extent cx="9525" cy="9525"/><wp:docPr id="1" name="Picture 1"/>'
        f'<a:graphic xmlns:a="{A_NS}"><a:graphicData uri="{PIC_NS}">'
        f'<pic:pic xmlns:pic="{PIC_NS}"><pic:nvPicPr><pic:cNvPr id="0" name="pixel.png"/>'
        "<pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>"
        '<a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        "<pic:spPr/></pic:pic></a:graphicData></a:graphic>"
        "</wp:inline></w:drawing></w:r></w:p>"
    )
    document = (
        f'<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="{WP_NS}"><w:body>'
        "<w:p><w:r><w:t>Picture Document</w:t></w:r></w:p>"
        f"{drawing}</w:body></w:document>"
    ).encode()
    write_zip(
        "embedded.docx",
        [
            ("[Content_Types].xml",
             content_types(
                 {"xml": "application/xml", "png": "image/png"},
                 {"/word/document.xml": "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"},
             )),
            ("_rels/.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                  "word/document.xml"),
             ])),
            ("word/document.xml", document),
            ("word/_rels/document.xml.rels",
             relationships([
                 ("rId2",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                  "media/image1.png"),
             ])),
            ("word/media/image1.png", PNG),
        ],
    )


def xlsx() -> None:
    workbook = (
        f'<workbook xmlns="{SML_NS}" xmlns:r="{R_NS}"><sheets>'
        '<sheet name="Budget" sheetId="1" r:id="rId1"/>'
        "</sheets></workbook>"
    ).encode()
    sheet = (
        f'<worksheet xmlns="{SML_NS}"><sheetData>'
        '<row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c>'
        '<c r="B1" t="inlineStr"><is><t>Value</t></is></c></row>'
        '<row r="2"><c r="A2" t="inlineStr"><is><t>North</t></is></c>'
        '<c r="B2"><v>42</v></c></row>'
        "</sheetData></worksheet>"
    ).encode()
    write_zip(
        "sample.xlsx",
        [
            ("[Content_Types].xml",
             content_types(
                 {"xml": "application/xml"},
                 {
                     "/xl/workbook.xml": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                     "/xl/worksheets/sheet1.xml": "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
                 },
             )),
            ("_rels/.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                  "xl/workbook.xml"),
             ])),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
                  "worksheets/sheet1.xml"),
             ])),
            ("xl/worksheets/sheet1.xml", sheet),
        ],
    )


def pptx() -> None:
    presentation = (
        f'<p:presentation xmlns:p="{P_NS}" xmlns:r="{R_NS}">'
        '<p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>'
        "</p:presentation>"
    ).encode()
    slide = (
        f'<p:sld xmlns:p="{P_NS}" xmlns:a="{A_NS}"><p:cSld><p:spTree>'
        '<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/>'
        '<p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/>'
        "<p:txBody><a:bodyPr/><a:p><a:r><a:t>Slide title</a:t></a:r></a:p>"
        "</p:txBody></p:sp>"
        "</p:spTree></p:cSld></p:sld>"
    ).encode()
    write_zip(
        "sample.pptx",
        [
            ("[Content_Types].xml",
             content_types(
                 {"xml": "application/xml"},
                 {
                     "/ppt/presentation.xml": "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
                     "/ppt/slides/slide1.xml": "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
                 },
             )),
            ("_rels/.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                  "ppt/presentation.xml"),
             ])),
            ("ppt/presentation.xml", presentation),
            ("ppt/_rels/presentation.xml.rels",
             relationships([
                 ("rId1",
                  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
                  "slides/slide1.xml"),
             ])),
            ("ppt/slides/slide1.xml", slide),
        ],
    )


def epub() -> None:
    container = (
        '<?xml version="1.0"?>'
        '<container version="1.0" '
        'xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        '<rootfiles><rootfile full-path="OEBPS/content.opf" '
        'media-type="application/oebps-package+xml"/></rootfiles></container>'
    ).encode()
    opf = (
        '<?xml version="1.0"?>'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        "<dc:identifier id=\"id\">muninn-fixture</dc:identifier>"
        "<dc:title>Fixture Book</dc:title><dc:language>en</dc:language></metadata>"
        '<manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>'
        '</manifest><spine><itemref idref="c1"/></spine></package>'
    ).encode()
    chapter = (
        '<?xml version="1.0"?>'
        '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>One</title></head>'
        "<body><h1>Chapter One</h1><p>The book begins here.</p></body></html>"
    ).encode()
    write_zip(
        "sample.epub",
        [
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", opf),
            ("OEBPS/chapter1.xhtml", chapter),
        ],
        mimetype="application/epub+zip",
    )


def odt() -> None:
    content = (
        '<office:document-content'
        ' xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"'
        ' xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"'
        ' office:version="1.2"><office:body><office:text>'
        '<text:h text:outline-level="1">Quarterly Report</text:h>'
        "<text:p>Revenue grew in every region.</text:p>"
        "</office:text></office:body></office:document-content>"
    ).encode()
    write_zip(
        "sample.odt",
        [("content.xml", content)],
        mimetype="application/vnd.oasis.opendocument.text",
    )


def pdf() -> None:
    """A one-page PDF with real text (no images), so extraction yields text."""
    stream = b"BT /F1 18 Tf 72 720 Td (Muninn import fixture) Tj ET\n"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"endstream",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for offset in offsets[1:]:
        out += f"{offset:010d} 00000 n \n".encode()
    out += (
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF\n"
    ).encode()
    (OUT / "sample.pdf").write_bytes(bytes(out))


def rtf() -> None:
    body = (
        r"{\rtf1\ansi\deff0{\fonttbl{\f0 Helvetica;}}"
        r"\f0\fs24 Quarterly report\par Revenue grew in every region.\par}"
    )
    (OUT / "sample.rtf").write_text(body, encoding="ascii")


def encrypted_docx() -> None:
    """An OLE container with the two streams protected OOXML packages carry.

    `anydoc` identifies this as `Encrypted` from the stream names before it
    looks for an inner format, which is exactly the MS-OFFCRYPTO shape.
    """
    FREE = 0xFFFFFFFF
    END = 0xFFFFFFFE
    FATSECT = 0xFFFFFFFD

    header = bytearray(512)
    header[0:8] = bytes.fromhex("D0CF11E0A1B11AE1")
    struct.pack_into("<HHHHH", header, 0x18, 0x003E, 0x0003, 0xFFFE, 9, 6)
    struct.pack_into("<IIIIIIII", header, 0x28, 0, 1, 1, 0, 4096, END, 0, END)
    struct.pack_into("<I", header, 0x48, 0)
    struct.pack_into("<I", header, 0x4C, 0)
    for slot in range(1, 109):
        struct.pack_into("<I", header, 0x4C + slot * 4, FREE)

    fat = bytearray(b"\xff" * 512)
    struct.pack_into("<II", fat, 0, FATSECT, END)

    def directory_entry(name: str, kind: int, child: int | None, right: int | None) -> bytes:
        entry = bytearray(128)
        encoded = name.encode("utf-16-le") + b"\0\0"
        entry[0 : len(encoded)] = encoded
        struct.pack_into("<H", entry, 0x40, len(encoded))
        entry[0x42] = kind
        entry[0x43] = 1
        struct.pack_into("<I", entry, 0x44, FREE)
        struct.pack_into("<I", entry, 0x48, FREE if right is None else right)
        struct.pack_into("<I", entry, 0x4C, FREE if child is None else child)
        struct.pack_into("<I", entry, 0x74, END)
        struct.pack_into("<Q", entry, 0x78, 0)
        return bytes(entry)

    directory = bytearray(512)
    directory[0:128] = directory_entry("Root Entry", 5, child=1, right=None)
    directory[128:256] = directory_entry("EncryptionInfo", 2, child=None, right=2)
    directory[256:384] = directory_entry("EncryptedPackage", 2, child=None, right=None)

    (OUT / "encrypted.docx").write_bytes(bytes(header) + bytes(fat) + bytes(directory))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    docx()
    docx_with_image()
    xlsx()
    odt()
    pptx()
    epub()
    pdf()
    rtf()
    encrypted_docx()
    (OUT / "pixel.png").write_bytes(PNG)
    (OUT / "sample.csv").write_text("Name,Value\nNorth,42\n", encoding="utf-8")
    for path in sorted(OUT.iterdir()):
        print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
