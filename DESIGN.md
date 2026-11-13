---
name: Dettino website
description: Design system for the public website (site/) only, the "Partenze" world; the desktop app in apps/desktop keeps its own UI.
colors:
  cobalt: "#2F5FDB"
  cobalt-lo: "#2551C6"
  cobalt-ink: "#2348B0"
  on-cobalt: "#FFFFFF"
  on-cobalt-2: "#E4EAFC"
  signal: "#F5C518"
  strike: "#FF5A4E"
  line-live: "#6A90F5"
  paper: "#EEF1F5"
  paper-2: "#E2E6EE"
  white: "#FAFBFD"
  ink: "#111317"
  ink-2: "#434957"
  rule: "#CBD0D8"
  rule-2: "#9DA5B3"
  board: "#131418"
  board-2: "#18191E"
  flap: "#202227"
  flap-lo: "#1B1D21"
  flap-text: "#F1F0EA"
  flap-dim: "#8D919A"
  board-line: "#2B2E35"
  board-label: "#A6AAB3"
typography:
  display:
    fontFamily: "Archivo, ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "clamp(52px, 7.4vw, 96px)"
    fontWeight: 820
    lineHeight: 0.9
    letterSpacing: "-0.012em"
    fontVariation: "\"wdth\" 70"
  display-notice:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(40px, 5vw, 72px)"
    fontWeight: 820
    lineHeight: 0.92
    letterSpacing: "-0.008em"
    fontVariation: "\"wdth\" 70"
  headline-sign:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(32px, 3.4vw, 48px)"
    fontWeight: 820
    lineHeight: 0.96
    letterSpacing: "-0.004em"
    fontVariation: "\"wdth\" 72"
  headline-sub:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(30px, 3vw, 40px)"
    fontWeight: 820
    lineHeight: 0.94
    letterSpacing: "-0.008em"
    fontVariation: "\"wdth\" 72"
  title-timetable:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "30px"
    fontWeight: 800
    lineHeight: 1
    fontFeature: "\"tnum\" 1"
    fontVariation: "\"wdth\" 75"
  title:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "21px"
    fontWeight: 700
    lineHeight: 1.2
    fontVariation: "\"wdth\" 90"
  body:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.55
    fontVariation: "\"wdth\" 100"
  lead:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1.55
  label-ui:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 650
    lineHeight: 1.1
    letterSpacing: "0.005em"
    fontVariation: "\"wdth\" 92"
  label-column:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 560
    letterSpacing: "0.11em"
  flap:
    fontFamily: "Archivo, ui-sans-serif, system-ui, sans-serif"
    fontSize: "25px"
    fontWeight: 640
    lineHeight: 1
    fontFeature: "\"tnum\" 1"
    fontVariation: "\"wdth\" 82"
rounded:
  tile: "2px"
  flap: "3px"
  sm: "4px"
  key: "7px"
  plate: "8px"
  pick: "10px"
  sign: "12px"
  card: "14px"
  board: "16px"
  enamel: "22px"
spacing:
  gutter: "clamp(16px, 4vw, 48px)"
  wrap: "1280px"
  column-gap: "clamp(16px, 2.4vw, 32px)"
  section: "clamp(72px, 9vw, 128px)"
  board-inset: "24px"
  row: "22px"
  stack: "12px"
components:
  plate:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    typography: "{typography.label-ui}"
    rounded: "{rounded.plate}"
    padding: "0 22px"
    height: "52px"
  plate-primary:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.on-cobalt}"
    typography: "{typography.label-ui}"
    rounded: "{rounded.plate}"
    padding: "0 22px"
    height: "52px"
  plate-primary-hover:
    backgroundColor: "{colors.cobalt-lo}"
    textColor: "{colors.on-cobalt}"
  band-cta:
    backgroundColor: "{colors.on-cobalt}"
    textColor: "{colors.cobalt-ink}"
    rounded: "{rounded.key}"
    padding: "0 16px"
    height: "38px"
  band-cta-hover:
    backgroundColor: "{colors.on-cobalt-2}"
    textColor: "{colors.cobalt-ink}"
  sign:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.on-cobalt}"
    typography: "{typography.headline-sign}"
    rounded: "{rounded.sign}"
    padding: "20px 26px 22px"
  route-pick:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.pick}"
    padding: "8px 18px 8px 8px"
  route-pick-active-badge:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.on-cobalt}"
    rounded: "{rounded.plate}"
    size: "40px"
  board:
    backgroundColor: "{colors.board}"
    textColor: "{colors.flap-text}"
    rounded: "{rounded.board}"
  flap-cell:
    backgroundColor: "{colors.flap}"
    textColor: "{colors.flap-text}"
    typography: "{typography.flap}"
    rounded: "{rounded.flap}"
    width: "26px"
    height: "42px"
  flap-cell-signal:
    backgroundColor: "{colors.flap}"
    textColor: "{colors.signal}"
  enamel-notice:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.on-cobalt}"
    rounded: "{rounded.enamel}"
    padding: "12px"
  platform-card:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
    padding: "22px"
  platform-plate:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.on-cobalt}"
    rounded: "{rounded.sign}"
    size: "92px"
  kbd:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.key}"
    padding: "5px 11px 6px"
