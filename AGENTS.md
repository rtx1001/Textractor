# Textractor project preferences

- Ship portable executables only: `npm run tauri build -- --no-bundle`, never installers.
- Preserve the size-optimized release build, static MSVC runtime, and PNG/BMP-only image support.
- Accent: pale orange `#F2BE8D`, dark warm-brown surfaces. Never return to green.
- All app-owned controls must match the theme. Never use native dropdowns/color pickers, default tooltips, number spinners, or Windows/default scrollbars. Use custom accessible controls and themed scrollbars, visible only when content actually overflows.
- Keep settings compact, readable, and fitted to the small window without page scrolling.
- Derive translation pack installation from source/target selections. Do not add a separate pack picker. Downloads remain opt-in and original text stays available.
- Add explanatory custom-styled tooltips, keyboard navigation, and Escape dismissal.
- Verify translation installation plans, compact UI fit, OCR/translation regressions, and portable startup.
