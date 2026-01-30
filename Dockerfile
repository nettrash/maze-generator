# Build stage
FROM rust:1.88-slim AS builder

# Install system dependencies needed for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set work directory
WORKDIR /usr/src/app

# Copy all source files at once (simpler and more reliable)
COPY app/ ./

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpq5 \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -u 1000 -m -c "Maze user" -d /home/maze -s /bin/bash maze

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