---

# Design System: Dettino website

Scope: the public website in `site/` (index and privacy pages). The desktop app keeps its own UI and is not governed by this file. Brand commitments shared with the app: the speech-bubble "d" mark, Cobalt as the brand accent, and the light Italian voice.

## Overview

**Creative North Star: "Partenze"**

A daylight Italian station concourse. Cool timetable paper with blue-grey rules is the ground; cobalt enamel signage carries structure (the header and footer band, section signs, the privacy notice, platform plates); one black Solari split-flap board is the single dark object, with off-white mixed-case flaps split by a hinge line. Every dictation is shown as a departure: words flap into clean text and leave for an app.

Density is calm and editorial. Sections sit on a 12-column grid: a cobalt sign and short paragraph hold the left four columns, and a list ruled like a timetable fills the remaining eight. Colour is rationed by role: cobalt means signage or action, yellow means a live value on a flap, ink means reading. One typeface, Archivo, does everything by moving along its width axis.

Motion belongs to the board: flaps rotate in two halves (76 ms per half-step), cascade with an 8 to 9 ms stagger and up to 40 ms jitter, and rows fade out when replaced. Reduced motion lands every cell instantly; the page is complete at rest.

**Key Characteristics:**
- Cool paper ground, cobalt enamel signs, one black split-flap board.
- Archivo variable only, condensed heavy for signs, normal width for reading.
- Yellow appears only on split-flap cells.
- 3px ink top rule opens every content column; 1px blue-grey rules separate rows.
- Enamel objects carry an inset white rule and pressed-metal relief; paper objects are flat and bordered.

## Colors

A cool, low-chroma paper and ink palette with one saturated enamel blue and one rationed signal yellow.

### Primary
- **Enamel Cobalt** (cobalt): the brand accent and the colour of every sign: header and footer band, section signs, privacy enamel, platform plates, primary plates, active route line and badge, focus ring, text selection, the live mic dot on the hold key.
- **Pressed Cobalt** (cobalt-lo): hover state of primary plates and the border of primary plates.
- **Cobalt Ink** (cobalt-ink): cobalt when it has to read as text on paper: body links, timetable stop names, the header Download label, FAQ summary hover.
- **On-Enamel White** (on-cobalt) and **Enamel Mist** (on-cobalt-2): text on cobalt; the mist is for secondary text on enamel and the Download plate hover.

### Secondary
- **Solari Yellow** (signal): live values on split-flap cells only: the live row time and status, the header clock, the measured-time tiles in the timetable.
- **Live Line Blue** (line-live): the board's own focus ring and the pill line while listening; a lighter cobalt that holds contrast on the black board.

### Neutral
- **Timetable Paper** (paper): page background. **Paper Shade** (paper-2): inline code ground on the reading page.
- **Plate White** (white): raised paper objects: plates, keys, route pickers, the map, platform cards.
- **Ink** (ink): headings, strong text, the 3px column rules. **Ink Grey** (ink-2): body and secondary text.
- **Rule** (rule): 1px row dividers, card and map borders, idle map lines. **Rule Dark** (rule-2): plate and key borders, idle map stations, scrollbar.
- **Board Black** (board), **Live Row** (board-2): the board body and its live row. **Flap** (flap) and **Flap Underside** (flap-lo): the top and bottom halves of every cell. **Flap White** (flap-text), **Flap Dim** (flap-dim): flap characters; dim marks the idle pill. **Strike Red** (strike): dropped filler words only, both the letters and the 3px line through them. **Board Line** (board-line), **Board Label** (board-label): board row rules and column labels.

