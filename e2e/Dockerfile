# syntax=docker/dockerfile:1
# Reusable Linux/X11 environment for Python GUI E2E tests.
#
# Build:
#   docker build -f e2e/Dockerfile -t xmms-renascene-pye2e .
# Run against the checked-out source tree:
#   docker run --rm -v "$PWD:/workspace" xmms-renascene-pye2e

FROM rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive \
    DISPLAY=:99 \
    GDK_BACKEND=x11 \
    GDK_DISABLE=gl \
    GSK_RENDERER=cairo \
    NO_AT_BRIDGE=1 \
    XMMS_E2E_SCREENSHOT_DIR=/testoutput

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        dbus-x11 \
        imagemagick \
        libasound2-dev \
        libcairo2-dev \
        libgdk-pixbuf-2.0-dev \
        libglib2.0-dev \
        libgstreamer-plugins-base1.0-dev \
        libgstreamer1.0-dev \
        libgtk-4-dev \
        pkg-config \
        python3 \
        python3-venv \
        x11-apps \
        x11-utils \
        xauth \
        xdotool \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /testoutput && chmod 0777 /testoutput

WORKDIR /workspace

COPY e2e/docker-entrypoint.sh /usr/local/bin/xmms-e2e-entrypoint
RUN chmod +x /usr/local/bin/xmms-e2e-entrypoint

ENTRYPOINT ["/usr/local/bin/xmms-e2e-entrypoint"]
CMD ["./repo", "pye2e", "-q"]
