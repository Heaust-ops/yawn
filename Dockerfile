FROM node:22-bookworm-slim AS base

# Install Rust nightly and required tools
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- \
        -y --profile minimal --default-toolchain none \
    && . "$HOME/.cargo/env" \
    && rustup toolchain install nightly-2025-10-20 \
        --profile minimal \
        --component rust-src \
        --target wasm32-unknown-unknown \
    && rustup default nightly-2025-10-20 \
    && curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh \
    && cargo install rsw

# Set environment variables for Rust
ENV PATH="/root/.cargo/bin:$PATH"
ENV RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals"

# Set working directory
WORKDIR /app

# Copy package files
COPY package.json package-lock.json ./

# Install Node.js dependencies
RUN npm ci

# Copy source code
COPY . .

FROM base AS development

# Expose Vite's development server and run the Rust/Vite watchers.
EXPOSE 8080

CMD ["npm", "run", "dev"]

FROM base AS production

# Build the project
RUN npm run build

# Expose port for serving
EXPOSE 8080

# Command to serve the built application
CMD ["npm", "run", "start", "--", "--host", "0.0.0.0"]
