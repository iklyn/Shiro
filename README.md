# Shiro

A save-it-all app for your Mac.

Most bookmarking apps are one or more of these:
1. doesn't support all files
2. confined to browser
3. shit ui
4. complex
5. subscription

A bookmarking app has one job — save everything, and give it back when you want it. Shiro does exactly that and more.

This is a simple, practical, & beautiful app that can save any file / text / image, from anywhere. Everything is stored locally on your Mac.

**No login. No internet. No charge.** And it's free forever.

## How it works

- Press one shortcut (default **⌘E**) anywhere on your Mac. A small capture pill pops up, prefilled with whatever you had selected — hit **Enter** to save.
  - **Selected text** is saved with its formatting and a link back to the source page.
  - Hold **Shift** with the shortcut to grab a **screenshot** region instead.
- **Drag any file** onto the window — or click the logo to pick files.
- **Search, browse, and open** everything from the main window. Shiro lives in your menu bar.

Your captures are written as plain **Markdown files plus the original files**, in a folder you choose (default `~/Desktop/Shiro`). Open and read them in any app — Shiro just keeps a fast search index alongside them.

Shiro never makes a network request. Web-hosted images in a clipping are kept as a link (their pixels live on the web, and Shiro won't fetch them); self-contained images are saved in full.

## Permissions

Shiro asks for exactly two macOS permissions, once, during onboarding:

- **Accessibility** — to read the text you select when you press the shortcut.
- **Screen Recording** — only used when you take a screenshot.

That's it. No surprise "Automation" prompts, no background access.

## Requirements

- macOS on Apple Silicon (M-series).

## Build from source

```bash
npm install
npm run tauri dev      # run in development
npm run tauri build    # build Shiro.app
```

The built app lands in `src-tauri/target/release/bundle/macos/Shiro.app`.
