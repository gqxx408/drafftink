# Build stage
FROM rust:1.75-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p drafftink-gateway

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/drafftink-gateway /app/drafftink-gateway
EXPOSE 80 443
CMD ["/app/drafftink-gateway"]
