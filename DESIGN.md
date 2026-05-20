# Design System: T2W Industrial Studio
**Project ID:** Local Stitch export `stitch_t2w_artifact_studio (3)`

## 1. Visual Theme & Atmosphere
T2W Industrial Studio is a dense, technical, amber-on-charcoal workbench for generated HTML artifacts. The mood is precise, operational, and IDE-like: a fixed shell frames a bright generated-output canvas while the right utility panel exposes prompt, data, template, and run context without becoming decorative.

The interface should feel like an artifact control room rather than a marketing dashboard. It uses rigid panes, compact type, low-contrast outlines, and one warm amber signal color to keep attention on execution state and primary actions.

## 2. Color Palette & Roles
- **Deep Charcoal Canvas (#121314):** Primary application background and workspace field.
- **Blackened Workspace Floor (#0d0e0f):** Lowest-depth surface behind the fluid center canvas.
- **Raised Charcoal Panel (#1f2021):** App bar, utility panel, and major fixed chrome surfaces.
- **High Charcoal Control Surface (#292a2b):** Hovered or emphasized controls, recessed readouts, and secondary panel layers.
- **Highest Charcoal Input Well (#343536):** Active readout containers and dense technical blocks.
- **Warm Industrial Amber (#f59e0b):** Primary actions, active rail/tab indicators, and live/completed status dots.
- **Soft Amber Text (#ffc174):** Branded t2w marks and high-visibility labels on dark chrome.
- **Toasted Border (#534434):** Low-contrast structural outlines between fixed panes.
- **Muted Technical Copy (#d8c3ad):** Secondary labels and contextual descriptions.
- **Light Artifact Surface (#e3e2e3):** Preview canvas background for generated HTML, clearly separating output from shell chrome.
- **Dense Artifact Text (#303031):** Text color inside light preview-like surfaces.
- **Error Tint (#ffb4ab) and Error Container (#93000a):** Failed run states and error metadata.

## 3. Typography Rules
Use a self-contained font stack that approximates the Stitch intent without external loading. The main interface uses modern humanist sans faces such as Aptos, Bahnschrift, Segoe UI Variable, and Helvetica Neue. Technical labels, prompt readouts, metadata, and dense values use Cascadia Code, JetBrains Mono, Consolas, or another local monospace.

Headlines are compact and semi-bold with slightly tight letter spacing. Labels are small, monospaced, and often uppercase or title-cased to reinforce CLI lineage. Body copy stays short and operational.

## 4. Component Stylings
* **Buttons:** Soft-square 4px corners. Primary buttons are filled Warm Industrial Amber (#f59e0b) with dark text. Secondary buttons use transparent charcoal surfaces with Toasted Border (#534434), becoming brighter on hover.
* **Fixed Rail:** A 64px vertical rail on desktop with square icon buttons. The active item uses an amber left or side indicator and amber icon text.
* **App Bar:** A 56px fixed top bar spans between the rail and utility panel. It carries brand, template, status, export, run, and compact icon controls.
* **Preview Canvas:** A bright Light Artifact Surface (#e3e2e3) frame inside the dark workspace. The generated HTML is loaded into a sandboxed iframe or equivalent generic artifact container.
* **Utility Tabs:** Prompt, Data, Theme, and Run tabs use equal-width controls with icon-over-label rhythm where space allows. The active tab uses a 2px amber underline.
* **Readouts:** Prompt and data blocks are recessed charcoal wells with monospaced text, tight line height, and subtle borders.
* **Status Indicators:** Execution status uses a small amber dot for ready/streaming/completed and an error tint for failed.

## 5. Layout Principles
The layout follows a fixed-shell / fluid-center model. Desktop uses a 64px left rail, a 56px app bar, a 256px right utility panel, and a fluid center canvas with 24px internal workspace padding. Spacing follows a 4px grid with compact 8px/12px vertical rhythm inside panels.

At narrow widths, the rail collapses away, the app bar wraps, the preview remains first, and the utility panel stacks beneath it. The generated artifact must remain visually separate from the shell at every breakpoint.
