# Potagia

Potagia is a web application for creating and operating different kinds of management projects through configurable content stored in a database. Project administrators can define content types, fields, relationships, workflows, permissions, and views without developing a separate application for each management domain.

The [application specifications](magia/specifications.md) describe the target product. The current Rust and Axum prototype initializes a PostgreSQL schema, seeds its initial web content, and serves that content from the database. The configurable management features are not implemented yet.

## Run locally

### Prerequisites

Install Docker Engine with Docker Compose and grant your user access to the Docker daemon.

### Start the application

1. Clone the repository and open its root directory.
2. Copy the environment template:

   ```bash
   cp .env.example .env
   ```

3. Create `.secrets/postgres_password` containing a non-empty password for the local database. Both `.env` and `.secrets/` are excluded from version control.
4. Build and start the application and database:

   ```bash
   docker compose up --detach --build --wait
   ```

5. Open [http://localhost:3000](http://localhost:3000). The root page displays the seeded Potagia content. If you changed `APP_PORT` in `.env`, use that port instead.
6. Stop the services when you finish:

   ```bash
   docker compose down
   ```

The first startup initializes the schema and seeds the database. Later startups preserve the content in the `potagia_postgres_data` Docker volume.

## Run tests

The complete test suite requires a Unix host, Rust 1.99.1, Docker Compose v2.24.4 or later, and access to the Docker daemon.

Run all Rust and Docker Compose integration tests:

```bash
cargo test --all-targets --locked
```

The integration tests build the application and start isolated, disposable Compose stacks. They remove their own containers, networks, volumes, and test images after execution and do not use the local development volume.

To run only the library unit tests without starting Docker:

```bash
cargo test --lib --locked
```

See the [integration testing guide](doc/integration-testing.md) for coverage, prerequisites, and failure recovery.
