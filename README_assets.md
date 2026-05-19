# assets — Shared Media

## Purpose

Brand images and shared static media referenced by the web dashboard, email digest, and future clients. Lives at repo root so multiple crates can serve the same files without duplication.

## Contents

```
assets/
├── README.md
└── brand/
    ├── README.md
    ├── pepper_avatar_teal.png    # Web dashboard header
    └── pepper_avatar_white.png   # Email digest (light-on-dark)
```

## Usage

| Surface | Path | Avatar |
|---------|------|--------|
| pepper-web | `/assets/brand/*` via Axum static mount | Teal |
| Email digest | Embedded or linked from templates | White |

## Open-source candidate

**Yes** for code integration; **replace PNGs** if open-sourcing under a license that requires asset clearance. No proprietary dependencies.
