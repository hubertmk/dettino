---
name: Dettino website
description: Design system for the public website (site/) only, the "Partenze" world; the desktop app in apps/desktop keeps its own UI.
colors:
  surf: "#2EC4A0"
  surf-lo: "#26B08E"
  on-surf: "#0B0D0C"
  on-surf-2: "#163A30"
  tint: "#D3F4EA"
  voice: "#2F5FDB"
  strike: "#D9472F"
  ok: "#2EC4A0"
  paper: "#F4EEE2"
  paper-2: "#EAE2D2"
  white: "#FFFCF6"
  ink: "#0B0D0C"
  ink-2: "#444947"
  rule: "#DCD3C1"
  rule-2: "#A99F8C"
  desk: "#E5DCCB"
  press: "#C9F1E4"
  heard: "#7F857C"
typography:
  display:
    fontFamily: "Saira, ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif"
    fontSize: "clamp(42px, 4.9vw, 70px)"
    fontWeight: 700
    lineHeight: 0.94
    letterSpacing: "-0.02em"
    fontVariation: '"wdth" 104'
  display-notice:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(36px, 4.2vw, 60px)"
    fontWeight: 700
    lineHeight: 0.96
    letterSpacing: "-0.02em"
    fontVariation: '"wdth" 100'
  headline-sign:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(34px, 3.6vw, 50px)"
    fontWeight: 700
    lineHeight: 0.98
    letterSpacing: "-0.02em"
    fontVariation: '"wdth" 100'
  headline-sub:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(27px, 2.6vw, 34px)"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.015em"
    fontVariation: '"wdth" 100'
  title-timetable:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.01em"
    fontFeature: "\"tnum\" 1"
    fontVariation: '"wdth" 100'
  title:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 650
    lineHeight: 1.25
    fontVariation: '"wdth" 100'
  body:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.55
    fontVariation: "\"wdth\" 100"
  lead:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1.55
  label-ui:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "0.01em"
    fontVariation: '"wdth" 104'
  label-column:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 560
    letterSpacing: "0.14em"
  demo-text:
    fontFamily: "Saira, ui-sans-serif, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  sm: "4px"
  key: "7px"
  plate: "8px"
  pick: "10px"
  sign: "12px"
  card: "14px"
  screen: "18px"
  enamel: "22px"
spacing:
  gutter: "clamp(16px, 4vw, 48px)"
  wrap: "1280px"
  column-gap: "clamp(16px, 2.4vw, 32px)"
  section: "clamp(72px, 9vw, 128px)"
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
    backgroundColor: "{colors.surf}"
    textColor: "{colors.on-surf}"
    typography: "{typography.label-ui}"
    rounded: "{rounded.plate}"
    padding: "0 22px"
    height: "52px"
  plate-primary-hover:
    backgroundColor: "{colors.surf-lo}"
    textColor: "{colors.on-surf}"
  sign:
    backgroundColor: "{colors.surf}"
    textColor: "{colors.on-surf}"
    typography: "{typography.headline-sign}"
    rounded: "{rounded.sign}"
    padding: "20px 26px 22px"
  route-pick:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.pick}"
    padding: "8px 18px 8px 8px"
  route-pick-active-badge:
    backgroundColor: "{colors.surf}"
    textColor: "{colors.on-surf}"
    rounded: "{rounded.plate}"
    size: "40px"
  demo-screen:
    backgroundColor: "{colors.desk}"
    rounded: "{rounded.screen}"
    padding: "40px 36px 80px"
  demo-window:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    typography: "{typography.demo-text}"
    rounded: "{rounded.pick}"
  enamel-notice:
    backgroundColor: "{colors.surf}"
    textColor: "{colors.on-surf}"
    rounded: "{rounded.enamel}"
    padding: "12px"
  platform-card:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
    padding: "22px"
  platform-plate:
    backgroundColor: "{colors.surf}"
    textColor: "{colors.on-surf}"
    rounded: "{rounded.sign}"
    size: "92px"
  kbd:
    backgroundColor: "{colors.white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.key}"
    padding: "5px 11px 6px"
---

# Design System: Dettino website