### Named Rules
**The Signal Ration Rule.** Yellow lives only on a flap cell. It never fills paper, enamel, text or a button.

**The Enamel Carries Structure Rule.** A cobalt fill means signage or the primary action. Cobalt text on paper always uses Cobalt Ink, never the fill value.

## Typography

**Display Font:** Archivo variable (wdth 62 to 125, wght 100 to 900), self-hosted, with ui-sans-serif and system-ui fallback
**Body Font:** Archivo at 100% width
**Label/Mono Font:** Archivo; a system monospace stack appears only for inline code on the reading page

**Character:** One family pulled across its width axis: condensed heavy reads as station signage and flaps, normal width reads as calm prose.

### Hierarchy
- **Display** (820, wdth 70%, clamp 52 to 96px, 0.9): the hero headline, ink on paper, with a blinking cobalt caret.
- **Display Notice** (820, wdth 70%, clamp 40 to 72px, 0.92): the privacy enamel headline.
- **Headline Sign** (820, wdth 72%, clamp 32 to 48px, 0.96): section titles set inside cobalt signs; 44 to 68px for the reading page title sign.
- **Headline Sub** (820, wdth 72%, clamp 30 to 40px): a secondary section title on paper under a 3px ink rule. Platform names use the same voice at 40px.
- **Title Timetable** (800, wdth 75%, 30px, tabular): timetable stop names in Cobalt Ink; 26px for the small-things list; 32px for reading-page h2.
- **Title** (700, wdth 90%, 21px, 1.2): list item titles. FAQ summaries use 680 at 21px.
- **Body** (400, 17px, 1.55): all reading text, capped at 62 to 64ch in lists and 34ch beside signs. **Lead** is 18px in Ink Grey, max 46ch.
- **Label UI** (650, wdth 92%, 16px): plates; nav links are 520 at 15px.
- **Label Column** (560, 11.5px, 0.11em, uppercase): board column heads. Map zone labels and table heads use the same treatment at 12px.
- **Flap** (640, wdth 82%, tabular): split-flap characters, 25px on the live row, 14px on history rows.

### Named Rules
**The Width Axis Rule.** Hierarchy moves on width before size: 70 to 75% for signs and display, 82% for flaps, 88 to 92% for UI labels, 100% for reading.

**The Tabular Time Rule.** Every time, duration and clock digit sets in tabular figures.

## Layout

Content sits in a 1280px wrap with a fluid gutter (16 to 48px). Sections use a 12-column grid with a fluid column gap (16 to 32px) and a fluid top padding (72 to 128px). The section pattern is a left header over columns 1 to 4 (sign plus a paragraph of 34ch or less) and content over columns 5 to 12, opened by a 3px ink rule. Below 960px everything stacks to full width. List rows pair a fixed label column (150px for timetable stops, 250px for key combos and small things) with the text; below 560px they stack.

The hero is a two-column grid (1.25fr / 1fr) with the headline and the lead plus plates aligned to a shared baseline, then the board at full content width. The header band is sticky at 62px and section anchors offset by the same 62px. The board uses its own five-column grid (time, destination, text, route, status) that drops to four columns at 1100px and two at 760px; flap sizes step down with it.

## Elevation & Depth

A hybrid. Paper surfaces are flat and separated by borders and rules. Enamel objects carry relief: an inset white highlight on top, an inset shade on the bottom, and a soft cast shadow. The board casts the deepest shadow because it is the only heavy object. Shadows are always soft, cool and negative-spread; no hard offset shadows.

### Shadow Vocabulary
- **Enamel relief** (`box-shadow: inset 0 1px 0 rgba(255,255,255,.25), inset 0 -2px 0 rgba(0,0,0,.18), 0 14px 26px -18px rgba(14,17,28,.55)`): section signs; the privacy enamel uses a deeper two-layer cast.
- **Board cast** (`box-shadow: 0 40px 70px -40px rgba(14,17,28,.55), 0 10px 24px -12px rgba(14,17,28,.35)`): the split-flap board only.
- **Band** (`box-shadow: 0 1px 0 rgba(14,17,28,.25), 0 6px 18px -10px rgba(14,17,28,.5)`): the sticky header band.
- **Plate rest / hover** (`0 1px 2px rgba(17,19,23,.08)` to `0 4px 12px -4px rgba(17,19,23,.2)`): paper plates lift slightly on hover.

