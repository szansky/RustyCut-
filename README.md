# RustyCut 🦀✂️

**RustyCut** is a modern, lightweight, and fast video editor written in **Rust**, leveraging the power of **FFmpeg**.

The project is currently in **Open Beta**. We focus on performance, minimalism, and a professional workflow (inspired by DaVinci Resolve).

![RustyCut Preview](https://via.placeholder.com/800x450.png?text=RustyCut+Screenshot+Here)

## 🆕 Recent Updates (v0.2.0)

*   **Visual Timeline:** Creates filmstrips (thumbnails) for videos and waveforms for audio tracks (Davinci Resolve style).
*   **Media Library Grid:** New responsive grid layout for media assets with improved card styling.
*   **Smart Playback Engine:** Fixed playback for library-only projects and improved gap handling.
*   **Safe Blade Tool:** Clip dragging is disabled when using the cut tool (prevents accidental moves).
*   **Full Localization:** 100% support for PL/EN (including error messages, empty states, and modals).
*   **UX Improvements:** Improved window centering and interface responsiveness.

## ✨ Key Features

*   **🚀 Rust Performance:** Blazing fast performance with no unnecessary overhead.
*   **✂️ Blade Mode:** Precise clip cutting with a unique "Razor" cursor. Keyboard shortcut: `B`.
*   **🌊 Ripple Delete:** Intelligent clip removal that automatically shifts remaining elements (closes gaps).
*   **🔊 Audio Masking:** Automatic audio silencing in gaps between clips.
*   **🎬 Live Fading:** Real-time preview of Fade In/Out effects (even while scrubbing!).
*   **🖥️ Modern UI:** Dark theme, two-column layout, and dockable panels.
*   **📂 Project System:** Save and resume work thanks to the `.rev` (JSON) format.
*   **📦 Media Library:** Import and organize multiple video, audio, and image assets.

## 🛠️ Requirements

*   **Rust** (latest stable version)
*   **FFmpeg** (installed and available in the `PATH` environment variable)

## 🚀 How to Run?

1.  Clone the repository:
    ```bash
    git clone https://github.com/szansky/RustyCut-.git
    cd RustyCut-
    ```

2.  Run the project:
    ```bash
    cargo run
    ```

## ⌨️ Keyboard Shortcuts

| Key | Action |
| :--- | :--- |
| `Space` | Play / Stop |
| `A` | Selection Mode (Hand Tool) |
| `B` | Cut Mode (Blade Tool) |
| `RMB` | Context Menu (on clip) |

## 🤝 Contribution

This is an Open Source project! We welcome Issue reports and Pull Requests.

---
*RustyCut - Made with ❤️ in Rust.*
