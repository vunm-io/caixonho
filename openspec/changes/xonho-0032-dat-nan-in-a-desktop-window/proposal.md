# Đất Nặn, in a desktop window

## Why

The owner has a design system — **Đất Nặn**, claymation — built for
`vunm.io.vn` and packaged as a skill with 165 files: tokens, 27 components, 17
guideline cards and six UI kits. They asked for it here: *"tôi muốn apply
design style này vào cho caixonho"*.

This is not a re-skin invented for the occasion. Two things were found by
reading the system rather than deciding here:

- **It already has an app branch.** `design-system.md`: *"App / dashboard: nền
  nguội `--surface-app` (#F2F4F0), sidebar `--surface-app-sidebar`, thẻ trắng
  `--surface-app-raised`, kẻ `--app-line`. **Không dùng giấy vàng cho app.**"*
  The warm paper that is the brand's signature on the web is explicitly *not*
  what an app gets, because a large yellow field makes data hard to read.
- **It has a desktop file-manager UI kit.** `ui_kits/desktop/FileApp.jsx` — a
  1280×800 window with a folder sidebar, a file grid, a data table and a
  preview panel. It is very close to what caixonho already is, and it settles
  the questions that would otherwise be taste.

## What the system decides, so this change does not

| Question | The system's answer |
|---|---|
| How much clay in an app? | *"Clay trong app này **chỉ còn ở sidebar (mục đang xem), nút, chip và badge**"* |
| The tables? | Flat. *"Không bọc bảng trong `Card surface="clay"`"* |
| A selected row? | Raised background + a thin `--clay-aqua` border — *"không tô vàng, không nhấc khối lên"* |
| The delete control? | `tone="danger"` — so terracotta `#D4552F`, the system's own `--status-danger` |
| The background? | `--surface-app` `#F2F4F0`, cool. Not paper |

## What this changes

The **app branch** of the system, and only that:

- Colour, surface, elevation, radius, type scale and spacing tokens, into
  `theme.json` and `theme.rs`.
- Baloo 2 and Be Vietnam Pro embedded and loaded — display and body, never
  mixed inside one block.
- Clay where the system says clay: the sidebar's current item, buttons, chips,
  badges, the empty states. Flat everywhere else, which here means both tables
  and every strip.

## What it cannot do, and the owner has been told

gpui is not CSS, and two of the system's signatures do not survive the port.

- **Blob radii.** `--blob-1: 48% 52% 46% 54% / 56% 54% 46% 44%` is elliptical
  and expressed in percentages; gpui's corners are four `Pixels`. Asymmetric
  pixel radii get close to the hand-made feel and are not the same thing.
- **The squish.** `Button.prompt.md` is explicit: *"Đừng tự đổi `transform`
  khi active — cú **bóp bẹt** là **chữ ký chuyển động của brand**."* gpui has
  no transform on a div. **The brand's named motion signature is lost here**,
  and no approximation is attempted rather than a bad one shipped.

So caixonho will carry Đất Nặn's material, colour and voice, and not its
playfulness. That is the honest trade and it was stated before starting.

## Two decisions the owner made, because the system says to ask

`REPO-NOTES.md`: *"Gặp ô trống trong bảng trên thì **hỏi owner, đừng tự chế**."*

- **Dark mode is dropped.** Đất Nặn has one light theme; dark is recorded as
  *"owner chọn để sau"*. caixonho ships light and dark today, and the owner
  chose to drop dark rather than have this change invent a palette the system
  does not have. It comes back through Claude Design, not through here.
- **Both fonts are embedded**, with their OFL licences, rather than the lighter
  display-only option.

## `[M]` requirements

Delivers none. `PROJECT_BRIEF.md` names no visual requirement, and this claims
none. Unbuilt `[M]` ahead of it is unchanged: signing in to IAM Identity Center
from the app (§4.1).

This goes first anyway, and the reason is the owner's own: the Local Zone
browsing they built this for is *"đủ xài rồi"*, and what is left is making the
thing feel finished. `XONHO-0031` runs beside it and its window tier is
deliberately parked until after — writing screenshot frames twice would be
waste.
