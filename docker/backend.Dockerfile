# Build stage
FROM rust:1.75-slim AS builder
WORKDIR /app
# Install dependencies
RUN apt-get update && apt-get install -y pkg-config
# Copy workspace
COPY . .
# Build release binary
RUN cargo build --release -p drafftink-backend

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/drafftink-backend /app/drafftink-backend
EXPOSE 8080
CMD ["/app/drafftink-backend"]
