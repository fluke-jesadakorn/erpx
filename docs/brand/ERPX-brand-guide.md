# ERPX brand guide

ERPX uses **Precision E**: three engineered bars form the first letter of the ERPX wordmark. The shared angle and spacing represent coordinated enterprise operations, while the forward-cut terminals suggest AI-assisted momentum. E, R and P always share one color within a logo variant. X is full-height and uses the same base color at 48% opacity, expressing the multiplier while keeping ERP visually primary.

## Master artwork

- `public/brand/erpx-symbol.svg` — Electric Blue E symbol
- `public/brand/erpx-symbol-mono.svg` — black E symbol
- `public/brand/erpx-symbol-white.svg` — white E symbol
- `public/brand/erpx-wordmark.svg` — compact path-only ERPX artwork
- `public/brand/erpx-lockup.svg` — padded Electric Blue master for light backgrounds
- `public/brand/erpx-lockup-mono.svg` — padded black master for print and one-color use
- `public/brand/erpx-lockup-inverse.svg` — padded white master for dark backgrounds
- `public/brand/erpx-favicon.svg` — white E on an Electric Blue rounded tile
- `public/brand/concepts/` — earlier construction studies retained as design history, not alternate master logos

Use the SVG masters for layout and exports. Do not typeset the wordmark, redraw the symbol, add a tagline to the lockup, stretch, rotate, crop, outline, shadow or recolor individual parts.

## Color and spacing

Electric Blue is `#2F69F5`. The app may use its theme brand token so E, R and P move together between the light and dark palettes. Use the inverse artwork on dark fields and the monochrome artwork when production permits only one ink. Do not recolor E separately from R or P. Preserve the X opacity at 48% in every full wordmark variant.

Keep clear space of at least one quarter of the ERP letter height around the visible artwork. Minimum recommended symbol size is 16 px on screen or 5 mm in print; minimum horizontal wordmark width is 128 px on screen or 32 mm in print.

## App icons

`src/app/icon.svg` uses the same geometry as `public/brand/erpx-favicon.svg`. Generated assets include a 16/32/48 px multi-resolution favicon, 180 px Apple icon, 192/512 px PWA icons and a 512 px maskable icon. Regenerate them with `bun scripts/assets/generate-brand-assets.ts`; do not upscale a favicon. The maskable icon uses an opaque Electric Blue field and keeps the white E inside the central safe zone.

## Customer co-branding

ERPX remains visible in application navigation as the Product Brand. Holding and Branch marks identify the active Workspace and business-document context. Printed documents omit ERPX branding: they always name the legal Company and may visually show Holding, Branch, or both according to the record-derived scope.

This guide defines visual usage only. It is not a trademark search, registration opinion or legal clearance.
