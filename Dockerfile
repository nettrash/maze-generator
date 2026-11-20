# Build stage. The patch version matches rust-toolchain.toml, so the image is
# built by the same compiler a local build and CI use. The Debian suite matches
# the runtime stage: the binary links against the builder's glibc, so a newer
# builder suite could bind symbols the runtime's older glibc does not have.
#
# The slim image already carries gcc, libc6-dev and ca-certificates, which is
# everything the build needs. The dependency tree is clap alone, so there is no
# OpenSSL to link against and no libpq to find with pkg-config.
FROM rust:1.100.0-slim-trixie AS builder

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
FROM debian:trixie-slim

# Create a non-root user
# nologin rather than bash: a service account needs no shell, and /bin/bash is
# not installed in a slim image anyway. No -r: a system account would take its
# group from the system range (gid 999), and useradd warns that uid 1000 is
# above SYS_UID_MAX. This way the user and its group are both 1000.
RUN useradd -u 1000 -m -c "Maze user" -d /home/maze -s /usr/sbin/nologin maze

# Set work directory
WORKDIR /home/maze

# Copy the binary from builder stage
COPY --from=builder /usr/src/app/target/release/maze /usr/local/bin/maze

# The MIT license travels with the binary, as its terms ask.
COPY LICENSE /usr/share/doc/maze/LICENSE

# Switch to non-root user
USER maze

# Set environment variables
ENV PATH="/usr/local/bin:${PATH}"

# Expose any ports if needed (not applicable for CLI tool)
# EXPOSE 8080

# Default command - show help
CMD ["maze", "--help"]

# Metadata
# No version or org.opencontainers.image.* labels here: docker/metadata-action
# sets the OCI ones in CI (version, title, description, source, licenses,
# revision) and overrides any the Dockerfile has, so hard-coding them only made
# them drift. A version bump had to edit this file, and a forgotten one left
# the image claiming the previous release.
LABEL maintainer="nettrash" \
      description="Maze Generator - A tool for generating mazes"
