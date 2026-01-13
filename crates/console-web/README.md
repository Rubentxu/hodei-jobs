# Hodei Operator Web Dashboard

A Leptos-based WebAssembly dashboard for configuring and monitoring the Hodei Operator.

## Quick Start

### Build from source

```bash
# Install wasm32 target
rustup target add wasm32-unknown-unknown

# Install wasm-bindgen
cargo install wasm-bindgen-cli

# Build
./build.sh
```

### Serve locally

```bash
python3 -m http.server 8080 --directory assets
```

Then open http://localhost:8080 in your browser.

## Docker

```bash
# Build image
docker build -t hodei/operator-web .

# Run container
docker run -p 8080:80 hodei/operator-web
```

Then open http://localhost:8080 in your browser.

## Features

- **Dashboard**: Overview of jobs, worker pools, and providers
- **Providers**: Manage worker providers (Docker, Kubernetes, Firecracker)
- **Worker Pools**: Create and configure worker pool templates
- **Jobs**: Monitor job executions
- **Settings**: Configure operator settings

## Architecture

- **Framework**: Leptos 0.6 with WebAssembly
- **Routing**: Leptos Router (SPA)
- **Styling**: CSS with custom properties
- **Build**: cargo + wasm-bindgen

## Files

```
crates/operator-web/
├── Cargo.toml           # Dependencies
├── build.sh            # Build script
├── Dockerfile          # Container image
├── assets/
│   ├── index.html      # Entry HTML
│   ├── style.css       # Styles
│   └── pkg/            # WASM + JS bindings
└── src/
    ├── lib.rs          # App entry point
    ├── types.rs        # Domain types
    ├── components/     # UI components
    └── pages/          # Page components
```
