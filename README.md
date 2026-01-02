# RustyCut 🦀✂️

**RustyCut** to nowoczesny, lekki i szybki edytor wideo napisany w języku **Rust**, wykorzystujący moc **FFmpeg**. 

Projekt jest obecnie w fazie **Open Beta**. Stawiamy na wydajność, minimalizm i profesjonalny workflow (inspirowany DaVinci Resolve).

![RustyCut Preview](https://via.placeholder.com/800x450.png?text=RustyCut+Screenshot+Here)

## 🆕 Ostatnie Zmiany (Update 0.2.0)

*   **Smart Playback:** Płynne odtwarzanie mimo luk na osi czasu (Auto-Black & Silence) - brak zacięć!
*   **Bezpieczny Blade Tool:** Blokada przesuwania klipów podczas używania narzędzia cięcia (zapobiega przypadkowym ruchom).
*   **Pełna Lokalizacja:** 100% wsparcia dla PL/EN (w tym komunikaty błędów, puste stany i modale).
*   **UX Improvements:** Poprawione centrowanie okien i responsywność interfejsu.

## ✨ Główne Funkcje

*   **🚀 Wydajność Rusta:** Błyskawiczne działanie bez zbędnego narzutu.
*   **✂️ Blade Mode (Narzędzie Cięcia):** Precyzyjne cięcie klipów z unikalnym kursorem "Razor". Skrót klawiszowy: `B`.
*   **🌊 Ripple Delete:** Inteligentne usuwanie klipów z automatycznym przesuwaniem pozostałych elementów (zamykanie luk).
*   **🔊 Audio Masking:** Automatyczne wyciszanie dźwięku w lukach między klipami.
*   **🎬 Live Fading:** Podgląd efektów Fade In/Out w czasie rzeczywistym (nawet podczas przewijania!).
*   **🖥️ Nowoczesny UI:** Ciemny motyw, dwukolumnowy układ i dokowalne panele.
*   **📂 System Projektów:** Zapisz i wznów pracę dzięki formatowi `.rev` (JSON).

## 🛠️ Wymagania

*   **Rust** (najnowsza wersja stable)
*   **FFmpeg** (zainstalowany i dostępny w zmiennej środowiskowej `PATH`)

## 🚀 Jak uruchomić?

1.  Sklonuj repozytorium:
    ```bash
    git clone https://github.com/szansky/RustyCut-.git
    cd RustyCut-
    ```

2.  Uruchom projekt:
    ```bash
    cargo run
    ```

## ⌨️ Skróty Klawiszowe

| Klawisz | Akcja |
| :--- | :--- |
| `Space` | Play / Stop |
| `A` | Tryb Wyboru (Hand Tool) |
| `B` | Tryb Cięcia (Blade Tool) |
| `PPM` | Menu kontekstowe (na klipie) |

## 🤝 Kontrybucja

To projekt Open Source! Zapraszamy do zgłaszania błędów (Issues) i przesyłania poprawek (Pull Requests).

---
*RustyCut - Made with ❤️ in Rust.*
