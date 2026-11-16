# Ubuntu 22.04 supplies glibc 2.35 for the distributed Linux binaries.
FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential ca-certificates cmake git pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
