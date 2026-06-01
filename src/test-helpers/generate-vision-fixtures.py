"""
Generate synthetic vision-test fixtures for the MiniMax M2.7 image
understanding tests. Run from the project root with:

    python3 src/test-helpers/generate-vision-fixtures.py

The four outputs land in src/test-helpers/fixtures/vision/ and cover
the four comprehension axes the test plan requires:

  * chart.png   — line chart, exercises OCR + numeric readout
  * screenshot.png — UI mockup, exercises layout reasoning
  * diagram.jpg — architecture diagram, exercises relationship inference
  * photo.webp  — synthetic "natural photo" (gradient + shapes), exercises
                  scene description

We render programmatically with PIL rather than scraping the web so
fixtures are reproducible and don't drift if a remote image moves.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

OUT_DIR = Path(__file__).resolve().parent / "fixtures" / "vision"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# DejaVuSans ships with most distros and Pillow's freetype loader, so we
# don't need to bundle a font. Fall back to PIL's default if missing.
def get_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for candidate in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]:
        if Path(candidate).exists():
            return ImageFont.truetype(candidate, size=size)
    return ImageFont.load_default()


# -- chart.png ---------------------------------------------------------------
# Line chart with two series + axis labels. Forces the model to read
# both the title and the numeric values along the y-axis, which catches
# the "model just says 'a chart'" failure mode.
def make_chart() -> Path:
    W, H = 800, 500
    img = Image.new("RGB", (W, H), "white")
    d = ImageDraw.Draw(img)
    title_font = get_font(28)
    label_font = get_font(16)

    d.text((W // 2 - 160, 16), "Quarterly Revenue 2024-2025", fill="black", font=title_font)

    margin_l, margin_r, margin_t, margin_b = 80, 40, 80, 60
    plot_l, plot_t = margin_l, margin_t
    plot_w, plot_h = W - margin_l - margin_r, H - margin_t - margin_b

    d.line([(plot_l, plot_t), (plot_l, plot_t + plot_h), (plot_l + plot_w, plot_t + plot_h)],
           fill="black", width=2)

    y_ticks = [(plot_t + plot_h - i * plot_h // 5, f"{i * 200}k") for i in range(6)]
    for y, lbl in y_ticks:
        d.line([(plot_l - 5, y), (plot_l, y)], fill="black", width=1)
        d.text((plot_l - 60, y - 8), lbl, fill="black", font=label_font)

    quarters = ["Q1'24", "Q2'24", "Q3'24", "Q4'24", "Q1'25", "Q2'25"]
    for i, q in enumerate(quarters):
        x = plot_l + (i + 0.5) * plot_w / len(quarters)
        d.line([(x, plot_t + plot_h), (x, plot_t + plot_h + 5)], fill="black", width=1)
        d.text((x - 22, plot_t + plot_h + 10), q, fill="black", font=label_font)

    def series(values, color):
        pts = []
        for i, v in enumerate(values):
            x = plot_l + (i + 0.5) * plot_w / len(values)
            y = plot_t + plot_h - int(v / 1000) * (plot_h / 5)
            pts.append((x, y))
        for a, b in zip(pts, pts[1:]):
            d.line([a, b], fill=color, width=3)
        for x, y in pts:
            d.ellipse((x - 4, y - 4, x + 4, y + 4), fill=color)

    series([320, 410, 480, 560, 720, 880], "steelblue")  # actual
    series([300, 400, 500, 600, 700, 800], "tomato")     # target

    d.text((plot_l + 20, plot_t + 10), "— actual", fill="steelblue", font=label_font)
    d.text((plot_l + 20, plot_t + 30), "— target", fill="tomato", font=label_font)
    d.text((10, H // 2 - 40), "Revenue", fill="black", font=label_font)

    out = OUT_DIR / "chart.png"
    img.save(out, "PNG")
    return out


# -- screenshot.png ----------------------------------------------------------
# Mocked UI screenshot with a sidebar, header, and three cards. Forces
# the model to recognise layout / widget vocabulary, not just describe
# generic "an interface".
def make_screenshot() -> Path:
    W, H = 800, 500
    img = Image.new("RGB", (W, H), "#f3f4f6")
    d = ImageDraw.Draw(img)
    f_title = get_font(22)
    f_label = get_font(16)
    f_small = get_font(13)

    # Top bar
    d.rectangle((0, 0, W, 56), fill="#1f2937")
    d.text((24, 16), "LLM Wiki · Knowledge Base", fill="white", font=f_title)
    d.text((W - 180, 22), "user@example.com", fill="#d1d5db", font=f_label)

    # Sidebar
    d.rectangle((0, 56, 200, H), fill="#e5e7eb")
    items = ["Wiki", "Sources", "Search", "Graph", "Lint", "Review", "Deep Research", "Settings"]
    for i, name in enumerate(items):
        y = 80 + i * 38
        if name == "Deep Research":
            d.rectangle((0, y - 6, 200, y + 26), fill="#3b82f6")
            d.text((28, y), name, fill="white", font=f_label)
        else:
            d.text((28, y), name, fill="#374151", font=f_label)

    # Three "cards" in the main pane
    for i, (label, sub) in enumerate([
        ("Pages indexed", "1,284"),
        ("Ingest queue", "3 pending"),
        ("Last research", "WebGPU in 2025"),
    ]):
        x0 = 232 + i * 180
        d.rounded_rectangle((x0, 96, x0 + 160, 200), radius=10, fill="white", outline="#d1d5db", width=1)
        d.text((x0 + 14, 110), label, fill="#6b7280", font=f_small)
        d.text((x0 + 14, 140), sub, fill="#111827", font=f_title)

    # Main panel placeholder
    d.rounded_rectangle((232, 224, W - 24, H - 24), radius=8, fill="white", outline="#d1d5db", width=1)
    d.text((252, 244), "Deep Research — WebGPU in 2025", fill="#111827", font=f_title)
    d.text((252, 280), "Round 1/3 · searching 6 sub-queries…", fill="#6b7280", font=f_label)

    out = OUT_DIR / "screenshot.png"
    img.save(out, "PNG")
    return out


# -- diagram.jpg -------------------------------------------------------------
# Box-and-arrow architecture diagram. Exercises relationship inference
# ("X calls Y", "Z depends on W") — a common weak spot for VLM models
# that just describe shapes.
def make_diagram() -> Path:
    W, H = 900, 500
    img = Image.new("RGB", (W, H), "white")
    d = ImageDraw.Draw(img)
    f_title = get_font(20)
    f_label = get_font(14)

    d.text((W // 2 - 120, 16), "LLM Wiki Architecture", fill="black", font=f_title)

    def box(x, y, w, h, label, fill="#dbeafe"):
        d.rounded_rectangle((x, y, x + w, y + h), radius=6, fill=fill, outline="#1e3a8a", width=2)
        d.text((x + 12, y + h // 2 - 8), label, fill="#111827", font=f_label)

    def arrow(a, b, label=None):
        d.line([a, b], fill="#1e3a8a", width=2)
        # arrowhead
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = (dx * dx + dy * dy) ** 0.5 or 1
        ux, uy = dx / L, dy / L
        # perpendicular
        px, py = -uy, ux
        for sign in (1, -1):
            tip = (b[0] - 8 * ux + sign * 4 * px, b[1] - 8 * uy + sign * 4 * py)
            d.line([b, tip], fill="#1e3a8a", width=2)
        if label:
            mx, my = (a[0] + b[0]) // 2, (a[1] + b[1]) // 2
            d.text((mx, my - 14), label, fill="#1e3a8a", font=f_label)

    box(80, 120, 160, 60, "Sources (PDF / DOCX / MD)")
    box(360, 60, 200, 60, "Ingest Queue (Rust)")
    box(360, 200, 200, 60, "LLM (M2.7)")
    box(360, 340, 200, 60, "Wiki Pages")
    box(660, 200, 180, 60, "Knowledge Graph")

    arrow((240, 150), (360, 90), "enqueue")
    arrow((360, 120), (360, 200), "process")
    arrow((360, 260), (360, 340), "write")
    arrow((560, 370), (560, 230), "feed")
    arrow((560, 230), (660, 230), "edges")
    arrow((360, 90), (660, 200), "auto-ingest")

    out = OUT_DIR / "diagram.jpg"
    img.save(out, "JPEG", quality=85)
    return out


# -- photo.webp --------------------------------------------------------------
# Synthetic "natural photo" — a gradient sky, a sun, a hill silhouette.
# Tests the model can describe a scene (not just refuse or call it
# "abstract"). Rendered as WebP to also exercise the
# `image/webp` MIME path on the Anthropic wire.
def make_photo() -> Path:
    W, H = 800, 500
    img = Image.new("RGB", (W, H))
    px = img.load()
    for y in range(H):
        # dusk gradient: top indigo → bottom orange
        t = y / (H - 1)
        r = int(20 + (255 - 20) * t)
        g = int(30 + (180 - 30) * t)
        b = int(80 + (90 - 80) * (1 - t))
        for x in range(W):
            px[x, y] = (r, g, b)

    d = ImageDraw.Draw(img, "RGBA")
    # sun
    d.ellipse((520, 100, 660, 240), fill=(255, 230, 150, 255))
    # hill silhouette
    d.polygon(
        [(0, H), (0, 380), (200, 320), (420, 360), (600, 310), (W, 380), (W, H)],
        fill=(20, 30, 40, 255),
    )
    # second hill in front
    d.polygon(
        [(0, H), (0, 440), (260, 400), (520, 430), (W, 410), (W, H)],
        fill=(10, 18, 26, 255),
    )
    # small bird
    d.arc((180, 200, 220, 220), start=200, end=340, fill=(40, 40, 50, 255), width=2)
    d.arc((240, 180, 280, 200), start=200, end=340, fill=(40, 40, 50, 255), width=2)

    out = OUT_DIR / "photo.webp"
    img.save(out, "WEBP", quality=88)
    return out


def main() -> int:
    outputs = [make_chart(), make_screenshot(), make_diagram(), make_photo()]
    print("generated:")
    for p in outputs:
        size = p.stat().st_size
        print(f"  {p.relative_to(Path.cwd())}  ({size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
