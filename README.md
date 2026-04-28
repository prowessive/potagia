# Potagia

Potagia is a CMS written in Rust.

## Run

1. Install Rust.
2. Clone the repository.
    ```bash
    git clone https://github.com/yourusername/potagia.git
    cd potagia
    ```
3. Prepare a configuration file.
    ```bash
    cp config/.env.template config/.env
    ```
    Modify the new configuration file config/.env
4. Run the postgres server.
    ```bash
    docker compose --env-file config/.env -f devops/docker-compose.yml up
    ```
5. Import schema.
    ```bash
    source config/.env
    export DATABASE_URL="postgres://$DB_USER:$DB_PASS@$DB_HOST/$DB_NAME"
    cargo run --bin import_json -- \
      --json-path examples/aqui_se_come_bien/db/aqui_se_come_bien.json \
      --database potagia
    ```
6. Import files from a folder into `potagia` tables (`paths`, `permissions`, `files`).
    ```bash
    source config/.env
    export DATABASE_URL="postgres://$DB_USER:$DB_PASS@$DB_HOST/potagia"
    cargo run --bin import_files -- \
      --source-path examples/aqui_se_come_bien/web
    ```
7. Build and run the application.
```bash
cargo run
```

Server starts at `http://127.0.0.1:3000`.