### Named Rules
**The Enamel Relief Rule.** Only cobalt enamel and the board carry depth; paper cards, the map and lists stay flat with a 1px Rule border.

## Shapes

Softly rounded, sign-like rectangles. Radius grows with object size: 2 to 3px for flap tiles, 4px for small badges and code, 7 to 8px for keys and plates, 10px for route pickers, 12px for signs and the map, 14px for platform cards, 16px for the board, 22px for the privacy enamel. Enamel signs carry a 2px white inset rule (an outline drawn 8px inside the edge, or a bordered inner panel on the notice), which reads as a painted border on a metal plate. Every split-flap cell carries a 1px dark hinge line across its middle. Rules are straight and full-width: 3px ink to open a column, 1px Rule between rows.

## Components

### Buttons (plates)
Solid, confident, sign-like.
- **Shape:** gently rounded (8px), 52px tall, 22px side padding, optional 19px stroke icon.
- **Primary:** Enamel Cobalt with a Pressed Cobalt border, white label, inset highlight and soft cast.
- **Secondary:** Plate White with a 1px Rule Dark border and Ink label.
- **Hover / Focus:** primary darkens to Pressed Cobalt; secondary brightens and lifts. Active presses down 1px. Focus is a 2px cobalt outline offset 3px (white on enamel).
- **Band Download:** a white 38px plate on the cobalt band with a Cobalt Ink label.

### Chips (route picker)
- **Style:** Plate White pill-rectangle (10px) with a 40px square line badge (letter in condensed heavy, 2px ink border) and a two-line label.
- **State:** the selected route gets a cobalt border plus 1px cobalt ring and a filled cobalt badge. It drives the route map, where the active line thickens to 9px in cobalt.

### Cards / Containers
- **Corner Style:** 14px (platform cards), 12px (map).
- **Background:** Plate White on paper.
- **Shadow Strategy:** flat; see Elevation.
- **Border:** 1px Rule.
- **Internal Padding:** 22px.

### Navigation
The cobalt band holds the mark and the condensed wordmark (750, wdth 88%, 21px) on the left, section links (15px, 520, white) on the right, and the Download plate at the end. Hover underlines. Below 760px the links hide and the Download plate stays. The footer reuses the band.

### Split-Flap Board (signature)
A black 16px-radius slab with a cobalt header plate ("Partenze", condensed heavy 34px) and a yellow flap clock in an inset black well. Rows are 52px tall with board-line rules; the live row sits on Live Row and prints in large flaps. Each cell is two halves (Flap over Flap Underside) with a hinge line, and flips by rotating the top half down and the bottom half in. Dropped filler words turn Strike Red and get a red line through them; history rows are buttons that flip back to what was said. The foot holds a physical-looking Ctrl + Space key (light keycap with a 4px bottom edge that compresses when held) and the one-line pill drawn as a stroke.

### Enamel Notice
A large cobalt panel (22px) with a 2px white inset border, a display headline on the left and a facts list on the right, separated by translucent white rules.

### Platform Plate
A 92px cobalt square sign with a white inset rule, a condensed heavy number and a small uppercase "Binario" label, set inside a white platform card.

### Keys
Inline keycaps: Plate White, 1px Rule Dark border with a heavier bottom edge (2 to 3px), wdth 90%, 600 weight.

## Do's and Don'ts

### Do:
- **Do** put section titles on cobalt signs with the white inset rule, and open every content column with a 3px ink rule.
- **Do** keep yellow on flap cells: live time, status, clock and measured-time tiles.
- **Do** set every time and duration in tabular figures, and keep flap text mixed case.
- **Do** step hierarchy along Archivo's width axis (70 to 75% signs, 82% flaps, 100% reading) before reaching for new sizes.
- **Do** land every flap instantly under reduced motion; the board must read complete at rest.
- **Do** keep the speech-bubble "d" mark in the band lockup.

### Don't:
- **Don't** introduce a second typeface or a system display face; Archivo carries every role.
- **Don't** use yellow on paper, enamel, text or buttons.
- **Don't** add a second dark board-like slab; flap tiles on paper are small quotes of the one board.
- **Don't** give paper cards or lists shadows; depth belongs to enamel and the board.
- **Don't** use hard offset shadows or gradients.
- **Don't** set uppercase tracked labels above headings as kickers; uppercase is only for board column heads, map zone labels, table heads and plate labels.
