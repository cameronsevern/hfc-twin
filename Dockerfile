# One image for the twin. All application code talks to Postgres and nothing cloud-specific (invariant 7).
FROM rust:1.99.1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p mcp-server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /src/target/release/hfc-twin-mcp /usr/local/bin/hfc-twin-mcp
COPY fixtures ./fixtures
COPY ml/artifacts/proposal_gbdt.json ./proposal_gbdt.json
# Task 24: the model for plant with version 2 parts. Each plant gets the model trained on its kind.
COPY ml/artifacts/as_built/gbdt_maps.json ./as_built_gbdt.json
EXPOSE 8484
CMD ["hfc-twin-mcp", "--http", "0.0.0.0:8484", "--model", "proposal_gbdt.json", "--model", "as_built_gbdt.json"]
