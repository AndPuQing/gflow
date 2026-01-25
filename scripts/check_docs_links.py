#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable
from urllib.parse import urlsplit


DOCS_ROOT = Path(__file__).resolve().parents[1] / "docs" / "src"


SKIP_SCHEMES = {
    "http",
    "https",
    "mailto",
    "tel",
    "ftp",
    "ftps",
    "data",
    "javascript",
}


CODE_FENCE_RE = re.compile(r"```.*?```", re.S)

# Markdown links and images: [text](target) / ![alt](target)
MD_LINK_RE = re.compile(r"(!?\[[^\]]*\])\(([^)]+)\)")

HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$", re.M)


@dataclass(frozen=True)
class LinkRef:
    source: Path
    raw_target: str
    target: str
    fragment: str | None


def strip_code_fences(markdown: str) -> str:
    return CODE_FENCE_RE.sub("", markdown)


def parse_links(markdown: str, source: Path) -> Iterable[LinkRef]:
    for _label, raw_target in MD_LINK_RE.findall(markdown):
        target = raw_target.strip()
        if not target:
            continue

        # Allow optional surrounding angle brackets: (<...>)
        if target.startswith("<") and target.endswith(">"):
            target = target[1:-1].strip()
            if not target:
                continue

        parts = urlsplit(target)
        if parts.scheme and parts.scheme.lower() in SKIP_SCHEMES:
            continue

        fragment = parts.fragment if parts.fragment else None
        # urlsplit keeps path in .path; rebuild without fragment/query
        normalized = parts.path
        if parts.query:
            normalized = f"{normalized}?{parts.query}"

        yield LinkRef(
            source=source,
            raw_target=raw_target,
            target=normalized,
            fragment=fragment,
        )


def github_slugify(text: str) -> str:
    # Close enough to GitHub/VitePress slugging for typical headings.
    slug = text.strip().lower()
    slug = re.sub(r"[^\w\s-]", "", slug, flags=re.UNICODE)
    slug = re.sub(r"[\s_-]+", "-", slug).strip("-")
    return slug


def collect_heading_slugs(markdown: str) -> set[str]:
    slugs: set[str] = set()
    counts: dict[str, int] = {}
    for _hashes, title in HEADING_RE.findall(markdown):
        # Strip trailing hashes: "Title ###"
        title = re.sub(r"\s#+\s*$", "", title).strip()
        base = github_slugify(title)
        if not base:
            continue
        n = counts.get(base, 0)
        counts[base] = n + 1
        slug = base if n == 0 else f"{base}-{n}"
        slugs.add(slug)
    return slugs


def resolve_vitepress_path(
    source: Path, target_path: str
) -> tuple[Path | None, bool]:
    """
    Returns (resolved_path, is_markdown_page).
    """
    if target_path.startswith("#"):
        return source, True

    # Remove query if present (rare in docs links)
    path_only = target_path.split("?", 1)[0]

    if path_only.startswith("/"):
        p = path_only.lstrip("/")
        # VitePress routes: /foo/bar -> docs/src/foo/bar.md or docs/src/foo/bar/index.md
        if not Path(p).suffix:
            candidates = [
                DOCS_ROOT / f"{p}.md",
                DOCS_ROOT / p / "index.md",
            ]
            for c in candidates:
                if c.exists():
                    return c, True
        else:
            # Assets under / are served from docs/src/public first.
            asset_candidates = [DOCS_ROOT / p, DOCS_ROOT / "public" / p]
            for c in asset_candidates:
                if c.exists():
                    return c, c.suffix.lower() == ".md"
            # Map .html to .md when linking to built output.
            if p.endswith(".html"):
                md = DOCS_ROOT / (p[:-5] + ".md")
                if md.exists():
                    return md, True
        return None, False

    # Relative path.
    base_dir = source.parent
    rel = (base_dir / path_only).resolve()

    # Ensure relative links don't escape docs root.
    try:
        rel.relative_to(DOCS_ROOT)
    except ValueError:
        return None, False

    if rel.exists():
        return rel, rel.suffix.lower() == ".md"

    if not rel.suffix:
        candidates = [rel.with_suffix(".md"), rel / "index.md"]
        for c in candidates:
            if c.exists():
                return c, True

    if rel.suffix.lower() == ".html":
        md = rel.with_suffix(".md")
        if md.exists():
            return md, True

    return None, False


def main() -> int:
    if not DOCS_ROOT.exists():
        print(f"error: docs root not found: {DOCS_ROOT}", file=sys.stderr)
        return 2

    md_files = sorted(DOCS_ROOT.rglob("*.md"))
    headings_cache: dict[Path, set[str]] = {}
    errors: list[str] = []

    for md_path in md_files:
        raw = md_path.read_text(encoding="utf-8")
        content = strip_code_fences(raw)

        for link in parse_links(content, md_path):
            if link.target.startswith("#"):
                resolved, is_md = md_path, True
            else:
                resolved, is_md = resolve_vitepress_path(md_path, link.target)

            if resolved is None:
                errors.append(
                    f"{md_path.relative_to(DOCS_ROOT)}: broken link: ({link.raw_target})"
                )
                continue

            if link.fragment and is_md:
                slugs = headings_cache.get(resolved)
                if slugs is None:
                    slugs = collect_heading_slugs(strip_code_fences(resolved.read_text(encoding='utf-8')))
                    headings_cache[resolved] = slugs
                frag = link.fragment.lstrip("#")
                if frag and frag not in slugs:
                    errors.append(
                        f"{md_path.relative_to(DOCS_ROOT)}: broken anchor: ({link.raw_target}) -> {resolved.relative_to(DOCS_ROOT)}#{frag}"
                    )

    if errors:
        for e in errors:
            print(e, file=sys.stderr)
        print(f"\nfailed: {len(errors)} issue(s) found", file=sys.stderr)
        return 1

    print(f"ok: {len(md_files)} markdown file(s), no broken links detected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
