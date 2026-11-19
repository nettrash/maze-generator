# Build stage. The patch version matches rust-toolchain.toml, so the image is
# built by the same compiler a local build and CI use.
FROM rust:1.100.0-slim AS builder

# ca-certificates is all the build needs. The dependency tree is clap alone, so
# there is no OpenSSL to link against and no libpq to find with pkg-config.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set work directory
WORKDIR /usr/src/app

# Copy all source files at once (simpler and more reliable)
COPY app/ ./

# Build the application
# --locked builds from the committed Cargo.lock, so an image can never pick up
# a dependency version that nobody reviewed.
RUN cargo build --release --locked

# Runtime stage
# The binary needs nothing beyond the system C library, so there is no runtime
# package to install here.
FROM debian:bookworm-slim

# Create a non-root user
# nologin rather than bash: a service account needs no shell, and /bin/bash is
# not installed in a slim image anyway.
RUN useradd -r -u 1000 -m -c "Maze user" -d /home/maze -s /usr/sbin/nologin maze

# Set work directory
WORKDIR /home/maze

# Copy the binary from builder stage
COPY --from=builder /usr/src/app/target/release/maze /usr/local/bin/maze

# Switch to non-root user
USER maze

# Set environment variables
ENV PATH="/usr/local/bin:${PATH}"

# Expose any ports if needed (not applicable for CLI tool)
# EXPOSE 8080

# Default command - show help
CMD ["maze", "--help"]

# Metadata
LABEL maintainer="nettrash" \
      description="Maze Generator - A tool for generating mazes" \
      version="0.0.1" \
      org.opencontainers.image.title="maze" \
      org.opencontainers.image.description="Maze Generator" \
      org.opencontainers.image.version="0.0.1" \
      org.opencontainers.image.source="https://github.com/nettrash/maze-generator" \
      org.opencontainers.image.licenses="MIT"
