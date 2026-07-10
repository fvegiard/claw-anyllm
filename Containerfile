FROM rust:bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        libssl-dev \
        pkg-config \
        python3 \
        python3-pip \
        python3-venv \
        tmux \
        zsh \
        nodejs \
        npm \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_TERM_COLOR=always
ENV CLAW_VM=1
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace
RUN mkdir -p /workspace/projects

# Pre-install orchestrator deps when copied into image builds
COPY examples/agent-sdk-orchestrator /opt/claw-orchestrator
RUN cd /opt/claw-orchestrator && npm install --omit=dev 2>/dev/null || npm install || true

ENV CLAW_AGENT_SDK_ORCHESTRATOR=/opt/claw-orchestrator

CMD ["zsh", "-l"]
