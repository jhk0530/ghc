# ghc - GitHub Copilot CLI Desktop Application

<img src="images/banner.png" width="100%"/>

## Description

Use GitHub Copilot CLI with a desktop application.

## Features

<img src="images/screenshot.png" width="100%"/>

- Automatic Install of GitHub Copilot CLI
- GitHub Authentication

- Various AI Model
- Usage check

- File as context
- Export Response as clipboard
- Chat History

## Architectures

```mermaid
%%{init: {'theme':'base', 'themeVariables': { 'primaryColor': '#0b5fff', 'secondaryColor': '#0ea5a4', 'tertiaryColor': '#0f1724', 'background':'#0b1220', 'edgeLabelBackground':'#e6eef1', 'fontFamily':'Inter, Arial', 'fontSize':'14px'}}}%%
flowchart TD
    classDef cloud fill:#0f1724,stroke:#60a5fa,stroke-width:2,color:#ffffff,font-weight:600;
    classDef app fill:#0ea5a4,stroke:#035e4b,stroke-width:2,color:#022927,font-weight:600;
    classDef code fill:#f8fafc,stroke:#1f2937,stroke-width:1,color:#111827;
    classDef file fill:#fff7ed,stroke:#f97316,stroke-width:1,color:#92400e;
    classDef cli fill:#111827,stroke:#ef4444,stroke-width:2,color:#fff,font-weight:700;

    A[User]:::cloud -->|interacts| B[ghc]:::app

    subgraph Tauri["Tauri — Rust + Web"]
      direction TB
      Rust[Rust]:::code
      lib["src-tauri/lib.rs"]:::file
      main_rs["src-tauri/main.rs"]:::file
      HTML[HTML]:::code
      index["index.html"]:::file
      CSS[CSS]:::code
      css["src/styles.css"]:::file
      TS[TypeScript]:::code
      ts["src/main.ts"]:::file

      Rust --> lib
      Rust --> main_rs
      HTML --> index
      CSS --> css
      TS --> ts
    end

    B --> Tauri
    Tauri --> C[GitHub Copilot CLI]:::cli

    linkStyle default stroke:#94a3b8, stroke-width:1.5, stroke-dasharray:0;
```

## Installation

[![Windows](https://custom-icon-badges.demolab.com/badge/-Windows-0078D6?style=for-the-badge&logo=windows11)](https://github.com/jhk0530/ghc/releases/download/v0.1.0/ghc_0.1.0_x64_en-US.msi)

[![MacOS](https://img.shields.io/badge/-MacOS-0078D6?style=for-the-badge&logo=apple)](https://github.com/jhk0530/ghc/releases/download/v0.1.0/ghc_0.1.0_aarch64.dmg)

## Pre-requisites

- GitHub Account with Copliot enabled, Education account is recommended.