Scope: the public website in `site/` (index and privacy pages). The desktop app keeps its own UI and is not governed by this file. Brand commitments shared with the app: the speech-bubble "d" mark (near-black on Seafoam), Seafoam as the brand accent (the app's default accent, "surf"), blue for the voice while the line listens, and the light Italian voice.

## Overview

**Creative North Star: "Partenze"**

A daylight Italian station concourse by the sea. Warm, sun-bleached sand paper with near-black ink is the ground; seafoam enamel signage carries structure (the header and footer band, section signs, the privacy notice, platform plates). The hero shows the product itself rather than a metaphor: the Ctrl and Space keycaps in the headline press down, and beside them a small screen holds one app window and the app's own line at its foot. Words come in as said, fillers are struck out, and clean text lands at the cursor.

Density is calm and editorial. Sections sit on a 12-column grid: a platform number, an ink headline and a short paragraph hold the left four columns, and a list ruled like a timetable fills the remaining eight. Colour is rationed by role: seafoam is a fill or a mark (signs, buttons, underlines), never thin text; ink means reading; blue is the voice while the line listens; red belongs to the demo's struck filler. One typeface, Saira, does everything: a squared grotesk in the line of Novarese's Eurostile (Turin, 1962), the same Italian industrial vocabulary as Solari's split-flap boards.

Motion belongs to the hero demo, one thing at a time: the headline keys press, the line listens (words arrive every 230 ms), thinks for 900 ms, then fillers strike and fold, rewritten words carry a seafoam highlight for 850 ms, and the line glows seafoam. The line is the app's own pill drawing, ported from apps/desktop/src/pill/engine.ts. The demo plays only while on screen and pauses for a visitor holding the keys. Reduced motion stops autoplay and lands every change instantly; the first example is in the HTML, so the page is complete at rest.

**Key Characteristics:**
- Warm sand ground, near-black ink, seafoam enamel signs, one small screen in the hero.
- Saira variable only: squared bold at normal-to-wide width for signs and display, normal width for reading.
- The headline's keycaps are live: they press with the demo and can be held.
- 3px ink top rule opens every content column; 1px sand rules separate rows.
- Enamel objects carry an inset near-black rule and pressed-metal relief; paper objects are flat and bordered.
- Links are ink with a thick seafoam underline ("Jet Links").

## Colors

"Jet Links" (chosen 2026-11-14): warm sand paper, a neutral near-black ink, and one bright seafoam that only ever appears as a fill or a mark. Blue stays as the voice of the line while it listens; a red strike flashes inside the hero demo.

### Primary
- **Seafoam** (surf): the brand accent and the colour of every sign and mark: header and footer band, platform number badges, privacy enamel, platform signs, ticket stub, primary plates, active route line and badge, link and timetable-stop underlines, claim dots, text selection, the line's glow after a paste (ok), the app icon tile. Seafoam is never used for text: against sand it reaches only 1.9:1.
- **Pressed Seafoam** (surf-lo): hover state of primary plates; the dotted fallback line on the route map.
- **On-Seafoam Ink** (on-surf) and **Deep Kelp** (on-surf-2): text on seafoam (7:1 and 5:1); Deep Kelp is for secondary text on enamel, the footer line and the ticket stub labels.
- **Foam Tint** (tint): the seafoam highlighter behind a rewritten word in the demo, chat bubbles and avatar dots in the demo's app window.

### Secondary
- **Voice Blue** (voice): the line while it listens and the hold button's listening dot, in step with the app's pill. Nothing else.
- **Strike Red** (strike): filler words in the demo only, both the letters and the 2px line through them, for the moment before they fold away.

### Neutral
- **Sand Paper** (paper): page background. **Sand Shade** (paper-2): inline code ground on the reading page and placeholder bars in the demo.
- **Cream Plate** (white): raised paper objects: plates, keys, route pickers, the map, platform cards.
- **Jet Ink** (ink): headings, strong text, links, timetable stops, the 3px column rules, the caret and focus ring. **Ink Grey** (ink-2): body and secondary text.
- **Rule** (rule): 1px row dividers, card and map borders, idle map lines. **Rule Dark** (rule-2): plate and key borders, idle map stations, scrollbar.
- **Desk** (desk): the hero screen's ground, a darker sand so the cream app window reads as sitting on a screen. **Key Press** (press): the seafoam-tinted face of a pressed keycap. **Heard Grey** (heard): words in the demo while they are being said, before cleanup.

### Named Rules
**The Jet Links Rule.** Text is ink, never seafoam. A link is ink with a 3px seafoam underline that turns ink on hover; timetable stops carry the same underline at 4px.

**The Demo Colour Rule.** Strike Red and Voice Blue only appear inside the hero demo (and the hold button's dot), and only for a moment. They never fill paper, enamel, text outside the demo, or a button.

**The Enamel Carries Structure Rule.** A seafoam fill means signage or the primary action, always with On-Seafoam Ink on it.

## Typography

**Display Font:** Saira variable (wdth 50 to 125, wght 100 to 900), self-hosted latin subset, with ui-sans-serif and system-ui fallback
**Body Font:** Saira at 100% width, 400
**Label/Mono Font:** Saira; a system monospace stack appears only for inline code on the reading page and for the Editor example in the hero demo

**Character:** A squared grotesk with Italian industrial lineage (Eurostile, Solari, Olivetti-era product lettering). Squared bold at normal-to-wide width reads as station enamel; normal width at 400 reads as calm prose.

### Hierarchy
- **Display** (700, wdth 104%, clamp 42 to 70px, 0.94, -0.02em): the hero headline, ink on paper, with two live keycaps and a blinking ink caret.
- **Display Notice** (700, wdth 100%, clamp 36 to 60px, 0.96): the privacy enamel headline.
- **Headline Sign** (700, wdth 100%, clamp 34 to 50px, 0.98, -0.02em): section titles; 38 to 58px for the reading page title sign.
- **Headline Sub** (700, wdth 100%, clamp 27 to 34px): a secondary section title on paper under a 3px ink rule. Platform names use the same voice.
- **Title Timetable** (700, wdth 100%, 26px, tabular): timetable stop names in Ink over a 4px seafoam underline; 23px for the small-things list; 28px for reading-page h2.
- **Title** (650, wdth 100%, 20px, 1.25): list item titles. FAQ summaries use 600 at 20px.
- **Body** (400, 17px, 1.55): all reading text, capped at 62 to 64ch in lists and 34ch beside signs. **Lead** is 18px in Ink Grey, max 46ch.
- **Label UI** (600, wdth 104%, 16px, 0.01em): plates; nav links are 500 at 15px.
- **Label Column** (560, 11.5px to 12px, 0.14em, uppercase): map zone labels and table heads.
- **Demo Text** (400, 18px, 1.5): the dictation in the hero's app window; 16px on phones. The window header is 600 at 13px.
- **Wordmark** (700, wdth 112%, 21px): "Dettino" in the band.

### Named Rules
**The Width Axis Rule.** Everything sets at 100 to 112% width; wider is louder. Weight carries the rest: 700 for display and signs, 600 to 650 for titles and labels, 400 for reading.

**The Tabular Time Rule.** Every time, duration and clock digit sets in tabular figures.

## Layout

Content sits in a 1280px wrap with a fluid gutter (16 to 48px). Sections use a 12-column grid with a fluid column gap (16 to 32px) and a fluid top padding (72 to 128px). The section pattern is a left header over columns 1 to 4 (sign plus a paragraph of 34ch or less) and content over columns 5 to 12, opened by a 3px ink rule. Below 960px everything stacks to full width. List rows pair a fixed label column (150px for timetable stops, 250px for key combos and small things) with the text; below 560px they stack.

The hero is a two-column grid (1.05fr / 1fr, vertically centred): headline, lead, the three claims and the plates on the left, the demo screen on the right with a one-line hint under it. Below 960px it stacks, the hint gives way to a hold button with the two keycaps (the headline may be out of view by then), and below 560px the first headline line may wrap. The header band is sticky at 62px and section anchors offset by the same 62px.

## Elevation & Depth

A hybrid. Paper surfaces are flat and separated by borders and rules. Enamel objects carry relief: an inset white highlight on top, an inset shade on the bottom, and a soft cast shadow. The hero screen casts the deepest shadow because it is the only heavy object; the app window inside it carries a soft window shadow. Shadows are always soft, cool and negative-spread; no hard offset shadows.

### Shadow Vocabulary
- **Enamel relief** (`box-shadow: inset 0 1px 0 rgba(255,255,255,.25), inset 0 -2px 0 rgba(0,0,0,.18), 0 14px 26px -18px rgba(14,17,28,.55)`): section signs; the privacy enamel uses a deeper two-layer cast.
- **Screen cast** (`box-shadow: inset 0 1px 0 rgba(255,255,255,.6), 0 30px 60px -44px rgba(14,17,28,.55)`): the hero screen only. **Window** (`0 1px 2px rgba(14,17,28,.06), 0 18px 36px -22px rgba(14,17,28,.45)`): the app window on it.
- **Band** (`box-shadow: 0 1px 0 rgba(14,17,28,.25), 0 6px 18px -10px rgba(14,17,28,.5)`): the sticky header band.
- **Plate rest / hover** (`0 1px 2px rgba(17,19,23,.08)` to `0 4px 12px -4px rgba(17,19,23,.2)`): paper plates lift slightly on hover.

### Named Rules
**The Enamel Relief Rule.** Only seafoam enamel and the hero screen carry depth; paper cards, the map and lists stay flat with a 1px Rule border.

## Shapes

Softly rounded, sign-like rectangles. Radius grows with object size: 4px for small badges and code, 7 to 9px for keys and plates, 10px for route pickers and the demo's app window, 12px for signs and the map, 14px for platform cards, 18px for the hero screen, 22px for the privacy enamel. Enamel signs carry a 2px near-black inset rule (80% ink) (an outline drawn 8px inside the edge, or a bordered inner panel on the notice), which reads as a painted border on a metal plate. Rules are straight and full-width: 3px ink to open a column, 1px Rule between rows.

## Components

### Buttons (plates)
Solid, confident, sign-like.
- **Shape:** gently rounded (8px), 52px tall, 22px side padding, optional 19px stroke icon.
- **Primary:** Seafoam with an On-Seafoam Ink label and a soft seafoam cast. On sand the fill itself is low-contrast (1.9:1); the dark label carries it.
- **Secondary:** Plate White with a 1px Rule Dark border and Ink label.
- **Hover / Focus:** primary darkens to Pressed Seafoam; secondary brightens and lifts. Active presses down 1px. Focus is a 2px ink outline offset 3px, on paper and on enamel alike.

### Chips (route picker)
- **Style:** Plate White pill-rectangle (10px) with a 40px square line badge (letter in 700, 2px ink border) and a two-line label.
- **State:** the selected route gets an ink border plus 1px ink ring and a seafoam badge with an ink border. It drives the route map, where the active line thickens to 9px in seafoam.

### Cards / Containers
- **Corner Style:** 14px (platform cards), 12px (map).
- **Background:** Plate White on paper.
- **Shadow Strategy:** flat; see Elevation.
- **Border:** 1px Rule.
- **Internal Padding:** 22px.

### Navigation
The seafoam band holds the mark (a near-black tile with a seafoam d) and the wide wordmark (700, wdth 112%, 21px) on the left, section links (15px, 500, ink) on the right, then GitHub and the Buy Me a Coffee yellow. Hover underlines in ink. Below 760px the links hide. The footer reuses the band.

### Hero Demo (signature)
- **Headline keys:** the Ctrl and Space keycaps in the headline (Plate White, Rule Dark border with a heavier bottom edge) press down while the demo listens: they drop slightly, the bottom edge compresses, the face turns Key Press with a seafoam ring and the label stays Ink. Visitors can hold them with a mouse or finger, or hold Ctrl + Space on their own keyboard.
- **Screen:** an 18px-radius Desk panel with a Rule Dark border and the screen cast. It holds one app window and the line, nothing else.
- **App window:** Plate White, 10px radius, a Paper header with the app's icon and name on the left and who or what it is for on the right (Mail to Lena, Chat with Martin, Docs, Notes, Editor). Grey placeholder bars stand for what the window already holds. Words arrive in Heard Grey, fillers get a Strike Red line and fold away, rewritten words sit on a Foam Tint highlight for a moment, then everything settles in Ink with an ink caret.
- **The line:** the app's own pill, drawn on a canvas in the app's pixels (170 × 36, scaled to 210px) and docked at the bottom centre of the screen, where it sits in the app. Ink at rest, Voice Blue while listening, a gliding hump while thinking, a seafoam glow when the text is in.
- **Hold button:** below 960px only, two keycaps with a "Hold to talk" label under the screen.

### Enamel Notice
A large seafoam panel (22px) with a 2px near-black inset border, an ink display headline on the left and a facts list on the right in Deep Kelp, separated by translucent ink rules.

### Platform Plate
A 92px seafoam square sign with a near-black inset rule, a 700-weight number and a small uppercase "Binario" label, set inside a white platform card.

### Keys
Inline keycaps: Plate White, 1px Rule Dark border with a heavier bottom edge (2 to 3px), wdth 100%, 600 weight.

## Do's and Don'ts

### Do:
- **Do** keep seafoam to fills and marks (signs, buttons, underlines) with ink on it, and open every content column with a 3px ink rule.
- **Do** set every time and duration in tabular figures.
- **Do** keep display and signs at 700 and 100 to 112% width (the squared shoulders carry the voice), reading at 100%. Step hierarchy with weight and width before reaching for new sizes.
- **Do** keep the hero demo to one window and the line, with one thing moving at a time. Under reduced motion it stops playing by itself and lands every change instantly; the first example sits in the HTML so the hero reads complete at rest.
- **Do** keep the demo's line drawing in step with the app's pill engine.
- **Do** keep the speech-bubble "d" mark in the band lockup.

### Don't:
- **Don't** introduce a second typeface or a system display face; Saira carries every role.
- **Don't** push Saira back to condensed heavy (wdth under 90 at 800+) for headings; that turns it into a generic poster grotesk.
- **Don't** set text in seafoam, and don't use Strike Red or Voice Blue outside the hero demo.
- **Don't** add more to the hero demo: no second window, no history, no clock, no status board. It shows keys, line and text, and nothing else.
- **Don't** give paper cards or lists shadows; depth belongs to enamel and the hero screen.
- **Don't** use hard offset shadows or gradients.
- **Don't** set uppercase tracked labels above headings as kickers; uppercase is only for the hero claims, map zone labels, table heads and plate labels.
